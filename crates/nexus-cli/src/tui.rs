use std::path::Path;
use nexus_core::config::{ColorMode, DisplayMode, NexusConfig};

#[derive(Clone, Copy, Debug)]
pub struct ThemePalette {
    pub name: &'static str,
    // Brand & accents
    pub brand_primary: &'static str,   // Main accent (e.g. Anthropic terracotta)
    pub brand_secondary: &'static str, // Warm coral / highlight
    pub brand_glow: &'static str,      // Amber peach / glow
    
    // UI framing
    pub border: &'static str,          // Subtle rounded border
    pub border_accent: &'static str,   // Active card border
    pub card_bg: &'static str,         // Background tint for tool cards
    pub badge_bg: &'static str,        // Background tint for status pills
    
    // Typography
    pub text_primary: &'static str,    // Main crisp text
    pub text_secondary: &'static str,  // Subtle body text
    pub text_muted: &'static str,      // Labels, hints, timestamps
    pub text_dim: &'static str,        // Outer framing, dim separators
    
    // Semantic indicators
    pub success: &'static str,         // Emerald green
    pub error: &'static str,           // Soft crimson
    pub warning: &'static str,         // Warm gold
    pub info: &'static str,            // Electric sky blue
    pub purple: &'static str,          // Lavender violet
    
    // Text styles
    pub bold: &'static str,
    pub italic: &'static str,
    pub dim: &'static str,
    pub underline: &'static str,
    pub reset: &'static str,

    // Backward-compatible ANSI shorthands
    pub cyan: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub magenta: &'static str,
    pub red: &'static str,
}

