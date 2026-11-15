use clap::Parser;
use nexus_core::config::{ColorMode, DisplayMode, NexusConfig};
use nexus_core::context::{ContextBudget, InterruptedOutputManager, MacroCompactor};
use nexus_core::events::NexusEvent;
use nexus_core::git::GitSnapshotManager;
use nexus_core::gstack::{GStackEngine, GStackReviewMode};
use nexus_core::memory::MemoryStore;
use nexus_core::plugins::PluginManager;
use nexus_core::provider::anthropic::AnthropicProvider;
use nexus_core::provider::LlmProvider;
use nexus_core::rea::ReaBridge;
use nexus_core::rust_specialist::{CheckMode, RustCheckRunner};
use nexus_core::sandbox::Sandbox;
use nexus_core::session::{Message, SessionStore};
use nexus_core::skills::{SkillManager, SkillWatcher};
use nexus_core::tools::ToolManager;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

mod tui;
use tui::{ThemePalette, TuiRenderer};

pub type Theme = ThemePalette;

#[derive(Parser, Debug)]
#[command(
    name = "nexus",
    about = "Nexus: High-Throughput Polyglot & Rust AI Coding Harness"
)]
struct Cli {
    /// Resume a conversation by UUID
    #[arg(long)]
    conversation: Option<String>,

    /// Continue the most recent conversation
    #[arg(short = 'c', long)]
    continue_session: bool,

    /// Working directory for the workspace (defaults to current dir)
    #[arg(short, long)]
    workspace: Option<PathBuf>,

    /// Model to use for generation
    #[arg(long, default_value = "claude-3-7-sonnet-20250219")]
    model: String,

    /// Automatically approve all tool execution requests
    #[arg(long)]
    dangerously_skip_permissions: bool,

    /// Search cross-session memory and exit
    #[arg(short = 's', long)]
    search: Option<String>,

    /// Launch interactive setup wizard (theme, text mode, provider)
    #[arg(long)]
    setup: bool,
}

#[cfg(windows)]
fn trim_working_set() {
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(
            hProcess: isize,
            dwMinimumWorkingSetSize: usize,
            dwMaximumWorkingSetSize: usize,
        ) -> i32;
    }
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[cfg(not(windows))]
fn trim_working_set() {}

fn get_process_memory_kb() -> Option<usize> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct PROCESS_MEMORY_COUNTERS {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }

        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn K32GetProcessMemoryInfo(
                hProcess: isize,
                ppsmemCounters: *mut PROCESS_MEMORY_COUNTERS,
                cb: u32,
            ) -> i32;
        }

        unsafe {
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 {
                Some(counters.working_set_size / 1024)
            } else {
                None
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
            Some(pages * 4)
        } else {
            None
        }
    }
}

fn try_import_claude_settings() -> Option<NexusConfig> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    let claude_settings = home.join(".claude").join("settings.json");
    if claude_settings.exists() {
        if let Ok(content) = std::fs::read_to_string(&claude_settings) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(env_obj) = val.get("env") {
                    let token = env_obj
                        .get("ANTHROPIC_AUTH_TOKEN")
                        .or_else(|| env_obj.get("ANTHROPIC_API_KEY"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let base_url = env_obj
                        .get("ANTHROPIC_BASE_URL")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let model = env_obj
                        .get("ANTHROPIC_MODEL")
                        .and_then(|v| v.as_str())
                        .unwrap_or("claude-3-7-sonnet-20250219")
                        .to_string();

                    if !token.is_empty() {
                        return Some(NexusConfig {
                            color_mode: ColorMode::Claude,
                            display_mode: DisplayMode::Rich,
                            api_key: token,
                            base_url,
                            model,
                        });
                    }
                }
            }
        }
    }
    None
}

