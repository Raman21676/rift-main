//! Rift CLI - AI coding assistant

use anyhow::Result;
use clap::{Parser, Subcommand};
use rift_core::{Message, RiftConfig, RiftEngine, SessionStore, TaskStatus, create_sample_config};
use serde_json::Value;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "rift")]
#[command(about = "AI coding assistant with plugin-based architecture")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// API key (or set OPENAI_API_KEY env var, or put in config file)
    #[arg(long, env = "OPENAI_API_KEY")]
    api_key: Option<String>,

    /// Model to use
    #[arg(long, env = "RIFT_MODEL")]
    model: Option<String>,

    /// API base URL
    #[arg(long, env = "RIFT_BASE_URL")]
    base_url: Option<String>,

    /// Session name to resume
    #[arg(long, default_value = "default")]
    session: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session
    Chat {
        /// Initial message to send
        #[arg(short, long)]
        message: Option<String>,
    },

    /// Plan and execute a goal autonomously
    Do {
        /// The goal to accomplish
        goal: String,
        
        /// Enable self-correction for failed tasks
        #[arg(long)]
        self_correct: bool,
        
        /// Enable verification of task outputs
        #[arg(long)]
        verify: bool,
        
        /// Full autonomous mode (context + self-correct + verify)
        #[arg(long)]
        auto: bool,
    },

    /// Execute a single command
    Run {
        /// The command to execute
        message: String,
    },

    /// List available tools
    Tools,

    /// Show configuration info
    Config,
    
    /// Daemon control commands
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the background daemon
    Start {
        /// Run in foreground (don't detach)
        #[arg(long)]
        foreground: bool,
        /// Enable remote control API
        #[arg(long)]
        remote: bool,
        /// Port for remote control (default: 7788)
        #[arg(long, default_value = "7788")]
        port: u16,
    },
    
    /// Stop the daemon
    Stop,
    
    /// Check daemon status
    Status,
    
    /// Submit a task to the daemon queue
    Submit {
        /// The goal/task to accomplish
        goal: String,
    },
    
    /// List pending tasks
    Queue,
    
    /// List recent completed tasks
    History,
    
    /// Cancel a pending task
    Cancel {
        task_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let mut config = RiftConfig::load();

    // CLI overrides take highest priority
    if let Some(key) = cli.api_key {
        config.llm.api_key = key;
    }
    if let Some(model) = cli.model {
        config.llm.model = model;
    }
    if let Some(base_url) = cli.base_url {
        config.llm.base_url = base_url;
    }

    if config.llm.api_key.is_empty() {
        eprintln!("Error: API key required.");
        eprintln!("  Set OPENAI_API_KEY environment variable, or");
        eprintln!("  Add to ~/.config/rift/config.toml: [api] key = \"...\"");
        eprintln!("  Or use: rift --api-key <key> <command>");
        std::process::exit(1);
    }

    let mut engine = RiftEngine::new(config.clone());
    register_builtins(&mut engine);

    match cli.command {
        Commands::Chat { message } => {
            run_chat(&engine, message, &cli.session).await?;
        }
        Commands::Run { message } => {
            run_single(&engine, &message).await?;
        }
        Commands::Do { goal, self_correct, verify, auto } => {
            run_goal(&engine, &goal, self_correct, verify, auto).await?;
        }
        Commands::Tools => {
            list_tools(&engine);
        }
        Commands::Config => {
            show_config(&config);
        }
        Commands::Daemon { command } => {
            handle_daemon_command(command, config).await?;
        }
    }

    Ok(())
}

fn register_builtins(engine: &mut RiftEngine) {
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::BashTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::ReadFileTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::WriteFileTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GlobTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GrepTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::EditFileTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::InsertAtLineTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GitStatusTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GitDiffTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GitCommitTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GitPushTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::GitBranchTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::DeployTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::WebFetchTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::WebSearchTool::new()
    ));
}

fn show_config(config: &RiftConfig) {
    println!("🌊 Rift Configuration");
    println!();
    if let Some(path) = rift_core::ConfigFile::config_path() {
        println!("Config file: {}", path.display());
        if !path.exists() {
            println!("  (file does not exist yet)");
            if let Ok(sample_path) = create_sample_config() {
                println!("  Created sample config at: {}", sample_path.display());
            }
        }
    }
    println!();
    println!("Model:       {}", config.llm.model);
    println!("Base URL:    {}", config.llm.base_url);
    println!("API Key:     {}...", &config.llm.api_key[..config.llm.api_key.len().min(8)]);
    println!("Max tasks:   {}", config.max_concurrent_tasks);
    println!("Capabilities: {}", config.capabilities.iter()
        .map(|c| format!("{:?}", c))
        .collect::<Vec<_>>()
        .join(", "));
}