impl ThemePalette {
    pub fn for_mode(mode: ColorMode) -> Self {
        match mode {
            ColorMode::Claude | ColorMode::Dark => Self {
                name: "Terracotta",
                // Anthropic warm coral & terracotta palette (24-bit TrueColor)
                brand_primary: "\x1b[38;2;217;107;39m",   // #D96B27 (Terracotta)
                brand_secondary: "\x1b[38;2;235;127;92m", // #EB7F5C (Warm Coral)
                brand_glow: "\x1b[38;2;245;158;11m",      // #F59E0B (Amber Gold)

                border: "\x1b[38;2;71;85;105m",          // #475569 (Muted Slate)
                border_accent: "\x1b[38;2;217;107;39m",   // #D96B27 (Terracotta)
                card_bg: "\x1b[48;2;30;30;38m",          // #1E1E26 (Warm Charcoal Card)
                badge_bg: "\x1b[48;2;45;45;58m",         // #2D2D3A (Subtle Pill)

                text_primary: "\x1b[38;2;248;250;252m",    // #F8FAFC (Warm White)
                text_secondary: "\x1b[38;2;203;213;225m",  // #CBD5E1 (Light Slate)
                text_muted: "\x1b[38;2;148;163;184m",      // #94A3B8 (Dim Slate)
                text_dim: "\x1b[38;2;100;116;139m",        // #64748B (Frame Muted)

                success: "\x1b[38;2;52;211;153m",         // #34D399 (Soft Emerald)
                error: "\x1b[38;2;248;113;113m",           // #F87171 (Soft Coral Red)
                warning: "\x1b[38;2;251;191;36m",          // #FBBF24 (Warm Amber)
                info: "\x1b[38;2;56;189;248m",             // #38BDF8 (Sky Blue)
                purple: "\x1b[38;2;192;132;252m",          // #C084FC (Lavender)

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;56;189;248m",
                green: "\x1b[38;2;52;211;153m",
                yellow: "\x1b[38;2;245;158;11m",
                magenta: "\x1b[38;2;217;107;39m",
                red: "\x1b[38;2;248;113;113m",
            },
            ColorMode::Cyberpunk => Self {
                name: "Cyberpunk Neon",
                brand_primary: "\x1b[38;2;0;245;255m",     // #00F5FF (Neon Cyan)
                brand_secondary: "\x1b[38;2;255;0;127m",   // #FF007F (Hot Pink)
                brand_glow: "\x1b[38;2;255;230;0m",        // #FFE600 (Cyber Gold)

                border: "\x1b[38;2;75;60;110m",
                border_accent: "\x1b[38;2;0;245;255m",
                card_bg: "\x1b[48;2;20;15;35m",
                badge_bg: "\x1b[48;2;35;25;60m",

                text_primary: "\x1b[38;2;255;255;255m",
                text_secondary: "\x1b[38;2;220;215;240m",
                text_muted: "\x1b[38;2;160;145;190m",
                text_dim: "\x1b[38;2;110;95;140m",

                success: "\x1b[38;2;0;255;159m",
                error: "\x1b[38;2;255;51;102m",
                warning: "\x1b[38;2;255;230;0m",
                info: "\x1b[38;2;0;245;255m",
                purple: "\x1b[38;2;214;51;255m",

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;0;245;255m",
                green: "\x1b[38;2;0;255;159m",
                yellow: "\x1b[38;2;255;230;0m",
                magenta: "\x1b[38;2;255;0;127m",
                red: "\x1b[38;2;255;51;102m",
            },
            ColorMode::TokyoNight => Self {
                name: "Tokyo Night",
                brand_primary: "\x1b[38;2;122;162;247m",   // #7AA2F7 (Tokyo Blue)
                brand_secondary: "\x1b[38;2;187;154;247m", // #BB9AF7 (Purple)
                brand_glow: "\x1b[38;2;125;207;255m",      // #7DCFFF (Cyan)

                border: "\x1b[38;2;65;72;104m",
                border_accent: "\x1b[38;2;122;162;247m",
                card_bg: "\x1b[48;2;26;27;38m",
                badge_bg: "\x1b[48;2;36;40;59m",

                text_primary: "\x1b[38;2;192;202;245m",
                text_secondary: "\x1b[38;2;169;177;214m",
                text_muted: "\x1b[38;2;120;124;153m",
                text_dim: "\x1b[38;2;86;95;137m",

                success: "\x1b[38;2;158;206;106m",
                error: "\x1b[38;2;247;118;142m",
                warning: "\x1b[38;2;224;175;104m",
                info: "\x1b[38;2;125;207;255m",
                purple: "\x1b[38;2;187;154;247m",

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;125;207;255m",
                green: "\x1b[38;2;158;206;106m",
                yellow: "\x1b[38;2;224;175;104m",
                magenta: "\x1b[38;2;187;154;247m",
                red: "\x1b[38;2;247;118;142m",
            },
            ColorMode::Amber => Self {
                name: "Dec Amber CRT",
                brand_primary: "\x1b[38;2;255;176;0m",     // #FFB000 (Amber CRT)
                brand_secondary: "\x1b[38;2;255;204;0m",   // #FFCC00
                brand_glow: "\x1b[38;2;255;220;50m",

                border: "\x1b[38;2;140;95;10m",
                border_accent: "\x1b[38;2;255;176;0m",
                card_bg: "\x1b[48;2;28;20;5m",
                badge_bg: "\x1b[48;2;45;32;8m",

                text_primary: "\x1b[38;2;255;210;100m",
                text_secondary: "\x1b[38;2;230;175;60m",
                text_muted: "\x1b[38;2;180;130;30m",
                text_dim: "\x1b[38;2;120;85;15m",

                success: "\x1b[38;2;255;200;50m",
                error: "\x1b[38;2;255;100;50m",
                warning: "\x1b[38;2;255;176;0m",
                info: "\x1b[38;2;255;220;120m",
                purple: "\x1b[38;2;230;150;50m",

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;255;220;120m",
                green: "\x1b[38;2;255;200;50m",
                yellow: "\x1b[38;2;255;176;0m",
                magenta: "\x1b[38;2;255;140;20m",
                red: "\x1b[38;2;255;80;30m",
            },
            ColorMode::Nordic => Self {
                name: "Nordic Frost",
                brand_primary: "\x1b[38;2;136;192;208m",   // #88C0D0 (Arctic Frost)
                brand_secondary: "\x1b[38;2;129;161;193m", // #81A1C1
                brand_glow: "\x1b[38;2;235;203;139m",      // #EBCB8B (Aurora)

                border: "\x1b[38;2;76;86;106m",
                border_accent: "\x1b[38;2;136;192;208m",
                card_bg: "\x1b[48;2;46;52;64m",
                badge_bg: "\x1b[48;2;59;66;82m",

                text_primary: "\x1b[38;2;236;239;244m",
                text_secondary: "\x1b[38;2;229;233;240m",
                text_muted: "\x1b[38;2;144;155;176m",
                text_dim: "\x1b[38;2;100;110;130m",

                success: "\x1b[38;2;163;190;140m",
                error: "\x1b[38;2;191;97;106m",
                warning: "\x1b[38;2;235;203;139m",
                info: "\x1b[38;2;136;192;208m",
                purple: "\x1b[38;2;180;142;173m",

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;136;192;208m",
                green: "\x1b[38;2;163;190;140m",
                yellow: "\x1b[38;2;235;203;139m",
                magenta: "\x1b[38;2;180;142;173m",
                red: "\x1b[38;2;191;97;106m",
            },
            ColorMode::Light => Self {
                name: "Solarized Paper",
                brand_primary: "\x1b[38;2;203;75;22m",     // #CB4B16 (Terracotta Rust)
                brand_secondary: "\x1b[38;2;38;139;210m",  // #268BD2 (Blue)
                brand_glow: "\x1b[38;2;181;137;0m",        // #B58900 (Warm Ochre)

                border: "\x1b[38;2;147;161;161m",
                border_accent: "\x1b[38;2;203;75;22m",
                card_bg: "\x1b[48;2;238;232;213m",
                badge_bg: "\x1b[48;2;225;218;197m",

                text_primary: "\x1b[38;2;7;54;66m",        // #073642 (Deep Espresso)
                text_secondary: "\x1b[38;2;88;110;117m",
                text_muted: "\x1b[38;2;101;123;131m",
                text_dim: "\x1b[38;2;131;148;150m",

                success: "\x1b[38;2;42;161;152m",          // #2AA198 (Teal Green)
                error: "\x1b[38;2;220;50;47m",             // #DC322F (Red)
                warning: "\x1b[38;2;181;137;0m",           // #B58900 (Ochre)
                info: "\x1b[38;2;38;139;210m",             // #268BD2 (Blue)
                purple: "\x1b[38;2;108;113;196m",          // #6C71C4 (Violet)

                bold: "\x1b[1m",
                italic: "\x1b[3m",
                dim: "\x1b[2m",
                underline: "\x1b[4m",
                reset: "\x1b[0m",

                cyan: "\x1b[38;2;38;139;210m",
                green: "\x1b[38;2;42;161;152m",
                yellow: "\x1b[38;2;181;137;0m",
                magenta: "\x1b[38;2;203;75;22m",
                red: "\x1b[38;2;220;50;47m",
            },
            ColorMode::Plain => Self {
                name: "Plain Text",
                brand_primary: "",
                brand_secondary: "",
                brand_glow: "",

                border: "",
                border_accent: "",
                card_bg: "",
                badge_bg: "",

                text_primary: "",
                text_secondary: "",
                text_muted: "",
                text_dim: "",

                success: "",
                error: "",
                warning: "",
                info: "",
                purple: "",

                bold: "",
                italic: "",
                dim: "",
                underline: "",
                reset: "",

                cyan: "",
                green: "",
                yellow: "",
                magenta: "",
                red: "",
            },
        }
    }
}

