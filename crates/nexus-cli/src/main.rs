use clap::Parser;
use nexus_core::events::NexusEvent;
use nexus_core::git::GitSnapshotManager;
use nexus_core::gstack::{GStackEngine, GStackReviewMode};
use nexus_core::provider::anthropic::AnthropicProvider;
use nexus_core::provider::LlmProvider;
use nexus_core::sandbox::Sandbox;
use nexus_core::session::{Message, SessionStore};
use nexus_core::tools::ToolManager;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let workspace_dir = cli
        .workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Initialize security sandbox
    let sandbox = Sandbox::new(&workspace_dir, cli.dangerously_skip_permissions)?;
    let mut tool_manager_builder = ToolManager::new(sandbox.clone());

    // Auto-connect rust-analyzer if a Rust workspace exists
    if workspace_dir.join("Cargo.toml").exists() {
        if let Ok(client) = nexus_lsp::LspClient::start("rust-analyzer", &workspace_dir).await {
            println!("\x1b[1;32m[LSP]\x1b[0m Connected to rust-analyzer");
            tool_manager_builder =
                tool_manager_builder.with_lsp_client(Arc::new(tokio::sync::Mutex::new(client)));
        }
    }
    let tool_manager = Arc::new(tool_manager_builder);

    // Initialize session database in the workspace root (.nexus/sessions.db)
    let nexus_dir = workspace_dir.join(".nexus");
    std::fs::create_dir_all(&nexus_dir)?;
    let session_store = SessionStore::new(nexus_dir.join("sessions.db"))?;

    // Determine session ID
    let mut session = if let Some(id) = cli.conversation {
        println!("Resuming session: {}", id);
        session_store.load_session(&id)?
    } else {
        session_store.create_session("Nexus CLI Session")?
    };

    println!("\x1b[1;36m====================================================\x1b[0m");
    println!("\x1b[1;36m  Nexus: The Polyglot & Rust AI Coding Harness      \x1b[0m");
    println!("\x1b[1;36m  Workspace: {}\x1b[0m", workspace_dir.display());
    println!("\x1b[1;36m  Session ID: {}\x1b[0m", session.id);
    println!("\x1b[1;36m====================================================\x1b[0m\n");

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| {
        // Fallback placeholder for dry-run if no key is set yet
        "DUMMY_KEY_FOR_LOCAL_TEST".to_string()
    });

    let provider = Arc::new(AnthropicProvider::new(api_key, Some(cli.model)));
    let tool_defs = ToolManager::tool_definitions();

    // Load architecture invariants from AGENTS.md (or default standards)
    let invariants = GStackEngine::load_invariants(&workspace_dir);
    let invariants_prompt = GStackEngine::format_architecture_prompt(&invariants);

    let base_system_prompt = format!(
        "You are Nexus, a world-class AI coding assistant.
You specialize in writing clean, modular, and robust software with an emphasis on idiomatic Rust and clean architecture.
Your workspace is at: {}.
Always inspect files before modifying them, and prefer focused edits using `edit_file` over overwriting entire files.

{}",
        workspace_dir.display(),
        invariants_prompt
    );

    loop {
        print!("\x1b[1;32mnexus>\x1b[0m ");
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

        // Handle slash commands
        let mut active_system_prompt = base_system_prompt.clone();
        let prompt_to_send = match trimmed {
            "/help" => {
                println!("\x1b[1;33mAvailable Commands:\x1b[0m");
                println!("  /ceo   - Run CEO review (challenge assumptions, evaluate 10-star product & scope)");
                println!("  /eng   - Run Engineering architecture review (invariants, data flow, failure modes)");
                println!("  /qa    - Run automated QA loop (check, test, fix, re-verify)");
                println!(
                    "  /ship  - Run pre-landing ship verification (clean tree, clippy, test gate)"
                );
                println!("  exit   - Exit Nexus");
                continue;
            }
            "/ceo" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating CEO Review Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::CeoReview,
                ));
                "Review the current project and plan from the CEO perspective: challenge premises, define the 10-star product, and recommend scope posture."
            }
            "/eng" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating Engineering Review Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::EngReview,
                ));
                "Perform an engineering architecture review: audit single sources of truth, interface contracts, error domains, and failure recovery."
            }
            "/qa" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating QA Loop Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::QaLoop,
                ));
                "Run test suites and compiler checks, identify any failures, patch them atomically via edit_file, and re-verify until 100% green."
            }
            "/ship" => {
                println!("\x1b[1;35m[gstack]\x1b[0m Activating Ship Mode");
                active_system_prompt.push_str("\n\n");
                active_system_prompt.push_str(&GStackEngine::format_review_prompt(
                    GStackReviewMode::ShipReview,
                ));
                "Perform pre-landing verification: check git status, test coverage, clippy warnings, and docs/changelog."
            }
            other => other,
        };

        // Add user message to session
        let user_msg = Message {
            role: "user".to_string(),
            content: prompt_to_send.to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now(),
        };
        session.messages.push(user_msg.clone());
        let _ = session_store.save_message(&session.id, &user_msg);

        // Turn loop: stream model response, execute tools, repeat until model finishes
        let mut turn_active = true;
        let mut turn_counter = 0;

        while turn_active {
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

            print!("\x1b[1;35mAI:\x1b[0m ");
            io::stdout().flush()?;

            while let Some(event) = rx.recv().await {
                match event {
                    NexusEvent::TokenChunk(chunk) => {
                        print!("{}", chunk);
                        io::stdout().flush()?;
                    }
                    NexusEvent::ToolCallStarted(ref call) => {
                        println!(
                            "\n\x1b[1;33m[Tool Call]\x1b[0m {} {:?}",
                            call.name, call.arguments
                        );
                    }
                    _ => {}
                }
            }
            println!();

            let assistant_msg = match handle.await? {
                Ok(msg) => msg,
                Err(err) => {
                    eprintln!("\x1b[1;31mProvider Error: {}\x1b[0m", err);
                    break;
                }
            };

            session.messages.push(assistant_msg.clone());
            let _ = session_store.save_message(&session.id, &assistant_msg);

            // Execute tools if requested
            if let Some(calls) = &assistant_msg.tool_calls {
                // Compass: 1 git snapshot per agent turn before executing modifying tools
                if let Ok(Some(snapshot)) =
                    GitSnapshotManager::create_turn_snapshot(&workspace_dir, &turn_id)
                {
                    println!(
                        "\x1b[0;32m[Git Snapshot]\x1b[0m Ref: {} (dirty worktree: {})",
                        &snapshot.commit_or_stash_sha[..8.min(snapshot.commit_or_stash_sha.len())],
                        snapshot.had_uncommitted_changes
                    );
                }

                let mut results = Vec::new();
                for call in calls {
                    println!("\x1b[1;34m[Executing]\x1b[0m {}", call.name);
                    let result = tool_manager.execute(call).await;
                    println!("\x1b[0;34m[Result]\x1b[0m {}", result.output);
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
    }

    Ok(())
}
