//! Daemon control server
//!
//! Provides a Unix socket / TCP interface for controlling the daemon:
//! - Submit tasks
//! - Check status
//! - View queue
//! - Cancel tasks
//! - Stop daemon

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

use super::{Daemon, DaemonError, DaemonState, QueueStatus};

/// Commands that can be sent to the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonCommand {
    /// Submit a new task
    SubmitTask { goal: String },
    /// Get daemon status
    GetStatus,
    /// Get queue status
    GetQueueStatus,
    /// List pending tasks
    ListPending,
    /// List recent tasks
    ListRecent { limit: usize },
    /// Cancel a task
    CancelTask { task_id: String },
    /// Get task details
    GetTask { task_id: String },
    /// Stop the daemon
    Stop,
    /// Ping (health check)
    Ping,
}

/// Responses from the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Task submitted successfully
    TaskSubmitted { task_id: String },
    /// Current daemon status
    Status(DaemonState),
    /// Queue status
    QueueStatus(QueueStatus),
    /// List of tasks
    TaskList(Vec<super::QueuedTask>),
    /// Single task details
    Task(Option<super::QueuedTask>),
    /// Task cancelled (true if found and cancelled)
    Cancelled(bool),
    /// Daemon stopping
    Stopping,
    /// Pong (health check response)
    Pong,
    /// Error
    Error { message: String },
    /// Success
    Success { message: String },
}

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Server for daemon control
pub struct DaemonServer {
    daemon: Arc<RwLock<Daemon>>,
    socket_path: Option<PathBuf>,
    tcp_port: Option<u16>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl DaemonServer {
    /// Create a new daemon server with Unix socket
    pub async fn with_unix_socket(daemon: Daemon, socket_path: impl Into<PathBuf>) -> Self {
        Self {
            daemon: Arc::new(RwLock::new(daemon)),
            socket_path: Some(socket_path.into()),
            tcp_port: None,
            shutdown_tx: None,
        }
    }

    /// Create a new daemon server with TCP
    pub async fn with_tcp(daemon: Daemon, port: u16) -> Self {
        Self {
            daemon: Arc::new(RwLock::new(daemon)),
            socket_path: None,
            tcp_port: Some(port),
            shutdown_tx: None,
        }
    }
    
    /// Create from an existing Arc with shutdown channel
    pub fn from_arc_with_shutdown(
        daemon: Arc<RwLock<Daemon>>, 
        socket_path: PathBuf,
        shutdown_tx: mpsc::Sender<()>
    ) -> Self {
        Self {
            daemon,
            socket_path: Some(socket_path),
            tcp_port: None,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Start the server
    pub async fn start(&self) -> Result<(), DaemonError> {
        self.run().await
    }
    
    /// Run the server (blocks until error or shutdown)
    pub async fn run(&self) -> Result<(), DaemonError> {
        if let Some(ref socket_path) = self.socket_path {
            self.run_unix_socket(socket_path).await
        } else if let Some(port) = self.tcp_port {
            self.run_tcp(port).await
        } else {
            Err(DaemonError::Execution("No transport configured".to_string()))
        }
    }

    /// Run Unix socket server
    async fn run_unix_socket(&self, socket_path: &PathBuf) -> Result<(), DaemonError> {
        // Remove old socket if exists
        let _ = tokio::fs::remove_file(socket_path).await;
        
        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| DaemonError::Io(e))?;
        }

        let listener = UnixListener::bind(socket_path)
            .map_err(|e| DaemonError::Io(e))?;

        info!("Daemon server listening on Unix socket: {}", socket_path.display());

        let shutdown_tx = self.shutdown_tx.clone();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let daemon = self.daemon.clone();
                    let shutdown_tx = shutdown_tx.clone();
                    tokio::spawn(async move {
                        let daemon = daemon.read().await;
                        if let Err(e) = Self::handle_unix_client(stream, &daemon, &shutdown_tx).await {
                            error!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Run TCP server
    async fn run_tcp(&self, port: u16) -> Result<(), DaemonError> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .map_err(|e| DaemonError::Io(e))?;

        info!("Daemon server listening on TCP port: {}", port);

        let shutdown_tx = self.shutdown_tx.clone();

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let daemon = self.daemon.clone();
                    let shutdown_tx = shutdown_tx.clone();
                    tokio::spawn(async move {
                        let daemon = daemon.read().await;
                        if let Err(e) = Self::handle_tcp_client(stream, &daemon, &shutdown_tx).await {
                            error!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handle a Unix socket client
    async fn handle_unix_client(
        mut stream: UnixStream, 
        daemon: &Daemon,
        shutdown_tx: &Option<mpsc::Sender<()>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 4096];
        let n = stream.read(&mut buffer).await?;
        
        if n == 0 {
            return Ok(());
        }

        let command: DaemonCommand = serde_json::from_slice(&buffer[..n])?;
        let response = Self::handle_command(command, daemon, shutdown_tx).await;
        
        let response_json = serde_json::to_vec(&response)?;
        stream.write_all(&response_json).await?;
        
        Ok(())
    }

    /// Handle a TCP client
    async fn handle_tcp_client(
        mut stream: TcpStream, 
        daemon: &Daemon,
        shutdown_tx: &Option<mpsc::Sender<()>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = vec![0u8; 4096];
        let n = stream.read(&mut buffer).await?;
        
        if n == 0 {
            return Ok(());
        }

        let command: DaemonCommand = serde_json::from_slice(&buffer[..n])?;
        let response = Self::handle_command(command, daemon, shutdown_tx).await;
        
        let response_json = serde_json::to_vec(&response)?;
        stream.write_all(&response_json).await?;
        
        Ok(())
    }

    /// Handle a command
    async fn handle_command(
        command: DaemonCommand, 
        daemon: &Daemon,
        shutdown_tx: &Option<mpsc::Sender<()>>,
    ) -> DaemonResponse {
        match command {
            DaemonCommand::SubmitTask { goal } => {
                match daemon.submit_task(goal).await {
                    Ok(task_id) => DaemonResponse::TaskSubmitted { task_id },
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::GetStatus => {
                let state = daemon.get_state().await;
                DaemonResponse::Status(state)
            }
            DaemonCommand::GetQueueStatus => {
                match daemon.get_queue_status().await {
                    Ok(status) => DaemonResponse::QueueStatus(status),
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::ListPending => {
                match daemon.get_pending_tasks().await {
                    Ok(tasks) => DaemonResponse::TaskList(tasks),
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::ListRecent { limit } => {
                match daemon.get_recent_tasks(limit).await {
                    Ok(tasks) => DaemonResponse::TaskList(tasks),
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::CancelTask { task_id } => {
                match daemon.cancel_task(&task_id).await {
                    Ok(cancelled) => DaemonResponse::Cancelled(cancelled),
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::GetTask { task_id } => {
                match daemon.get_task(&task_id).await {
                    Ok(task) => DaemonResponse::Task(task),
                    Err(e) => DaemonResponse::Error { message: e.to_string() },
                }
            }
            DaemonCommand::Stop => {
                // Trigger shutdown
                if let Some(ref tx) = shutdown_tx {
                    let _ = tx.try_send(());
                }
                DaemonResponse::Stopping
            }
            DaemonCommand::Ping => {
                DaemonResponse::Pong
            }
        }
    }
}

/// Client for connecting to daemon
pub struct DaemonClient {
    socket_path: Option<PathBuf>,
    tcp_addr: Option<String>,
}

impl DaemonClient {
    /// Create client with Unix socket
    pub fn with_unix_socket(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: Some(socket_path.into()),
            tcp_addr: None,
        }
    }

    /// Create client with TCP
    pub fn with_tcp(port: u16) -> Self {
        Self {
            socket_path: None,
            tcp_addr: Some(format!("127.0.0.1:{}", port)),
        }
    }

    /// Send a command and get response
    pub async fn send(&self, command: DaemonCommand) -> Result<DaemonResponse, ClientError> {
        if let Some(ref socket_path) = self.socket_path {
            self.send_unix(socket_path, command).await
        } else if let Some(ref addr) = self.tcp_addr {
            self.send_tcp(addr, command).await
        } else {
            Err(ClientError::NotConfigured)
        }
    }

    async fn send_unix(
        &self, 
        socket_path: &PathBuf, 
        command: DaemonCommand
    ) -> Result<DaemonResponse, ClientError> {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .map_err(ClientError::Io)?;

        let command_json = serde_json::to_vec(&command)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        stream.write_all(&command_json).await.map_err(ClientError::Io)?;

        let mut buffer = vec![0u8; 4096];
        let n = stream.read(&mut buffer).await.map_err(ClientError::Io)?;
        
        let response: DaemonResponse = serde_json::from_slice(&buffer[..n])
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        Ok(response)
    }

    async fn send_tcp(
        &self, 
        addr: &str, 
        command: DaemonCommand
    ) -> Result<DaemonResponse, ClientError> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(ClientError::Io)?;

        let command_json = serde_json::to_vec(&command)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        stream.write_all(&command_json).await.map_err(ClientError::Io)?;

        let mut buffer = vec![0u8; 4096];
        let n = stream.read(&mut buffer).await.map_err(ClientError::Io)?;
        
        let response: DaemonResponse = serde_json::from_slice(&buffer[..n])
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        Ok(response)
    }

    /// Convenience: Submit a task
    pub async fn submit_task(&self, goal: impl Into<String>) -> Result<String, ClientError> {
        match self.send(DaemonCommand::SubmitTask { goal: goal.into() }).await? {
            DaemonResponse::TaskSubmitted { task_id } => Ok(task_id),
            DaemonResponse::Error { message } => Err(ClientError::Daemon(message)),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Convenience: Get status
    pub async fn get_status(&self) -> Result<DaemonState, ClientError> {
        match self.send(DaemonCommand::GetStatus).await? {
            DaemonResponse::Status(state) => Ok(state),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    /// Convenience: Check if daemon is running
    pub async fn ping(&self) -> Result<bool, ClientError> {
        match self.send(DaemonCommand::Ping).await {
            Ok(DaemonResponse::Pong) => Ok(true),
            Ok(_) => Ok(false),
            Err(ClientError::Io(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Client errors
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Daemon error: {0}")]
    Daemon(String),
    
    #[error("Unexpected response from daemon")]
    UnexpectedResponse,
    
    #[error("Client not configured with transport")]
    NotConfigured,
}