async fn run_chat(engine: &RiftEngine, initial: Option<String>, session_name: &str) -> Result<()> {
    println!("🌊 Rift - AI Coding Assistant");
    println!("Session: {} | Type 'exit', 'quit', or /exit to exit. /help for commands.\n", session_name);

    let store: Option<Arc<Mutex<SessionStore>>> = match SessionStore::default() {
        Ok(s) => Some(Arc::new(Mutex::new(s))),
        Err(e) => {
            eprintln!("Warning: Could not open session store: {}", e);
            None
        }
    };

    let session_id: Option<String> = store.as_ref()
        .and_then(|s| s.lock().ok())
        .and_then(|store| store.get_or_create(session_name).ok());

    let mut messages: Vec<Message> = if let Some(ref sid) = session_id {
        match store.as_ref().unwrap().lock().unwrap().load_messages(sid) {
            Ok(mut msgs) => {
                if msgs.is_empty() {
                    msgs.push(Message::system("You are a helpful coding assistant. Use tools when appropriate."));
                }
                println!("📂 Loaded {} messages from session '{}'\n", msgs.len(), session_name);
                msgs
            }
            Err(e) => {
                eprintln!("Warning: Could not load session: {}", e);
                vec![Message::system("You are a helpful coding assistant. Use tools when appropriate.")]
            }
        }
    } else {
        vec![Message::system("You are a helpful coding assistant. Use tools when appropriate.")]
    };

    if let Some(msg) = initial {
        messages.push(Message::user(msg));
        match process_message(engine, &messages).await {
            Ok(response) => messages.push(Message::assistant(response)),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    loop {
        print!("\n> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            break;
        }

        // Handle slash commands
        if input.starts_with('/') {
            match handle_slash_command(engine, input, &mut messages, store.clone(), session_id.clone()).await {
                SlashResult::Exit => break,
                SlashResult::Continue => continue,
            }
        }

        messages.push(Message::user(input));

        match process_message(engine, &messages).await {
            Ok(response) => {
                messages.push(Message::assistant(response));
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(())
}

enum SlashResult {
    Exit,
    Continue,
}

async fn handle_slash_command(
    engine: &RiftEngine,
    input: &str,
    messages: &mut Vec<Message>,
    store: Option<Arc<Mutex<SessionStore>>>,
    session_id: Option<String>,
) -> SlashResult {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return SlashResult::Continue;
    }

    match parts[0] {
        "/help" | "/h" => {
            println!("Available commands:");
            println!("  /help                   Show this help message");
            println!("  /plan <goal>            Plan and execute a goal autonomously");
            println!("  /plan --self-correct <goal>  Enable auto-retry on failure");
            println!("  /tool <name> <arg>      Execute a tool directly");
            println!("  /tools                  List available tools");
            println!("  /status                 Show session status");
            println!("  /sessions               List saved sessions");
            println!("  /clear                  Clear conversation history");
            println!("  /model <name>           Show/switch model (requires restart)");
            println!("  /exit, /quit            Exit the chat");
            println!();
            println!("CLI Usage:");
            println!("  rift do 'make a website'              Execute goal");
            println!("  rift do --self-correct 'deploy app'   With auto-recovery");
            println!("  rift do --verify 'create app'         With verification");
            println!("  rift do --auto 'build project'        Full autonomous mode");
            println!();
            println!("Mode flags can be combined:");
            println!("  --self-correct  Auto-retry failed tasks");
            println!("  --verify        Verify outputs after completion");
            println!("  --auto          Enable all autonomous features");
            println!();
            println!("Tool examples:");
            println!("  /tool bash '{{\"command\":\"ls -la\"}}'");
            println!("  /tool read_file '{{\"path\":\"src/main.rs\"}}'");
            println!("  /tool web_search '{{\"query\":\"rust async\"}}'");
        }
        "/exit" | "/quit" => {
            return SlashResult::Exit;
        }
        "/clear" => {
            messages.clear();
            messages.push(Message::system("You are a helpful coding assistant. Use tools when appropriate."));
            if let (Some(ref sid), Some(ref st)) = (session_id, store) {
                if let Ok(store) = st.lock() {
                    let _ = store.clear_messages(sid);
                }
            }
            println!("Conversation cleared.");
        }
        "/sessions" => {
            if let Some(ref st) = store {
                if let Ok(store) = st.lock() {
                    match store.list_sessions() {
                        Ok(sessions) => {
                            if sessions.is_empty() {
                                println!("No saved sessions.");
                            } else {
                                println!("Saved sessions:");
                                for (_, name, _) in sessions {
                                    let marker = if Some(name.clone()) == session_id { " (current)" } else { "" };
                                    println!("  - {}{}", name, marker);
                                }
                            }
                        }
                        Err(e) => println!("Error listing sessions: {}", e),
                    }
                }
            } else {
                println!("Session store not available.");
            }
        }
        "/tools" => {
            list_tools(engine);
        }
        "/status" => {
            println!("Messages in session: {}", messages.len());
            println!("Model: {}", engine.llm().config().model);
            println!("Base URL: {}", engine.llm().config().base_url);
            println!("Tools registered: {}", engine.plugins().list_tools().len());
        }
        "/model" => {
            if parts.len() > 1 {
                println!("Model switch requires restarting Rift.");
                println!("Current model: {}", engine.llm().config().model);
                println!("Run with: rift --model {} chat", parts[1]);
            } else {
                println!("Current model: {}", engine.llm().config().model);
            }
        }
        "/plan" => {
            if parts.len() < 2 {
                println!("Usage: /plan <goal>");
                println!("Example: /plan make a snake game in HTML");
                println!("       /plan --self-correct <goal>  (enable auto-retry on failure)");
                println!("       /plan --verify <goal>        (verify outputs)");
                println!("       /plan --auto <goal>          (full autonomous mode)");
                return SlashResult::Continue;
            }
            
            let self_correct = parts.contains(&"--self-correct");
            let verify = parts.contains(&"--verify");
            let auto = parts.contains(&"--auto");
            
            let goal_parts: Vec<&str> = parts.iter()
                .skip(1)
                .filter(|&&p| p != "--self-correct" && p != "--verify" && p != "--auto")
                .copied()
                .collect();
            let goal = goal_parts.join(" ");
            
            println!("Planning: {}", goal);
            if auto {
                println!("(Full autonomous mode: context + self-correct + verify)");
            } else {
                if self_correct { println!("(Self-correction enabled)"); }
                if verify { println!("(Verification enabled)"); }
            }
            if let Err(e) = run_goal(engine, &goal, self_correct, verify, auto).await {
                println!("Execution failed: {}", e);
            }
        }
        "/tool" => {
            if parts.len() < 2 {
                println!("Usage: /tool <name> [json_args]");
                println!("Example: /tool bash '{{\"command\":\"ls -la\"}}'");
                return SlashResult::Continue;
            }

            let tool_name = parts[1];
            let args_json = if parts.len() >= 3 {
                parts[2..].join(" ")
            } else {
                "{}".to_string()
            };

            let args: Value = match serde_json::from_str(&args_json) {
                Ok(v) => v,
                Err(e) => {
                    println!("Failed to parse arguments as JSON: {}", e);
                    println!("Example: /tool bash '{{\"command\":\"ls -la\"}}'");
                    return SlashResult::Continue;
                }
            };

            let agent = engine.agent();
            match agent.execute_tool_direct(tool_name, args).await {
                Ok(output) => {
                    println!("\n[{}] {}", tool_name, if output.success { "✓" } else { "✗" });
                    println!("{}", output.content);
                    if let Some(data) = output.data {
                        println!("Data: {}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    }
                }
                Err(e) => {
                    println!("Tool execution failed: {}", e);
                }
            }
        }
        _ => {
            println!("Unknown command: {}", parts[0]);
            println!("Type /help for available commands.");
        }
    }

    SlashResult::Continue
}


async fn run_single(engine: &RiftEngine, message: &str) -> Result<()> {
    let messages = vec![
        Message::system("You are a helpful coding assistant."),
        Message::user(message.to_string()),
    ];

    process_message(engine, &messages).await?;
    Ok(())
}

async fn run_goal(engine: &RiftEngine, goal: &str, self_correct: bool, verify: bool, auto: bool) -> Result<()> {
    let agent = engine.agent();
    
    println!("🧠 Planning tasks...\n");
    let mut job = match agent.plan_job(goal).await {
        Ok(job) => job,
        Err(e) => {
            anyhow::bail!("Planning failed: {}", e);
        }
    };

    println!("📋 Plan created with {} tasks:", job.tasks.len());
    // Build id -> name map for pretty dependency display
    let id_to_name: std::collections::HashMap<_, _> = job.tasks.iter()
        .map(|(id, task)| (*id, task.name.clone()))
        .collect();
    for (_, task) in &job.tasks {
        let deps = if task.dependencies.is_empty() {
            "none".to_string()
        } else {
            task.dependencies.iter()
                .map(|d| id_to_name.get(d).cloned().unwrap_or_else(|| d.to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!("  • {} (tool: {}, deps: {})", task.name, task.tool_name, deps);
    }
    println!();

    // Execute based on selected mode
    let (execution_result, verification) = if auto {
        println!("🤖 Executing in FULL AUTONOMOUS mode (context + self-correct + verify)...\n");
        let (result, verification) = engine.execute_job_autonomous(&mut job).await?;
        (Ok(result), Some(verification))
    } else if verify {
        println!("⚙️  Executing with verification...\n");
        let (result, verification) = engine.execute_job_with_verification(&mut job).await?;
        (Ok(result), Some(verification))
    } else if self_correct {
        println!("⚙️  Executing with self-correction enabled...\n");
        (engine.execute_job_with_self_correction(&mut job).await, None)
    } else {
        println!("⚙️  Executing...\n");
        (engine.execute_job(&mut job).await, None)
    };

    match execution_result {
        Ok(result) => {
            if result.success {
                println!("✅ All tasks completed successfully!");
                
                // Show verification results if available
                if let Some(verification) = verification {
                    println!("\n🔍 Verification Results:");
                    println!("   {}", verification.summary);
                    for check in &verification.checks {
                        let icon = if check.passed { "✅" } else { "❌" };
                        println!("   {} {}", icon, check.name);
                        if !check.passed {
                            println!("      Details: {}", check.details);
                        }
                    }
                }
            } else {
                println!("⚠️  Some tasks failed.");
                if self_correct || auto {
                    println!("   (Self-correction was attempted but couldn't resolve all issues)");
                } else {
                    println!("   (Hint: Use --self-correct or --auto for automatic recovery)");
                }
            }

            println!("\n📊 Results:");
            for (_, task) in &job.tasks {
                let icon = match task.status {
                    TaskStatus::Completed => "✅",
                    TaskStatus::Failed => "❌",
                    _ => "⏳",
                };
                println!("  {} {} - {:?}", icon, task.name, task.status);
                if let Some(ref res) = task.result {
                    let preview = if res.output.len() > 200 {
                        format!("{}...", &res.output[..200])
                    } else {
                        res.output.clone()
                    };
                    for line in preview.lines() {
                        println!("      {}", line);
                    }
                }
            }
        }
        Err(e) => {
            anyhow::bail!("Job execution failed: {}", e);
        }
    }

    Ok(())
}

async fn process_message(engine: &RiftEngine, _messages: &[Message]) -> Result<String> {
    let agent = engine.agent();
    let user_message = _messages.last()
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let response = agent.chat(&user_message).await?;
    println!("\n{}", response);
    Ok(response)
}

fn list_tools(engine: &RiftEngine) {
    println!("Available tools:");
    for name in engine.plugins().list_tools() {
        if let Some(tool) = engine.plugins().get_tool(name) {
            println!("  {} - {}", name, tool.description());
        }
    }
}

async fn handle_daemon_command(command: DaemonCommands, config: RiftConfig) -> Result<()> {
    use rift_core::{Daemon, DaemonClient, DaemonCommand, DaemonResponse, TaskQueue, QueuedTask};
    use rift_core::daemon::TaskStatus;
    use std::path::PathBuf;
    
    let socket_path = dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .map(|d| d.join("rift").join("daemon.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/rift-daemon.sock"));
    
    match command {
        DaemonCommands::Start { foreground, remote, port } => {
            println!("🚀 Starting Rift daemon...");
            
            // Check if daemon is already running
            let client = DaemonClient::with_unix_socket(&socket_path);
            if client.ping().await.unwrap_or(false) {
                println!("Daemon is already running!");
                return Ok(());
            }
            
            if foreground {
                // Run daemon in current process (blocking)
                let daemon = Daemon::new(config).await?;
                {
                    let mut d = daemon.write().await;
                    d.with_socket_path(&socket_path);
                }
                
                println!("✅ Daemon starting in foreground...");
                println!("Socket: {}", socket_path.display());
                if remote {
                    println!("Remote control: http://0.0.0.0:{}", port);
                }
                println!("Press Ctrl+C to stop");
                
                // Start remote server if enabled
                if remote {
                    let remote_server = rift_core::RemoteServer::new(daemon.clone(), port);
                    tokio::spawn(async move {
                        if let Err(e) = remote_server.run().await {
                            eprintln!("Remote server error: {}", e);
                        }
                    });
                }
                
                Daemon::start(daemon).await?;
            } else {
                // Spawn a detached child process
                let current_exe = std::env::current_exe()?;
                let mut cmd = std::process::Command::new(current_exe);
                cmd.arg("daemon")
                    .arg("start")
                    .arg("--foreground")
                    .env("RIFT_DAEMON", "1")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                
                // Set up the child process to be detached (platform-specific)
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    cmd.process_group(0); // Create new process group
                }
                
                cmd.spawn()?;
                
                // Give it time to start
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                
                // Check if started
                let client = DaemonClient::with_unix_socket(&socket_path);
                if client.ping().await.unwrap_or(false) {
                    println!("✅ Daemon started successfully!");
                    println!("Socket: {}", socket_path.display());
                } else {
                    println!("⚠️  Daemon may not have started properly");
                }
            }
        }
        DaemonCommands::Stop => {
            println!("🛑 Stopping Rift daemon...");
            
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.send(DaemonCommand::Stop).await {
                Ok(DaemonResponse::Stopping) => {
                    println!("✅ Daemon stopping...");
                }
                Ok(_) => {
                    println!("⚠️  Unexpected response from daemon");
                }
                Err(_) => {
                    println!("Daemon is not running");
                }
            }
        }
        DaemonCommands::Status => {
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.get_status().await {
                Ok(state) => {
                    println!("📊 Daemon Status");
                    println!("   Running: {}", if state.running { "✅ Yes" } else { "❌ No" });
                    println!("   Uptime: {} seconds", state.uptime_seconds);
                    println!("   Tasks completed: {}", state.tasks_completed);
                    println!("   Tasks failed: {}", state.tasks_failed);
                    if let Some(ref task) = state.current_task {
                        println!("   Current task: {} ({})", task.id, task.goal);
                    }
                    if let Some(ref activity) = state.last_activity {
                        println!("   Last activity: {}", activity);
                    }
                }
                Err(_) => {
                    println!("Daemon is not running");
                }
            }
        }
        DaemonCommands::Submit { goal } => {
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.submit_task(&goal).await {
                Ok(task_id) => {
                    println!("✅ Task submitted: {}", task_id);
                }
                Err(_) => {
                    println!("❌ Failed to submit task. Is the daemon running?");
                }
            }
        }
        DaemonCommands::Queue => {
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.send(DaemonCommand::ListPending).await {
                Ok(DaemonResponse::TaskList(tasks)) => {
                    if tasks.is_empty() {
                        println!("No pending tasks");
                    } else {
                        println!("📋 Pending Tasks:");
                        for task in tasks {
                            let status_icon = match task.status {
                                TaskStatus::Pending => "⏳",
                                TaskStatus::Running => "▶️",
                                _ => "❓",
                            };
                            println!("   {} {} - {} ({})", status_icon, &task.id[..8], task.goal, task.status);
                        }
                    }
                }
                Ok(_) => println!("Unexpected response"),
                Err(_) => println!("Daemon is not running"),
            }
        }
        DaemonCommands::History => {
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.send(DaemonCommand::ListRecent { limit: 10 }).await {
                Ok(DaemonResponse::TaskList(tasks)) => {
                    if tasks.is_empty() {
                        println!("No completed tasks");
                    } else {
                        println!("📜 Recent Tasks:");
                        for task in tasks {
                            let icon = match task.status {
                                TaskStatus::Completed => "✅",
                                TaskStatus::Failed => "❌",
                                TaskStatus::Cancelled => "🚫",
                                _ => "❓",
                            };
                            println!("   {} {} - {} ({})", icon, &task.id[..8], task.goal, task.status);
                        }
                    }
                }
                Ok(_) => println!("Unexpected response"),
                Err(_) => println!("Daemon is not running"),
            }
        }
        DaemonCommands::Cancel { task_id } => {
            let client = DaemonClient::with_unix_socket(&socket_path);
            match client.send(DaemonCommand::CancelTask { task_id }).await {
                Ok(DaemonResponse::Cancelled(true)) => {
                    println!("✅ Task cancelled");
                }
                Ok(DaemonResponse::Cancelled(false)) => {
                    println!("⚠️  Task not found or already running/completed");
                }
                Ok(_) => println!("Unexpected response"),
                Err(_) => println!("Daemon is not running"),
            }
        }
    }
    
    Ok(())
}
