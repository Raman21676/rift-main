//! Rift CLI - AI coding assistant

use anyhow::Result;
use clap::{Parser, Subcommand};
use rift_core::{Message, RiftConfig, RiftEngine};
use std::io::Write;

#[derive(Parser)]
#[command(name = "rift")]
#[command(about = "AI coding assistant with plugin-based architecture")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// API key (or set OPENAI_API_KEY env var)
    #[arg(long, env = "OPENAI_API_KEY")]
    api_key: Option<String>,
    
    /// Model to use
    #[arg(long, env = "RIFT_MODEL", default_value = "gpt-4o-mini")]
    model: String,
    
    /// API base URL (default: OpenAI, use https://openrouter.ai/api/v1 for OpenRouter)
    #[arg(long, env = "RIFT_BASE_URL", default_value = "https://api.openai.com/v1")]
    base_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session
    Chat {
        /// Initial message to send
        #[arg(short, long)]
        message: Option<String>,
    },
    
    /// Execute a single command
    Run {
        /// The command to execute
        message: String,
    },
    
    /// List available tools
    Tools,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    
    let api_key = cli.api_key.ok_or_else(|| {
        anyhow::anyhow!("API key required. Set OPENAI_API_KEY or use --api-key")
    })?;
    
    let config = RiftConfig::new(api_key)
        .with_model(cli.model)
        .with_base_url(cli.base_url);
    let mut engine = RiftEngine::new(config);
    
    // Register built-in tools
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
        rift_tools::builtin::WebFetchTool::new()
    ));
    engine.plugins_mut().register_tool(std::sync::Arc::new(
        rift_tools::builtin::WebSearchTool::new()
    ));
    
    match cli.command {
        Commands::Chat { message } => {
            run_chat(&engine, message).await?;
        }
        Commands::Run { message } => {
            run_single(&engine, &message).await?;
        }
        Commands::Tools => {
            list_tools(&engine);
        }
    }
    
    Ok(())
}

async fn run_chat(engine: &RiftEngine, initial: Option<String>) -> Result<()> {
    println!("🌊 Rift - AI Coding Assistant");
    println!("Type 'exit' or 'quit' to exit\n");
    
    let mut messages = vec![
        Message::system("You are a helpful coding assistant. Use tools when appropriate."),
    ];
    
    if let Some(msg) = initial {
        messages.push(Message::user(msg));
        process_message(engine, &messages).await?;
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

async fn run_single(engine: &RiftEngine, message: &str) -> Result<()> {
    let messages = vec![
        Message::system("You are a helpful coding assistant."),
        Message::user(message.to_string()),
    ];
    
    process_message(engine, &messages).await?;
    Ok(())
}

async fn process_message(engine: &RiftEngine, _messages: &[Message]) -> Result<String> {
    // Use agent for tool-enabled chat
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
