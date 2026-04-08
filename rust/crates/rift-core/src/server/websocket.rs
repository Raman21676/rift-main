//! WebSocket handler for real-time communication

use axum::extract::ws::{WebSocket, Message};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::daemon::Daemon;
use super::auth::AuthManager;

// Message type constants - must match Android client
const MSG_STATUS: u8 = 0x01;
const MSG_TASK_EVENT: u8 = 0x02;
const MSG_COMMAND: u8 = 0x03;
const MSG_PING: u8 = 0x04;
const MSG_PONG: u8 = 0x05;

/// Task event for WebSocket streaming
#[derive(Clone, Debug, Serialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub status: String,
    pub log_line: Option<String>,
    pub timestamp: u64,
}

/// Command from client
#[derive(Clone, Debug, Deserialize)]
pub struct ClientCommand {
    pub action: String,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Status update message
#[derive(Clone, Debug, Serialize)]
pub struct StatusUpdate {
    pub running: bool,
    pub uptime_seconds: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub current_task: Option<CurrentTaskInfo>,
    pub queue_pending: usize,
    pub queue_running: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrentTaskInfo {
    pub id: String,
    pub goal: String,
    pub status: String,
}

/// Handle WebSocket connection
pub async fn ws_handler(
    mut socket: WebSocket,
    daemon: Arc<RwLock<Daemon>>,
    _auth: Arc<AuthManager>,
) {
    println!("📱 Remote client connected via WebSocket");

    // Create a channel for broadcasting events to this client
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TaskEvent>(100);
    
    // Spawn a task to forward events from the broadcast channel to the WebSocket
    let mut forward_task = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(_) => continue,
            };
            let mut msg = vec![MSG_TASK_EVENT];
            msg.extend_from_slice(json.as_bytes());
            // This will fail if socket is closed, which is handled
        }
    });

    let mut ping_ticker = interval(Duration::from_secs(15));
    let mut last_pong = std::time::Instant::now();

    loop {
        tokio::select! {
            // Receive from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) if !data.is_empty() => {
                        match data[0] {
                            MSG_PING => {
                                let pong = vec![MSG_PONG];
                                if socket.send(Message::Binary(pong)).await.is_err() {
                                    break;
                                }
                            }
                            MSG_COMMAND => {
                                if let Ok(json) = serde_json::from_slice::<ClientCommand>(&data[1..]) {
                                    handle_command(json, &daemon, &tx).await;
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        // Handle text commands too for debugging
                        if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                            handle_command(cmd, &daemon, &tx).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        println!("📱 Remote client disconnected");
                        break;
                    }
                    _ => {}
                }
            }

            // Periodic status push (every 15s)
            _ = ping_ticker.tick() => {
                // Check if client is still alive
                if last_pong.elapsed() > Duration::from_secs(45) {
                    println!("📱 Client timeout - no pong received");
                    break;
                }

                let status = build_status(&daemon).await;
                let json = match serde_json::to_string(&status) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                let mut msg = vec![MSG_STATUS];
                msg.extend_from_slice(json.as_bytes());
                if socket.send(Message::Binary(msg)).await.is_err() {
                    break;
                }
            }
        }
    }

    // Clean up
    forward_task.abort();
    println!("📱 WebSocket connection closed");
}

/// Build current status from daemon
async fn build_status(daemon: &Arc<RwLock<Daemon>>) -> StatusUpdate {
    let d = daemon.read().await;
    let state = d.get_state().await;
    let queue_status = d.get_queue_status().await.unwrap_or(
        crate::daemon::QueueStatus { pending: 0, running: 0, completed: 0, failed: 0, total: 0 }
    );
    
    StatusUpdate {
        running: state.running,
        uptime_seconds: state.uptime_seconds,
        tasks_completed: state.tasks_completed,
        tasks_failed: state.tasks_failed,
        current_task: state.current_task.map(|t| CurrentTaskInfo {
            id: t.id,
            goal: t.goal,
            status: t.status.to_string(),
        }),
        queue_pending: queue_status.pending,
        queue_running: queue_status.running,
    }
}

/// Handle a command from the client
async fn handle_command(
    cmd: ClientCommand, 
    daemon: &Arc<RwLock<Daemon>>,
    _event_tx: &tokio::sync::mpsc::Sender<TaskEvent>,
) {
    match cmd.action.as_str() {
        "submit_task" => {
            if let Some(goal) = cmd.goal {
                let d = daemon.read().await;
                match d.submit_task(goal).await {
                    Ok(task_id) => {
                        println!("📱 Remote task submitted: {}", task_id);
                        // TODO: Broadcast to all connected clients
                    }
                    Err(e) => {
                        println!("📱 Failed to submit remote task: {}", e);
                    }
                }
            }
        }
        "cancel_task" => {
            if let Some(task_id) = cmd.task_id {
                let d = daemon.read().await;
                match d.cancel_task(&task_id).await {
                    Ok(cancelled) => {
                        println!("📱 Remote task {} cancelled: {}", task_id, cancelled);
                    }
                    Err(e) => {
                        println!("📱 Failed to cancel remote task: {}", e);
                    }
                }
            }
        }
        "get_status" => {
            // Status is already pushed periodically
        }
        _ => {
            println!("📱 Unknown command: {}", cmd.action);
        }
    }
}

/// Broadcast a task event to all connected clients
/// This is called from the daemon when tasks progress
pub async fn broadcast_task_event(
    _event: TaskEvent,
    _clients: &Vec<tokio::sync::mpsc::Sender<TaskEvent>>,
) {
    // TODO: Implement client registry to broadcast to all connected clients
}