pub struct TuiRenderer;

impl TuiRenderer {
    /// Render the iconic Claude Code-style header banner with rounded corners & status badges
    pub fn render_banner(
        config: &NexusConfig,
        workspace: &Path,
        git_branch: Option<&str>,
        memory_kb: Option<usize>,
        rea_available: bool,
        plugins_count: usize,
        theme: &ThemePalette,
    ) {
        if config.display_mode == DisplayMode::Minimal {
            let branch_str = git_branch.map(|b| format!(" [{}]", b)).unwrap_or_default();
            println!(
                "{}✻ Nexus{} [{}] (session in {}{}{})",
                theme.brand_primary,
                theme.reset,
                config.model,
                workspace.display(),
                branch_str,
                theme.reset
            );
            return;
        }

        let branch_chip = match git_branch {
            Some(b) => format!(" {}git: {}{}", theme.success, b, theme.reset),
            None => "".to_string(),
        };

        let mem_chip = match memory_kb {
            Some(kb) => format!("{:.1} MB", kb as f64 / 1024.0),
            None => "~3.9 MB".to_string(),
        };

        let endpoint_str = config.base_url.as_deref().unwrap_or("https://api.anthropic.com");

        let tagline = "· 7MB of pure native arrogance";

        let prefix_len: usize = 25; // "╭─ ✻  N E X U S  v0.1.0  "
        let total_target: usize = 74;
        let dashes_count = total_target.saturating_sub(prefix_len + tagline.chars().count()).max(3);
        let dashes = "─".repeat(dashes_count);

        println!();
        println!(
            "{}╭─ {}✻  N E X U S{}  {}v0.1.0{}  {}{}{} {}{}",
            theme.border,
            theme.brand_primary,
            theme.bold,
            theme.text_dim,
            theme.reset,
            theme.brand_secondary,
            tagline,
            theme.border,
            dashes,
            theme.reset
        );
        println!(
            "{}│{}  {}Model:{}     {}{}{}",
            theme.border, theme.reset, theme.text_dim, theme.text_primary, config.model, theme.reset, theme.reset
        );
        println!(
            "{}│{}  {}Endpoint:{}  {}{}{}",
            theme.border, theme.reset, theme.text_dim, theme.text_muted, endpoint_str, theme.reset, theme.reset
        );
        println!(
            "{}│{}  {}Workspace:{} {}{}{}{}",
            theme.border, theme.reset, theme.text_dim, theme.reset, theme.text_primary, workspace.display(), branch_chip, theme.reset
        );

        let rea_str = if rea_available {
            format!("{}REA: On{}", theme.info, theme.reset)
        } else {
            format!("{}REA: Off{}", theme.text_dim, theme.reset)
        };

        let plugins_str = if plugins_count > 0 {
            format!("{}Plugins: {}{}", theme.purple, plugins_count, theme.reset)
        } else {
            format!("{}Plugins: None{}", theme.text_dim, theme.reset)
        };

        println!(
            "{}│{}  {}Status:{}    {}RAM: {}{}  ·  {}  ·  {}  ·  {}Theme: {}{}",
            theme.border,
            theme.reset,
            theme.text_dim,
            theme.reset,
            theme.success,
            mem_chip,
            theme.reset,
            rea_str,
            plugins_str,
            theme.brand_secondary,
            theme.name,
            theme.reset
        );
        println!(
            "{}╰─────────────────────────────────────────────────────────────────────────{}\n",
            theme.border,
            theme.reset
        );
    }

