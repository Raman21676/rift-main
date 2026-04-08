//! Background daemon mode for 24/7 autonomous operation
//!
//! The daemon runs continuously, processing tasks from a queue:
//! 1. Polls for new tasks from SQLite queue
//! 2. Executes tasks using the autonomous engine
//! 3. Sends notifications on completion/failure
//! 4. Maintains logs and metrics

use crate::task::{Job, TaskId};
use crate::{RiftConfig, RiftEngine};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

pub mod queue;
pub mod server;

pub use queue::{TaskQueue, QueuedTask, TaskStatus};
pub use server::{DaemonServer, DaemonCommand, DaemonResponse, DaemonClient};

/// Background daemon for autonomous operation
pub struct Daemon {
    config: RiftConfig,
    engine: Arc<RwLock<RiftEngine>>,
    queue: Arc<RwLock<TaskQueue>>,
    state: Arc<RwLock<DaemonState>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    socket_path: Option<PathBuf>,
}

/// Current state of the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub running: bool,
    pub current_task: Option<QueuedTask>,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub uptime_seconds: u64,
    pub last_activity: Option<String>,
    pub version: String,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            running: false,
            current_task: None,
            tasks_completed: 0,
            tasks_failed: 0,
            uptime_seconds: 0,
            last_activity: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl Daemon {
    /// Create a new daemon instance
    pub async fn new(config: RiftConfig) -> Result<Arc<RwLock<Self>>, DaemonError> {
        let engine = RiftEngine::new(config.clone());
        let queue = TaskQueue::new().await?;
        
        Ok(Arc::new(RwLock::new(Self {
            config,
            engine: Arc::new(RwLock::new(engine)),
            queue: Arc::new(RwLock::new(queue)),
            state: Arc::new(RwLock::new(DaemonState::default())),
            shutdown_tx: None,
            socket_path: None,
        })))
    }
    
    /// Set the Unix socket path for the control server
    pub fn with_socket_path(&mut self, path: impl Into<PathBuf>) {
        self.socket_path = Some(path.into());
    }

    /// Start the daemon and block until shutdown (must be called on an Arc<RwLock<Daemon>>)
    pub async fn start(self_arc: Arc<RwLock<Self>>) -> Result<(), DaemonError> {
        info!("Starting Rift daemon...");
        
        let socket_path = {
            let daemon = self_arc.read().await;
            
            // Update state
            {
                let mut state = daemon.state.write().await;
                state.running = true;
            }
            
            daemon.socket_path.clone()
        };

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);
        let (_server_shutdown_tx, _server_shutdown_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);
        {
            let mut daemon = self_arc.write().await;
            daemon.shutdown_tx = Some(shutdown_tx);
        }

        // Spawn processing loop
        let loop_arc = self_arc.clone();
        let processing_handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(5)); // Check queue every 5 seconds
            let start_time = std::time::Instant::now();

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let daemon = loop_arc.read().await;
                        
                        // Update uptime
                        {
                            let mut s = daemon.state.write().await;
                            s.uptime_seconds = start_time.elapsed().as_secs();
                        }

                        // Process next task if available
                        if let Err(e) = Self::process_next_task(
                            &daemon.engine, 
                            &daemon.queue, 
                            &daemon.state,
                            &daemon.config
                        ).await {
                            error!("Error processing task: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Daemon shutdown signal received");
                        break;
                    }
                }
            }

            // Update state on shutdown
            let daemon = loop_arc.read().await;
            let mut s = daemon.state.write().await;
            s.running = false;
            info!("Processing loop stopped");
        });
        
        // Start the control server if socket path is configured
        let server_handle = if let Some(socket_path) = socket_path {
            let (server_shutdown_tx, mut server_shutdown_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);
            let server = DaemonServer::from_arc_with_shutdown(self_arc.clone(), socket_path, server_shutdown_tx);
            Some(tokio::spawn(async move {
                tokio::select! {
                    result = server.run() => {
                        if let Err(e) = result {
                            error!("Control server error: {}", e);
                        }
                    }
                    _ = server_shutdown_rx.recv() => {
                        info!("Control server shutting down");
                    }
                }
            }))
        } else {
            None
        };

        info!("Rift daemon started successfully");
        
        // Wait for shutdown signal (block here)
        if let Some(handle) = server_handle {
            tokio::select! {
                _ = handle => {},
                _ = processing_handle => {},
            }
        } else {
            processing_handle.await.ok();
        }
        
        info!("Rift daemon stopped");
        Ok(())
    }

    /// Stop the daemon
    pub async fn stop(&mut self) -> Result<(), DaemonError> {
        info!("Stopping Rift daemon...");
        
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        Ok(())
    }

    /// Process the next task from the queue
    async fn process_next_task(
        engine: &Arc<RwLock<RiftEngine>>,
        queue: &Arc<RwLock<TaskQueue>>,
        state: &Arc<RwLock<DaemonState>>,
        config: &RiftConfig,
    ) -> Result<(), DaemonError> {
        // Try to get next pending task
        let next_task = {
            let q = queue.read().await;
            q.dequeue().await?
        };

        if let Some(mut task) = next_task {
            info!("Processing task: {} ({})", task.id, task.goal);
            
            // Update state
            {
                let mut s = state.write().await;
                s.current_task = Some(task.clone());
                s.last_activity = Some(chrono::Local::now().to_rfc3339());
            }

            // Execute the task
            let result = Self::execute_task(engine, &task, config).await;

            // Update task status
            match result {
                Ok(success) => {
                    let mut q = queue.write().await;
                    if success {
                        q.mark_completed(&task.id, "Task completed successfully").await?;
                        drop(q); // Release queue lock before acquiring state lock
                        
                        let mut s = state.write().await;
                        s.tasks_completed += 1;
                        info!("Task {} completed successfully", task.id);
                    } else {
                        q.update_status(&task.id, TaskStatus::Failed).await?;
                        q.mark_completed(&task.id, "Task failed").await?;
                        drop(q);
                        
                        let mut s = state.write().await;
                        s.tasks_failed += 1;
                        warn!("Task {} failed", task.id);
                    }
                }
                Err(e) => {
                    let mut q = queue.write().await;
                    q.update_status(&task.id, TaskStatus::Failed).await?;
                    q.mark_completed(&task.id, &format!("Error: {}", e)).await?;
                    drop(q);
                    
                    let mut s = state.write().await;
                    s.tasks_failed += 1;
                    error!("Task {} error: {}", task.id, e);
                }
            }

            // Clear current task
            {
                let mut s = state.write().await;
                s.current_task = None;
            }
        }

        Ok(())
    }

    /// Execute a single task
    async fn execute_task(
        engine: &Arc<RwLock<RiftEngine>>,
        task: &QueuedTask,
        _config: &RiftConfig,
    ) -> Result<bool, DaemonError> {
        // Get the agent and plan the job
        let agent = {
            let eng = engine.read().await;
            eng.agent()
        };

        // Plan the job
        let mut job = match agent.plan_job(&task.goal).await {
            Ok(job) => job,
            Err(e) => {
                error!("Failed to plan task {}: {}", task.id, e);
                return Ok(false);
            }
        };

        // Execute with autonomous mode (context + self-correct + verify)
        let eng = engine.read().await;
        let (result, verification) = match eng.execute_job_autonomous(&mut job).await {
            Ok((r, v)) => (r, v),
            Err(e) => {
                error!("Task {} execution error: {}", task.id, e);
                return Ok(false);
            }
        };

        // Check both execution result and verification
        let success = result.success && verification.success;
        
        if !verification.success {
            warn!("Task {} verification failed: {}", task.id, verification.summary);
        }

        Ok(success)
    }

    /// Get current daemon state
    pub async fn get_state(&self) -> DaemonState {
        self.state.read().await.clone()
    }

    /// Submit a new task to the queue
    pub async fn submit_task(&self, goal: impl Into<String>) -> Result<String, DaemonError> {
        let q = self.queue.read().await;
        let task_id = q.enqueue(goal.into()).await?;
        info!("Submitted task: {}", task_id);
        Ok(task_id)
    }

    /// Get queue status
    pub async fn get_queue_status(&self) -> Result<QueueStatus, DaemonError> {
        let q = self.queue.read().await;
        q.get_status().await.map_err(|e| e.into())
    }

    /// Get pending tasks
    pub async fn get_pending_tasks(&self) -> Result<Vec<QueuedTask>, DaemonError> {
        let q = self.queue.read().await;
        q.list_pending().await.map_err(|e| e.into())
    }

    /// Get recent completed tasks
    pub async fn get_recent_tasks(&self, limit: usize) -> Result<Vec<QueuedTask>, DaemonError> {
        let q = self.queue.read().await;
        q.list_recent(limit).await.map_err(|e| e.into())
    }

    /// Cancel a pending task
    pub async fn cancel_task(&self, task_id: &str) -> Result<bool, DaemonError> {
        let mut q = self.queue.write().await;
        q.cancel(task_id).await.map_err(|e| e.into())
    }
    
    /// Get a specific task by ID
    pub async fn get_task(&self, task_id: &str) -> Result<Option<QueuedTask>, DaemonError> {
        let q = self.queue.read().await;
        q.get_task(task_id).await.map_err(|e| e.into())
    }
}

/// Queue status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub total: usize,
}

/// Errors that can occur in the daemon
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Queue error: {0}")]
    Queue(String),
    
    #[error("Execution error: {0}")]
    Execution(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<queue::QueueError> for DaemonError {
    fn from(e: queue::QueueError) -> Self {
        DaemonError::Queue(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_state_default() {
        let state = DaemonState::default();
        assert!(!state.running);
        assert_eq!(state.tasks_completed, 0);
    }
}
