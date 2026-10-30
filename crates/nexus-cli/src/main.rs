use clap::Parser;
use nexus_core::events::NexusEvent;
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
    let tool_manager = Arc::new(ToolManager::new(sandbox.clone()));

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

    let system_prompt = format!(
        "You are Nexus, a world-class AI coding assistant.
You specialize in writing clean, modular, and robust software with an emphasis on idiomatic Rust and clean architecture.
Your workspace is at: {}.
Always inspect files before modifying them, and prefer focused edits using `edit_file` over overwriting entire files.",
        workspace_dir.display()
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

        // Add user message to session
        let user_msg = Message {
            role: "user".to_string(),
            content: trimmed.to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now(),
        };
        session.messages.push(user_msg.clone());
        let _ = session_store.save_message(&session.id, &user_msg);

        // Turn loop: stream model response, execute tools, repeat until model finishes
        let mut turn_active = true;
        while turn_active {
            let (tx, mut rx) = mpsc::channel(64);
            let provider_clone = provider.clone();
            let sys_clone = system_prompt.clone();
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