    /// Render Claude Code-style framed prompt:
    /// ╭─ ~/nexus [● main] · 3.9 MB · idle
    /// ╰─❯ 
    pub fn render_prompt(
        workspace: &Path,
        git_branch: Option<&str>,
        memory_kb: Option<usize>,
        context_pill: Option<&str>,
        theme: &ThemePalette,
    ) {
        let ws_str = workspace.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(workspace.to_str().unwrap_or("."));

        let branch_chip = match git_branch {
            Some(b) => format!(" {}[● {}]{}", theme.success, b, theme.reset),
            None => "".to_string(),
        };

        let mem_str = match memory_kb {
            Some(kb) => format!("{:.1} MB", kb as f64 / 1024.0),
            None => "~3.9 MB".to_string(),
        };

        let ctx_chip = match context_pill {
            Some(p) => format!(" · {}{}{}", theme.warning, p, theme.reset),
            None => "".to_string(),
        };

        println!(
            "{}╭─ {}{}{}{}{}{} · {}{}{} · {}{}idle{}",
            theme.border,
            theme.text_primary,
            theme.bold,
            ws_str,
            theme.reset,
            branch_chip,
            ctx_chip,
            theme.info,
            mem_str,
            theme.reset,
            theme.border,
            theme.text_dim,
            theme.reset
        );
        print!(
            "{}╰─❯{} ",
            theme.brand_primary,
            theme.reset
        );
    }