fn run_setup_wizard() -> io::Result<NexusConfig> {
    println!("\x1b[1;36m====================================================\x1b[0m");
    println!("\x1b[1;36m       Welcome to Nexus: Setup & Configuration      \x1b[0m");
    println!("\x1b[1;36m====================================================\x1b[0m\n");

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    // 1. Color Palette / Theme
    println!("\x1b[1;38;2;217;107;39m[1/4] Select Visual Palette & Theme:\x1b[0m");
    println!("  1) \x1b[38;2;217;107;39mClaude Terracotta\x1b[0m (Iconic Anthropic warm coral & terracotta) [Default]");
    println!("  2) \x1b[38;2;0;245;255mCyberpunk Neon\x1b[0m (Electric cyan, hot pink & cyber yellow)");
    println!("  3) \x1b[38;2;122;162;247mTokyo Night\x1b[0m (Calm deep indigo, mint & lavender)");
    println!("  4) \x1b[38;2;255;176;0mDec Amber CRT\x1b[0m (Classic retro monochrome amber CRT glow)");
    println!("  5) \x1b[38;2;136;192;208mNordic Frost\x1b[0m (Arctic ice blue, snow white & dark slate)");
    println!("  6) \x1b[38;2;203;75;22mSolarized Light\x1b[0m (Clean warm paper for bright terminals)");
    println!("  7) Plain Text (Monochrome, zero ANSI codes for logging)");
    print!("Choose [1-7] (default 1): ");
    io::stdout().flush()?;
    let mut choice = String::new();
    reader.read_line(&mut choice)?;
    let color_mode = match choice.trim() {
        "2" => ColorMode::Cyberpunk,
        "3" => ColorMode::TokyoNight,
        "4" => ColorMode::Amber,
        "5" => ColorMode::Nordic,
        "6" => ColorMode::Light,
        "7" => ColorMode::Plain,
        _ => ColorMode::Claude,
    };

    // 2. Display Mode
    println!("\n\x1b[1;33m[2/4] Select Display Mode:\x1b[0m");
    println!("  1) Rich (Syntax highlighting, status chips, formatted banners) [Default]");
    println!("  2) Minimal (Streamlined, compact output)");
    print!("Choose [1-2] (default 1): ");
    io::stdout().flush()?;
    choice.clear();
    reader.read_line(&mut choice)?;
    let display_mode = match choice.trim() {
        "2" => DisplayMode::Minimal,
        _ => DisplayMode::Rich,
    };

    // 3. Provider Selection
    println!("\n\x1b[1;33m[3/4] Select LLM Provider:\x1b[0m");
    println!("  1) Auto-import from Claude Code (~/.claude/settings.json) [Recommended]");
    println!("  2) Anthropic Claude (Direct API Key: sk-ant-...)");
    println!("  3) Custom Endpoint / Proxy / AI Credits (Base URL + Auth Token)");
    println!("  4) Local LLM / Ollama (http://localhost:11434/v1)");
    print!("Choose [1-4] (default 1): ");
    io::stdout().flush()?;
    choice.clear();
    reader.read_line(&mut choice)?;

    let (api_key, base_url, mut default_model) = match choice.trim() {
        "2" => {
            print!("\nEnter Anthropic API Key (sk-ant-...): ");
            io::stdout().flush()?;
            let mut key_input = String::new();
            reader.read_line(&mut key_input)?;
            (
                key_input.trim().to_string(),
                None,
                "claude-3-7-sonnet-20250219".to_string(),
            )
        }
        "3" => {
            print!(
                "\nEnter Base URL (e.g. https://api.aicredits.in or https://openrouter.ai/api/v1): "
            );
            io::stdout().flush()?;
            let mut url_input = String::new();
            reader.read_line(&mut url_input)?;
            let trimmed_url = url_input.trim();
            let b_url = if !trimmed_url.is_empty() {
                Some(trimmed_url.to_string())
            } else {
                None
            };

            print!("Enter Auth Token / API Key: ");
            io::stdout().flush()?;
            let mut key_input = String::new();
            reader.read_line(&mut key_input)?;

            print!(
                "Enter Default Model (e.g. z-ai/glm-5.3-flash or anthropic/claude-3.5-sonnet): "
            );
            io::stdout().flush()?;
            let mut model_input = String::new();
            reader.read_line(&mut model_input)?;
            let trimmed_model = model_input.trim();
            let model = if !trimmed_model.is_empty() {
                trimmed_model.to_string()
            } else {
                "claude-3-7-sonnet-20250219".to_string()
            };

            (key_input.trim().to_string(), b_url, model)
        }
        "4" => (
            "ollama".to_string(),
            Some("http://localhost:11434/v1".to_string()),
            "qwen2.5-coder:latest".to_string(),
        ),
        _ => {
            if let Some(claude_cfg) = try_import_claude_settings() {
                println!("\x1b[1;32m✓ Successfully imported credentials from Claude Code!\x1b[0m");
                (claude_cfg.api_key, claude_cfg.base_url, claude_cfg.model)
            } else {
                println!("\x1b[0;33mNo ~/.claude/settings.json found. Please enter your API Key directly:\x1b[0m");
                print!("Enter API Key: ");
                io::stdout().flush()?;
                let mut key_input = String::new();
                reader.read_line(&mut key_input)?;
                (
                    key_input.trim().to_string(),
                    None,
                    "claude-3-7-sonnet-20250219".to_string(),
                )
            }
        }
    };

    if choice.trim() != "3" && choice.trim() != "4" {
        println!("\n\x1b[1;33m[4/4] Default Model:\x1b[0m");
        println!("  1) claude-3-7-sonnet-20250219 [Default]");
        println!("  2) claude-3-5-sonnet-20241022");
        println!("  3) claude-3-5-haiku-20241022");
        println!("  4) Custom (enter manually)");
        print!("Choose [1-4] (default 1): ");
        io::stdout().flush()?;
        choice.clear();
        reader.read_line(&mut choice)?;
        match choice.trim() {
            "2" => default_model = "claude-3-5-sonnet-20241022".to_string(),
            "3" => default_model = "claude-3-5-haiku-20241022".to_string(),
            "4" => {
                print!("Enter model name: ");
                io::stdout().flush()?;
                let mut m_in = String::new();
                reader.read_line(&mut m_in)?;
                if !m_in.trim().is_empty() {
                    default_model = m_in.trim().to_string();
                }
            }
            _ => {}
        }
    }

    let config = NexusConfig {
        color_mode,
        display_mode,
        api_key,
        base_url,
        model: default_model,
    };

    let _ = config.save();
    println!("\n\x1b[1;32m✓ Setup complete! Configuration saved to ~/.nexus/config.json\x1b[0m\n");
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let workspace_dir = cli
        .workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Initialize session database in the workspace root (.nexus/sessions.db)
    let nexus_dir = workspace_dir.join(".nexus");
    std::fs::create_dir_all(&nexus_dir)?;
    let session_store = SessionStore::new(nexus_dir.join("sessions.db"))?;

    // Initialize cross-session distilled memory store
    let memory_store = MemoryStore::open(&nexus_dir.join("memory.db"))?;

    // Handle immediate search CLI flag
    if let Some(query) = cli.search {
        println!(
            "\x1b[1;36m[Memory Recall]\x1b[0m Searching cross-session memory for: '{}'",
            query
        );
        let results = memory_store.search(&query, 10)?;
        if results.is_empty() {
            println!("No matching historical memories found.");
        } else {
            println!("Found {} relevant historical records:\n", results.len());
            for (i, r) in results.iter().enumerate() {
                println!(
                    "{}. \x1b[1;33m[{}]\x1b[0m \x1b[1m{}\x1b[0m (score: {:.2})\n   {}\n",
                    i + 1,
                    r.source.to_uppercase(),
                    r.topic_or_role,
                    r.score,
                    r.snippet
                );
            }
        }
        return Ok(());
    }

    // Determine session ID
    let mut session = if let Some(id) = cli.conversation {
        println!("Resuming session: {}", id);
        session_store.load_session(&id)?
    } else {
        session_store.create_session("Nexus CLI Session")?
    };

    // Initialize security sandbox
    let sandbox = Sandbox::new(&workspace_dir, cli.dangerously_skip_permissions)?;
    let mut tool_manager_builder =
        ToolManager::new(sandbox.clone()).with_memory_store(memory_store.clone(), &session.id);

    // Auto-connect rust-analyzer if a Rust workspace exists
    if workspace_dir.join("Cargo.toml").exists() {
        if let Ok(client) = nexus_lsp::LspClient::start("rust-analyzer", &workspace_dir).await {
            println!("\x1b[1;32m[LSP]\x1b[0m Connected to rust-analyzer");
            tool_manager_builder =
                tool_manager_builder.with_lsp_client(Arc::new(tokio::sync::Mutex::new(client)));
        }
    }
    let tool_manager = Arc::new(tool_manager_builder);

    // Initialize dynamic Skill Watcher
    let (skill_tx, mut skill_rx) = mpsc::channel(16);
    let _skill_watcher = SkillWatcher::start(&workspace_dir, skill_tx).ok();

    // Load or initialize user configuration (Theme, Display Mode, Provider)
    let mut config = if cli.setup {
        run_setup_wizard()?
    } else if let Some(loaded) = NexusConfig::load_from_workspace_or_global(&workspace_dir) {
        loaded
    } else {
        let has_env = std::env::var("ANTHROPIC_API_KEY").is_ok()
            || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();

        if !has_env {
            println!(
                "\x1b[1;36mNo existing configuration detected. Launching setup wizard...\x1b[0m\n"
            );
            run_setup_wizard()?
        } else {
            NexusConfig::default()
        }
    };

    // Environment and CLI flag overrides
    if let Ok(key) =
        std::env::var("ANTHROPIC_API_KEY").or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
    {
        config.api_key = key.trim().to_string();
    }
    if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
        config.base_url = Some(url);
    }
    if cli.model != "claude-3-7-sonnet-20250219" {
        config.model = cli.model.clone();
    }

    let mut theme = Theme::for_mode(config.color_mode);

    // Load plugins (workspace .nexus/plugins + Claude Code plugins)
    let plugins = PluginManager::load_all(&workspace_dir).unwrap_or_default();

    let git_branch = GitSnapshotManager::current_branch(&workspace_dir);
    TuiRenderer::render_banner(
        &config,
        &workspace_dir,
        git_branch.as_deref(),
        get_process_memory_kb(),
        ReaBridge::is_available(),
        plugins.len(),
        &theme,
    );

    let final_key = if config.api_key.is_empty() {
        "DUMMY_KEY_FOR_LOCAL_TEST".to_string()
    } else {
        config.api_key.clone()
    };

    let mut provider_builder = AnthropicProvider::new(final_key, Some(config.model.clone()));
    if let Some(ref b) = config.base_url {
        provider_builder = provider_builder.with_base_url(b.clone());
    }

    let provider = Arc::new(provider_builder);
    let tool_defs = ToolManager::tool_definitions();

    // Load architecture invariants from AGENTS.md (or default standards)
    let invariants = GStackEngine::load_invariants(&workspace_dir);
    let invariants_prompt = GStackEngine::format_architecture_prompt(&invariants);
    let plugins_prompt = PluginManager::format_plugins_prompt(&plugins);

    let mut base_system_prompt = format!(
        "You are Nexus, an elite autonomous AI coding harness built in native Rust.
You specialize in writing clean, modular, and robust software with an emphasis on idiomatic code, clean architecture, and precision edits.
Your workspace is at: {}.

### Autonomous Tool & CLI Execution Guidelines:
1. You have direct access to tools: `run_command` (shell execution), `read_file`, `write_file`, `edit_file`, `cargo_check`, and `lsp_*`.
2. Be proactive: when the user asks you to inspect, build, test, refactor, or fix an issue, execute the appropriate tools directly rather than merely giving conversational advice.
3. Always inspect files using `read_file` before modifying them.
4. Prefer focused surgical edits using `edit_file` over overwriting entire files. Preserve all existing comments, docstrings, tests, and formatting intact.
5. After applying code changes, proactively verify your work by running `cargo_check` or `run_command` (test runner/linter) to confirm zero compiler errors or regressions before completing your response.
6. Write complete, production-grade code. Never omit code, leave placeholders, or write lazy `// TODO` stubs.

{}",
        workspace_dir.display(),
        invariants_prompt
    );

    if !plugins_prompt.is_empty() {
        base_system_prompt.push_str("\n\n");
        base_system_prompt.push_str(&plugins_prompt);
    }

    let mut session_tokens = nexus_core::events::TokenUsage::default();
    let context_budget = ContextBudget::default();
    let mut pending_interrupted_output: Option<String> = None;

    loop {
        // Non-blocking check for hotloaded skill notifications
        while let Ok(change) = skill_rx.try_recv() {
            println!(
                "\x1b[1;36m[Hotload Watcher]\x1b[0m Skill filesystem change detected: {:?}",
                change
            );
        }

        let cur_tokens = MacroCompactor::estimate_history_tokens(&session.messages);
        let short_pill = if cur_tokens > 0 {
            Some(context_budget.format_short_pill(cur_tokens))
        } else {
            None
        };

        trim_working_set();
        TuiRenderer::render_prompt(
            &workspace_dir,
            git_branch.as_deref(),
            get_process_memory_kb(),
            short_pill.as_deref(),
            &theme,
        );
        io::stdout().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break; // EOF
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "exit" || trimmed == "quit" {
            println!("Goodbye!");
            break;
        }

        // Direct CLI pass-through with '!' prefix (e.g. !git status, !cargo check, !ls)
        if let Some(cmd) = trimmed.strip_prefix('!') {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                println!("{}Usage: !<command> (e.g. !git status, !cargo check){}", theme.dim, theme.reset);
                continue;
            }
            println!("{}╭─ ❯ Shell: {} ────────────────────────────{}", theme.border, cmd, theme.reset);
            let shell_prog = if cfg!(windows) { "cmd" } else { "bash" };
            let shell_arg = if cfg!(windows) { "/C" } else { "-c" };
            let status = std::process::Command::new(shell_prog)
                .arg(shell_arg)
                .arg(cmd)
                .current_dir(&workspace_dir)
                .status();
            match status {
                Ok(s) => {
                    let code_str = s.code().map(|c| c.to_string()).unwrap_or_else(|| "terminated".to_string());
                    let color = if s.success() { "\x1b[32m" } else { "\x1b[31m" };
                    println!("{}╰─ ⟡ Exited: {}{}{} ───────────────────────────────{}", theme.border, color, code_str, theme.border, theme.reset);
                }
                Err(e) => {
                    eprintln!("{}╰─ ⟡ Execution failed: {}{}", theme.error, e, theme.reset);
                }
            }
            continue;
        }

        // Handle slash commands
        if trimmed == "/help" {
            println!("\x1b[1;33mAvailable Commands:\x1b[0m");
            println!("  !<command>              - Execute shell command directly in workspace (zero API tokens)");
            println!("  /doctor                 - Run compiler diagnostic check and trigger targeted repair");
            println!("  /compact [focus]        - Compact session turns into structured summary (<context_summary>)");
            println!("  /context                - Show context window usage, capacity, and auto-compact threshold");
            println!("  /continue               - Resume interrupted generation using <interrupted-output> fence");
            println!("  /theme [name]           - Switch visual palette (claude, cyberpunk, tokyo, amber, nordic, light, plain)");
            println!("  /tokens                 - Show real-time prompt cache hit rate and token efficiency metrics");
            println!("  /setup                  - Reconfigure color mode, display mode, provider, and model");
            println!("  /status                 - Show real-time process memory, SQLite cache, and diagnostics");
            println!("  /ceo                    - Run CEO review (challenge assumptions, evaluate 10-star product & scope)");
            println!("  /eng                    - Run Engineering architecture review (invariants, data flow, failure modes)");
            println!(
                "  /qa                     - Run automated QA loop (check, test, fix, re-verify)"
            );
            println!("  /ship                   - Run pre-landing ship verification (clean tree, clippy, test gate)");
            println!("  /skills                 - List all loaded permanent and staged skills");
            println!("  /promote <name>         - Promote a staged skill to permanent storage");
            println!("  /discard <name>         - Discard a staged skill from sandbox quarantine");
            println!("  /recall <query>         - Search past sessions and distilled memory cards");
            println!("  /remember <topic>: <msg>- Save a distilled insight into long-term memory");
            println!("  /bench-memory           - Benchmark memory search throughput and microsecond latency");
            println!("  /benchmark              - Run local air-gapped industry benchmark battery");
            println!("  /rea <path>             - Inspect a native binary via REA CLI");
            println!("  /plugins                - List active plugins and registered plugin slash commands");
            println!("  exit                    - Exit Nexus");

            if !plugins.is_empty() {
                println!("\n\x1b[1;32mRegistered Plugin Slash Commands:\x1b[0m");
                for p in &plugins {
                    for c in &p.commands {
                        println!(
                            "  /{:<22} - {} (from plugin: {})",
                            c.name, c.description, p.name
                        );
                    }
                }
            }
            continue;
        }

        if trimmed == "/theme" || trimmed.starts_with("/theme ") {
            let arg = trimmed.strip_prefix("/theme").unwrap_or("").trim();
            if arg.is_empty() {
                println!("\x1b[1;38;2;217;107;39mAvailable Themes:\x1b[0m");
                println!("  \x1b[38;2;217;107;39m/theme claude\x1b[0m     - Anthropic Terracotta & Warm Coral [Default]");
                println!("  \x1b[38;2;0;245;255m/theme cyberpunk\x1b[0m  - Electric Cyan, Hot Pink & Cyber Gold");
                println!("  \x1b[38;2;122;162;247m/theme tokyo\x1b[0m      - Tokyo Night Indigo, Mint & Lavender");
                println!("  \x1b[38;2;255;176;0m/theme amber\x1b[0m      - Classic Amber CRT Glow");
                println!("  \x1b[38;2;136;192;208m/theme nordic\x1b[0m     - Arctic Ice & Deep Slate");
                println!("  \x1b[38;2;203;75;22m/theme light\x1b[0m      - Solarized Paper for Light Terminals");
                println!("  /theme plain      - Monochrome Plain Text");
            } else {
                let new_mode = match arg.to_lowercase().as_str() {
                    "cyberpunk" | "neon" => ColorMode::Cyberpunk,
                    "tokyo" | "tokyonight" => ColorMode::TokyoNight,
                    "amber" | "crt" => ColorMode::Amber,
                    "nordic" | "frost" | "nord" => ColorMode::Nordic,
                    "light" | "paper" => ColorMode::Light,
                    "plain" | "none" => ColorMode::Plain,
                    _ => ColorMode::Claude,
                };
                config.color_mode = new_mode;
                theme = Theme::for_mode(new_mode);
                if let Err(err) = config.save() {
                    eprintln!("Warning: failed to persist theme config: {}", err);
                }
                println!(
                    "{}Theme switched to: {}{}",
                    theme.brand_primary, theme.name, theme.reset
                );
            }
            continue;
        }

        if trimmed == "/setup" {
            match run_setup_wizard() {
                Ok(new_cfg) => {
                    config = new_cfg;
                    theme = Theme::for_mode(config.color_mode);
                    println!(
                        "{}Active configuration updated! Theme: {}, Display: {:?}, Model: {}{}",
                        theme.brand_primary,
                        theme.name,
                        config.display_mode,
                        config.model,
                        theme.reset
                    );
                }
                Err(err) => eprintln!("{}Setup error: {}{}", theme.error, err, theme.reset),
            }
            continue;
        }

        if trimmed == "/status" || trimmed == "/memory" {
            let mem_str = if let Some(kb) = get_process_memory_kb() {
                format!("{:.2} MB ({} KB)", kb as f64 / 1024.0, kb)
            } else {
                "Unknown".to_string()
            };
            println!(
                "{}--- Nexus Process Diagnostics ---{}",
                theme.brand_primary, theme.reset
            );
            println!("  Process Memory: {}", mem_str);
            println!("  Session ID:     {}", session.id);
            println!("  Model:          {}", config.model);
            println!("  Theme:          {} ({:?})", theme.name, config.color_mode);
            println!("  Display Mode:   {:?}", config.display_mode);
            println!("  SQLite Cache:   256 KB");
            println!("  Note: Windows Terminal DirectX GPU renderer uses ~50 MB separately for the window frame.");
            println!(
                "{}---------------------------------{}",
                theme.brand_primary, theme.reset
            );
            continue;
        }

        if trimmed == "/tokens" || trimmed == "/cost" {
            TuiRenderer::render_token_dashboard(&session_tokens, &theme);
            continue;
        }

        if trimmed == "/compact" || trimmed.starts_with("/compact ") {
            let arg = trimmed.strip_prefix("/compact").unwrap_or("").trim();
            let focus = if arg.is_empty() { None } else { Some(arg) };
            println!("{}╭─ 🗜️ Nexus Context Compactor ───────────────────────────{}", theme.border, theme.reset);
            let current_tokens = MacroCompactor::estimate_history_tokens(&session.messages);
            println!(
                "{}│{} Current history: {} messages (~{} tokens)",
                theme.border, theme.reset, session.messages.len(), current_tokens
            );
            if session.messages.is_empty() {
                println!("{}│{} History is empty, nothing to compact.{}", theme.border, theme.reset, theme.reset);
                println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
                continue;
            }

            let (new_history, summary) = MacroCompactor::compact_history(
                &session.messages,
                &workspace_dir,
                focus,
                2,
            );

            session.messages = new_history;
            if let Err(err) = session_store.rewrite_session_messages(&session.id, &session.messages) {
                eprintln!("{}│{} Warning: Failed to persist compacted session: {}{}", theme.border, theme.reset, err, theme.reset);
            }

            println!(
                "{}│{} \x1b[1;32m✓ Compacted {} turns into synthesized boundary\x1b[0m",
                theme.border, theme.reset, summary.compacted_turns_count
            );
            println!(
                "{}│{} Tokens: {} → {} ({:.1}% reduction)",
                theme.border, theme.reset,
                summary.pre_compact_tokens,
                summary.post_compact_tokens,
                summary.reduction_percentage()
            );
            println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
            continue;
        }

        if trimmed == "/context" || trimmed == "/budget" {
            let current_tokens = MacroCompactor::estimate_history_tokens(&session.messages);
            let pill = context_budget.format_status_pill(current_tokens);
            println!("{}╭─ 🧠 Nexus Context Engineering ─────────────────────────{}", theme.border, theme.reset);
            println!("{}│{} Context Utilization: {}", theme.border, theme.reset, pill);
            println!("{}│{} Active Turns:       {} messages", theme.border, theme.reset, session.messages.len());
            println!("{}│{} Window Capacity:    {} tokens", theme.border, theme.reset, context_budget.window_capacity);
            println!("{}│{} Auto-compact at:    {} tokens ({:.0}%)", theme.border, theme.reset, context_budget.threshold_tokens(), context_budget.compact_threshold_pct * 100.0);
            println!("{}│{} Manual Compaction:  Run /compact [optional focus]", theme.border, theme.reset);
            println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
            continue;
        }

        if trimmed == "/plugins" {
            println!("\x1b[1;36mActive Plugins:\x1b[0m");
            if plugins.is_empty() {
                println!("\x1b[0;33mNo plugins found in workspace (.nexus/plugins/) or Claude Code cache.\x1b[0m");
            } else {
                for p in &plugins {
                    println!(
                        "  - \x1b[1m{}\x1b[0m (v{}): {}",
                        p.name, p.version, p.description
                    );
                    for c in &p.commands {
                        println!("      \x1b[1;32m/{}\x1b[0m: {}", c.name, c.description);
                    }
                }
            }
            continue;
        }

        if trimmed == "/skills" {
            let loaded = SkillManager::load_all(&workspace_dir).unwrap_or_default();
            if loaded.is_empty() {
                println!("\x1b[0;33mNo specialized skills installed in workspace.\x1b[0m");
            } else {
                println!("\x1b[1;36mActive Workspace Skills:\x1b[0m");
                for s in loaded {
                    let status = if s.is_staged {
                        "\x1b[1;33m[STAGED TEST-DRIVE]\x1b[0m"
                    } else {
                        "\x1b[1;32m[PERMANENT]\x1b[0m"
                    };
                    println!("  - \x1b[1m{}\x1b[0m {}: {}", s.name, status, s.description);
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("/promote ") {
            let skill_name = rest.trim();
            match SkillManager::promote_skill(&workspace_dir, skill_name) {
                Ok(_) => {
                    println!(
                        "\x1b[1;32m[Promoted]\x1b[0m Skill '{}' moved to permanent storage (.nexus/skills/{}).",
                        skill_name, skill_name
                    );
                }
                Err(err) => {
                    eprintln!("\x1b[1;31m[Error promoting skill]\x1b[0m {}", err);
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("/discard ") {
            let skill_name = rest.trim();
            match SkillManager::discard_skill(&workspace_dir, skill_name) {
                Ok(_) => {
                    println!(
                        "\x1b[1;33m[Discarded]\x1b[0m Staged skill '{}' removed from quarantine.",
                        skill_name
                    );
                }
                Err(err) => {
                    eprintln!("\x1b[1;31m[Error discarding skill]\x1b[0m {}", err);
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("/recall ") {
            let query = rest.trim();
            match memory_store.search(query, 5) {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No matching historical memories found for '{}'.", query);
                    } else {
                        println!("\x1b[1;36mFound {} memory results:\x1b[0m", results.len());
                        for (i, r) in results.iter().enumerate() {
                            println!(
                                "{}. \x1b[1;33m[{}]\x1b[0m \x1b[1m{}\x1b[0m (score: {:.2})\n   {}\n",
                                i + 1,
                                r.source.to_uppercase(),
                                r.topic_or_role,
                                r.score,
                                r.snippet
                            );
                        }
                    }
                }
                Err(err) => eprintln!("\x1b[1;31m[Memory Search Error]\x1b[0m {}", err),
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("/remember ") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                let topic = parts[0].trim();
                let content = parts[1].trim();
                match memory_store.save_distillation(&session.id, topic, content) {
                    Ok(mem) => println!(
                        "\x1b[1;32m[Remembered]\x1b[0m Distilled '{}' into memory (id: {}).",
                        mem.topic, mem.id
                    ),
                    Err(err) => eprintln!("\x1b[1;31m[Memory Save Error]\x1b[0m {}", err),
                }
            } else {
                println!("Usage: /remember <topic>: <distilled insight note>");
            }
            continue;
        }

        if trimmed == "/bench-memory" {
            let sample_queries = ["borrow checker", "lsp client", "prompt caching", "sandbox"];
            match memory_store.benchmark_search_throughput(&sample_queries) {
                Ok(report) => {
                    println!("\x1b[1;36m[Memory Benchmark Report]\x1b[0m");
                    println!("  Total Queries: {}", report.total_queries);
                    println!("  Total Duration: {} µs", report.total_duration_micros);
                    println!("  Average Latency: {:.2} µs", report.avg_latency_micros);
                    println!("  P99 Latency: {:.2} µs", report.p99_latency_micros);
                }
                Err(err) => eprintln!("\x1b[1;31m[Benchmark Error]\x1b[0m {}", err),
            }
            continue;
        }

        if trimmed == "/benchmark" || trimmed == "/bench" {
            println!("\x1b[1;36m[Benchmark Runner]\x1b[0m Executing local air-gapped industry benchmark battery...");
            let py_cmd = if cfg!(windows) { "python" } else { "python3" };
            let bench_script = workspace_dir.join("benchmarks").join("run_benchmarks.py");
            let script_path = if bench_script.exists() {
                bench_script
            } else {
                Path::new("benchmarks/run_benchmarks.py").to_path_buf()
            };

            let status = std::process::Command::new(py_cmd)
                .arg(&script_path)
                .current_dir(&workspace_dir)
                .status();

            match status {
                Ok(s) => {
                    if s.success() {
                        println!("\x1b[1;32m[Benchmark Complete]\x1b[0m Report generated: BENCHMARK_REPORT.md");
                    } else {
                        eprintln!("\x1b[1;31m[Benchmark Failed]\x1b[0m Non-zero exit code: {:?}", s.code());
                    }
                }
                Err(err) => {
                    eprintln!("\x1b[1;31m[Benchmark Runner Error]\x1b[0m Failed to spawn {}: {}", py_cmd, err);
                    println!("Tip: Run directly via: {} benchmarks/run_benchmarks.py", py_cmd);
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("/rea ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                println!("Usage: /rea <binary-path> [overview|objdump|version]");
                continue;
            }
            let target_bin = Path::new(parts[0]);
            let mode = parts.get(1).copied().unwrap_or("overview");
            match ReaBridge::inspect_binary(target_bin, mode) {
                Ok(out) => println!("\x1b[1;34m[REA Output]\x1b[0m\n{}", out),
                Err(err) => eprintln!("\x1b[1;31m[REA Error]\x1b[0m {}", err),
            }
            continue;
        }

        let mut active_system_prompt = base_system_prompt.clone();

        // Inject active skills into prompt dynamically
        if let Ok(skills) = SkillManager::load_all(&workspace_dir) {
            let skills_prompt = SkillManager::format_skills_prompt(&skills);
            if !skills_prompt.is_empty() {
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&skills_prompt);
            }
        }

        let (cmd_word, cmd_arg) = match trimmed.split_once(' ') {
            Some((w, a)) => (w, Some(a.trim())),
            None => (trimmed, None),
        };

        let prompt_to_send: String = match cmd_word {
            "/doctor" | "/check" => {
                println!("{}╭─ 🩺 Nexus Doctor: Running Diagnostics ─────────────────{}", theme.border, theme.reset);
                println!("{}│{} Checking workspace compilation health...", theme.border, theme.reset);
                let check_res = RustCheckRunner::run_check(
                    &workspace_dir,
                    CheckMode::Check,
                ).await;
                match check_res {
                    Ok(res) => {
                        if res.success {
                            println!("{}│{} \x1b[1;32m✓ Clean build!\x1b[0m 0 compiler errors, {} warnings.{}", theme.border, theme.reset, res.warning_count, theme.reset);
                            println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
                            continue;
                        } else {
                            println!(
                                "{}│{} \x1b[1;31m✗ {} compiler errors found:\x1b[0m{}",
                                theme.border, theme.reset, res.error_count, theme.reset
                            );
                            let mut concise_error_summary = String::new();
                            for (i, diag) in res.diagnostics.iter().filter(|d| d.is_error()).take(3).enumerate() {
                                let loc = diag.spans.first().map(|s| format!("{}:{}:{}", s.file_name, s.line_start, s.column_start)).unwrap_or_else(|| "unknown".to_string());
                                let code = diag.code.as_ref().map(|c| format!("[{}] ", c.code)).unwrap_or_default();
                                println!("{}│{}   {}. \x1b[1m{}\x1b[0m {}{}", theme.border, theme.reset, i + 1, loc, code, diag.message);
                                concise_error_summary.push_str(&format!("{}. {}: {}{}\n", i + 1, loc, code, diag.message));
                                if let Some(tutor) = &diag.tutor_advice {
                                    println!("{}│{}      \x1b[0;36m💡 Tutor: {}\x1b[0m", theme.border, theme.reset, tutor);
                                }
                            }
                            println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
                            println!("{}Initiating targeted precision repair with Nexus...{}", theme.brand_primary, theme.reset);
                            format!(
                                "The compiler reported {} error(s):\n{}\nPlease inspect the affected files, apply the minimal necessary fix using edit_file, and verify the repair by running cargo_check.",
                                res.error_count, concise_error_summary
                            )
                        }
                    }
                    Err(e) => {
                        println!("{}│{} \x1b[1;33m! Note: Cargo check not available in this directory: {}{}", theme.border, theme.reset, e, theme.reset);
                        println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
                        continue;
                    }
                }
            }
            "/ceo" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating CEO Review Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::CeoReview,
                ));
                match cmd_arg {
                    Some(arg) if !arg.is_empty() => arg.to_string(),
                    _ => "Review the current project and plan from the CEO perspective: challenge premises, define the 10-star product, and recommend scope posture.".to_string(),
                }
            }
            "/eng" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating Engineering Review Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::EngReview,
                ));
                match cmd_arg {
                    Some(arg) if !arg.is_empty() => arg.to_string(),
                    _ => "Perform an engineering architecture review: audit single sources of truth, interface contracts, error domains, and failure recovery.".to_string(),
                }
            }
            "/qa" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating QA Loop Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::QaLoop,
                ));
                match cmd_arg {
                    Some(arg) if !arg.is_empty() => arg.to_string(),
                    _ => "Run test suites and compiler checks, identify any failures, patch them atomically via edit_file, and re-verify until 100% green.".to_string(),
                }
            }
            "/ship" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating Ship Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::ShipReview,
                ));
                match cmd_arg {
                    Some(arg) if !arg.is_empty() => arg.to_string(),
                    _ => "Perform pre-landing verification: check git status, test coverage, clippy warnings, and docs/changelog.".to_string(),
                }
            }
            "/continue" | "/resume" => {
                if let Some(interrupted) = pending_interrupted_output.take() {
                    println!("{}╭─ ⟡ Resuming Interrupted Turn ─────────────────────────{}", theme.border, theme.reset);
                    println!("{}│{} Restoring interrupted state with continuation fence...", theme.border, theme.reset);
                    println!("{}╰──────────────────────────────────────────────────────────{}", theme.border, theme.reset);
                    InterruptedOutputManager::wrap_interrupted_output(&interrupted)
                } else {
                    "Please continue the task from where you left off.".to_string()
                }
            }
            _ => {
                // Check if cmd_word matches any registered plugin command
                let matching_plugin_cmd = if let Some(cmd_name) = cmd_word.strip_prefix('/') {
                    plugins.iter().find_map(|p| {
                        p.commands
                            .iter()
                            .find(|c| c.name == cmd_name)
                            .map(|c| (p, c))
                    })
                } else {
                    None
                };

                if let Some((plugin, cmd)) = matching_plugin_cmd {
                    println!(
                        "\x1b[1;35m[Plugin: {}]\x1b[0m Activating /{}",
                        plugin.name, cmd.name
                    );
                    active_system_prompt.push_str(&format!(
                        "\n\n[Active Plugin Command /{}]:\n{}\n",
                        cmd.name, cmd.prompt
                    ));
                    match cmd_arg {
                        Some(arg) if !arg.is_empty() => arg.to_string(),
                        _ => format!(
                            "Activate and execute guidelines for plugin command /{}: {}",
                            cmd.name, cmd.prompt
                        ),
                    }
                } else if cmd_word.starts_with('/') {
                    println!(
                        "\x1b[1;31mUnknown command: {}\x1b[0m Type /help or /plugins to view available commands.",
                        cmd_word
                    );
                    continue;
                } else {
                    trimmed.to_string()
                }
            }
        };

        let mut final_prompt = prompt_to_send;
        if let Some(interrupted) = pending_interrupted_output.take() {
            final_prompt = format!(
                "{}\n\n{}",
                InterruptedOutputManager::wrap_interrupted_output(&interrupted),
                final_prompt
            );
        }

        // Auto-compaction check: if history tokens exceed threshold (75% of 128k = 96k),
        // automatically compact older turns while preserving the last 2 turns.
        if MacroCompactor::should_auto_compact(&session.messages, &context_budget) {
            println!(
                "{}╭─ 🗜️ Context Limit Approaching ────────────────────────{}\n{}│{} History reached {:.1}% of capacity. Compacting older turns...\n{}╰──────────────────────────────────────────────────────────{}",
                theme.border, theme.reset,
                theme.border, theme.reset,
                context_budget.usage_percentage(MacroCompactor::estimate_history_tokens(&session.messages)),
                theme.border, theme.reset
            );
            let (new_history, summary) = MacroCompactor::compact_history(
                &session.messages,
                &workspace_dir,
                None,
                2,
            );
            session.messages = new_history;
            if let Err(err) = session_store.rewrite_session_messages(&session.id, &session.messages) {
                eprintln!("Warning: Failed to persist auto-compacted session: {}", err);
            }
            println!(
                "{}│{} Auto-compacted {} turns ({} → {} tokens, {:.1}% saved){}",
                theme.border,
                theme.reset,
                summary.compacted_turns_count,
                summary.pre_compact_tokens,
                summary.post_compact_tokens,
                summary.reduction_percentage(),
                theme.reset
            );
        }

        // Add user message to session and index in memory
        let user_msg = Message {
            role: "user".to_string(),
            content: final_prompt,
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now(),
        };
        session.messages.push(user_msg.clone());
        let _ = session_store.save_message(&session.id, &user_msg);
        let _ = memory_store.index_turn(&session.id, "user", &user_msg.content, user_msg.timestamp);

        // Turn loop: stream model response, execute tools, repeat until model finishes
        let mut turn_active = true;
        let mut turn_counter = 0;
        let max_tool_turns = 8;
        let mut last_turn_usage: Option<nexus_core::events::TokenUsage> = None;

        while turn_active {
            if turn_counter >= max_tool_turns {
                println!(
                    "{}╭─ ⟡ Turn Limit Reached ───────────────────────────────────{}\n{}│{} Reached safety ceiling of {} tool iterations to protect API budget.\n{}│{} Type your next instruction to continue.\n{}╰──────────────────────────────────────────────────────────{}",
                    theme.border, theme.reset,
                    theme.border, theme.reset, max_tool_turns,
                    theme.border, theme.reset,
                    theme.border, theme.reset
                );
                break;
            }
            turn_counter += 1;
            let turn_id = format!("{}-turn-{}", session.id, turn_counter);

            let (tx, mut rx) = mpsc::channel(64);
            let provider_clone = provider.clone();
            let sys_clone = active_system_prompt.clone();
            let history_clone = session.messages.clone();
            let tools_clone = tool_defs.clone();

            let handle = tokio::spawn(async move {
                provider_clone
                    .stream_turn(&sys_clone, &history_clone, &tools_clone, tx)
                    .await
            });

            let mut current_turn_text = String::new();
            TuiRenderer::render_ai_prefix(&theme);
            io::stdout().flush()?;

            while let Some(event) = rx.recv().await {
                match event {
                    NexusEvent::TokenChunk(chunk) => {
                        print!("{}", chunk);
                        io::stdout().flush()?;
                        current_turn_text.push_str(&chunk);
                    }
                    NexusEvent::ToolCallStarted(ref call) => {
                        TuiRenderer::render_tool_call_start(&call.name, &call.arguments, &theme);
                    }
                    NexusEvent::TokenUsageRecorded(usage) => {
                        session_tokens.input_tokens += usage.input_tokens;
                        session_tokens.output_tokens += usage.output_tokens;
                        session_tokens.cache_read_tokens += usage.cache_read_tokens;
                        session_tokens.cache_creation_tokens += usage.cache_creation_tokens;
                        last_turn_usage = Some(usage);
                    }
                    _ => {}
                }
            }
            println!();

            let assistant_msg = match handle.await? {
                Ok(msg) => msg,
                Err(err) => {
                    eprintln!("{}Provider Error: {}{}", theme.error, err, theme.reset);
                    if !current_turn_text.trim().is_empty() {
                        pending_interrupted_output = Some(current_turn_text);
                        eprintln!(
                            "{}Turn interrupted with partial output. Type /continue to resume without losing progress.{}",
                            theme.warning, theme.reset
                        );
                    }
                    break;
                }
            };

            session.messages.push(assistant_msg.clone());
            let _ = session_store.save_message(&session.id, &assistant_msg);
            let _ = memory_store.index_turn(
                &session.id,
                "assistant",
                &assistant_msg.content,
                assistant_msg.timestamp,
            );

            // Execute tools if requested
            if let Some(calls) = &assistant_msg.tool_calls {
                // Compass: 1 git snapshot per agent turn before executing modifying tools
                if let Ok(Some(snapshot)) =
                    GitSnapshotManager::create_turn_snapshot(&workspace_dir, &turn_id)
                {
                    println!(
                        "{}╭─ ⟡ Git Snapshot ──────────────────────────────{}\n{}│{} Ref: {} (dirty worktree: {})\n{}╰───────────────────────────────────────────────────{}",
                        theme.border,
                        theme.reset,
                        theme.border,
                        theme.reset,
                        &snapshot.commit_or_stash_sha[..8.min(snapshot.commit_or_stash_sha.len())],
                        snapshot.had_uncommitted_changes,
                        theme.border,
                        theme.reset
                    );
                }

                let mut results = Vec::new();
                for call in calls {
                    let result = tool_manager.execute(call).await;
                    TuiRenderer::render_tool_result(&call.name, &result.output, !result.is_error, &theme);
                    results.push(result);
                }

                let tool_result_msg = Message {
                    role: "user".to_string(),
                    content: String::new(),
                    tool_calls: None,
                    tool_results: Some(results),
                    timestamp: chrono::Utc::now(),
                };
                session.messages.push(tool_result_msg.clone());
                let _ = session_store.save_message(&session.id, &tool_result_msg);
            } else {
                turn_active = false;
            }
        }

        let cur_tokens = MacroCompactor::estimate_history_tokens(&session.messages);
        let pill = context_budget.format_status_pill(cur_tokens);
        TuiRenderer::render_turn_footer(get_process_memory_kb(), last_turn_usage.as_ref(), Some(&pill), &theme);
    }

    Ok(())
}