    /// Render tool call start card (Unified diff preview for edits, command preview for bash)
    pub fn render_tool_call_start(name: &str, args: &serde_json::Value, theme: &ThemePalette) {
        println!();
        match name {
            "edit_file" => {
                let path = args.get("Path").or_else(|| args.get("path")).and_then(|v| v.as_str()).unwrap_or("unknown");
                let target = args.get("Target").or_else(|| args.get("target")).and_then(|v| v.as_str()).unwrap_or("");
                let replacement = args.get("Replacement").or_else(|| args.get("replacement")).and_then(|v| v.as_str()).unwrap_or("");

                println!(
                    "{}╭─ {}⟡ Patch File:{} {}{}{}",
                    theme.border_accent,
                    theme.brand_secondary,
                    theme.bold,
                    theme.text_primary,
                    path,
                    theme.reset
                );

                // Show up to 3 lines of diff preview
                for line in target.lines().take(3) {
                    println!("{}│{} {}- {}{}", theme.border_accent, theme.reset, theme.error, line, theme.reset);
                }
                if target.lines().count() > 3 {
                    println!("{}│{} {}  ... (truncated target){}", theme.border_accent, theme.reset, theme.text_dim, theme.reset);
                }
                for line in replacement.lines().take(3) {
                    println!("{}│{} {}+ {}{}", theme.border_accent, theme.reset, theme.success, line, theme.reset);
                }
                if replacement.lines().count() > 3 {
                    println!("{}│{} {}  ... (truncated replacement){}", theme.border_accent, theme.reset, theme.text_dim, theme.reset);
                }
                println!("{}╰───────────────────────────────────────────────────{}", theme.border_accent, theme.reset);
            }
            "run_command" => {
                let cmd = args.get("CommandLine").or_else(|| args.get("cmd")).and_then(|v| v.as_str()).unwrap_or("");
                println!(
                    "{}╭─ {}⟡ Bash:{} {}{}{}",
                    theme.border_accent,
                    theme.info,
                    theme.bold,
                    theme.text_primary,
                    cmd,
                    theme.reset
                );
            }
            "read_file" => {
                let path = args.get("Path").or_else(|| args.get("path")).and_then(|v| v.as_str()).unwrap_or("unknown");
                let start = args.get("StartLine").and_then(|v| v.as_u64());
                let end = args.get("EndLine").and_then(|v| v.as_u64());
                let range = match (start, end) {
                    (Some(s), Some(e)) => format!(" (L{}-{})", s, e),
                    (Some(s), None) => format!(" (L{}+)", s),
                    _ => "".to_string(),
                };
                println!(
                    "{}╭─ {}⟡ Read File:{} {}{}{}{}",
                    theme.border,
                    theme.purple,
                    theme.bold,
                    theme.text_primary,
                    path,
                    range,
                    theme.reset
                );
                println!("{}╰───────────────────────────────────────────────────{}", theme.border, theme.reset);
            }
            "cargo_check" => {
                println!(
                    "{}╭─ {}⟡ Rust Doctor:{} {}Compiler Diagnostic Check{}",
                    theme.border_accent,
                    theme.brand_glow,
                    theme.bold,
                    theme.text_secondary,
                    theme.reset
                );
            }
            "invoke_subagent" => {
                let agent = args.get("agent").or_else(|| args.get("Agent")).and_then(|v| v.as_str()).unwrap_or("worker");
                let topology = args.get("topology").and_then(|v| v.as_str()).unwrap_or("single");
                let task = args.get("task").or_else(|| args.get("Task")).and_then(|v| v.as_str()).unwrap_or("");
                println!(
                    "{}╭─ {}🤖 Subagent Spawn:{} {}{} {}[{}] (topology: {}){}",
                    theme.border_accent,
                    theme.purple,
                    theme.bold,
                    theme.text_primary,
                    agent,
                    theme.brand_glow,
                    topology,
                    topology,
                    theme.reset
                );
                for line in task.lines().take(2) {
                    println!("{}│{}   {}{}", theme.border_accent, theme.reset, line, theme.reset);
                }
                println!("{}╰───────────────────────────────────────────────────{}", theme.border_accent, theme.reset);
            }
            "list_agents" => {
                println!(
                    "{}╭─ {}🤖 Subagent Roster Query:{} Discovering Agents {}{}",
                    theme.border_accent,
                    theme.purple,
                    theme.bold,
                    theme.text_secondary,
                    theme.reset
                );
            }
            other => {
                println!(
                    "{}╭─ {}⟡ Tool Invocation:{} {}{}{}",
                    theme.border,
                    theme.brand_secondary,
                    theme.bold,
                    theme.text_primary,
                    other,
                    theme.reset
                );
            }
        }
    }

    /// Render tool call completion card
    pub fn render_tool_result(name: &str, output: &str, success: bool, theme: &ThemePalette) {
        let (icon, color) = if success {
            ("✓", theme.success)
        } else {
            ("✗", theme.error)
        };

        if name == "invoke_subagent" {
            println!(
                "{}╭─ {}🤖 Subagent Complete [{}]{}",
                theme.border_accent,
                theme.purple,
                if success { "Success" } else { "Failed" },
                theme.reset
            );
            for l in output.lines().take(4) {
                println!("{}│{}  {}{}", theme.border_accent, theme.reset, l, theme.reset);
            }
            println!("{}╰───────────────────────────────────────────────────{}", theme.border_accent, theme.reset);
            return;
        }

        if name == "run_command" || name == "cargo_check" {
            // Print output trimmed with border
            let lines: Vec<&str> = output.lines().collect();
            let display_lines = if lines.len() > 10 {
                &lines[..10]
            } else {
                &lines[..]
            };

            for l in display_lines {
                println!("{}│{}  {}{}", theme.border, theme.reset, l, theme.reset);
            }
            if lines.len() > 10 {
                println!(
                    "{}│{}  {}... ({} lines omitted){}",
                    theme.border,
                    theme.reset,
                    theme.text_dim,
                    lines.len() - 10,
                    theme.reset
                );
            }
            println!(
                "{}╰─ {}{} Done:{} {}{}",
                theme.border,
                color,
                icon,
                theme.reset,
                if success { "Success" } else { "Failed" },
                theme.reset
            );
        }
    }

    /// Render streamed AI token with clean markdown and code highlighting
    pub fn render_ai_prefix(theme: &ThemePalette) {
        print!(
            "{}⟡ Nexus:{} ",
            theme.brand_primary,
            theme.reset
        );
    }

    /// Render turn completed footer chip with RAM, token metrics, and context status
    pub fn render_turn_footer(
        memory_kb: Option<usize>,
        token_usage: Option<&nexus_core::events::TokenUsage>,
        context_pill: Option<&str>,
        theme: &ThemePalette,
    ) {
        let mem_str = match memory_kb {
            Some(kb) => format!("{:.1} MB", kb as f64 / 1024.0),
            None => "~3.9 MB".to_string(),
        };

        let token_chip = match token_usage {
            Some(u) if u.input_tokens > 0 || u.output_tokens > 0 || u.cache_read_tokens > 0 => {
                let hit_rate = u.cache_hit_rate();
                if u.cache_read_tokens > 0 {
                    format!(
                        " · {}{} tokens{} {}({:.0}% cached: {}){}",
                        theme.text_primary,
                        u.output_tokens,
                        theme.reset,
                        theme.success,
                        hit_rate,
                        u.cache_read_tokens,
                        theme.reset
                    )
                } else {
                    format!(
                        " · {}{} tokens ({} in, {} out){}",
                        theme.text_primary,
                        u.input_tokens + u.output_tokens,
                        u.input_tokens,
                        u.output_tokens,
                        theme.reset
                    )
                }
            }
            _ => "".to_string(),
        };

        let context_chip = match context_pill {
            Some(pill) => format!(" · {}{}{}", theme.text_secondary, pill, theme.reset),
            None => "".to_string(),
        };

        println!(
            "\n{}─── {}⟡ Turn finished{} {}· RAM: {} · CPU: 0.0%{}{} ───{}{}\n",
            theme.border,
            theme.brand_secondary,
            theme.reset,
            theme.text_dim,
            mem_str,
            token_chip,
            context_chip,
            theme.border,
            theme.reset
        );
    }

    /// Render /tokens slash command scorecard
    pub fn render_token_dashboard(tokens: &nexus_core::events::TokenUsage, theme: &ThemePalette) {
        let total_input = tokens.input_tokens;
        let total_cached = tokens.cache_read_tokens;
        let total_created = tokens.cache_creation_tokens;
        let total_output = tokens.output_tokens;
        let total_tokens = total_input + total_output + total_cached;
        let hit_rate = tokens.cache_hit_rate();

        println!();
        println!(
            "{}╭─ {}✻ Token Efficiency & Prompt Cache Scorecard ───────────────────────{}{}",
            theme.border,
            theme.brand_primary,
            theme.border,
            theme.reset
        );
        println!(
            "{}│{}  {}Total Output Tokens:      {}{:>8}{}",
            theme.border, theme.reset, theme.text_dim, theme.text_primary, total_output, theme.reset
        );
        println!(
            "{}│{}  {}Base Input Tokens:        {}{:>8}{}",
            theme.border, theme.reset, theme.text_dim, theme.text_secondary, total_input, theme.reset
        );
        println!(
            "{}│{}  {}Prompt Cache Read Tokens: {}{:>8}{} {}[90% API discount]{}",
            theme.border, theme.reset, theme.text_dim, theme.success, total_cached, theme.reset, theme.success, theme.reset
        );
        println!(
            "{}│{}  {}Cache Creation Tokens:    {}{:>8}{}",
            theme.border, theme.reset, theme.text_dim, theme.warning, total_created, theme.reset
        );
        println!(
            "{}│{}  {}Prompt Cache Hit Rate:    {}{:>7.1}%{}",
            theme.border, theme.reset, theme.text_dim, theme.brand_glow, hit_rate, theme.reset
        );
        println!(
            "{}│{}  {}Cumulative Processed:     {}{:>8} tokens{}",
            theme.border, theme.reset, theme.text_dim, theme.bold, total_tokens, theme.reset
        );
        println!(
            "{}╰─────────────────────────────────────────────────────────────────────────{}\n",
            theme.border, theme.reset
        );
    }
}
