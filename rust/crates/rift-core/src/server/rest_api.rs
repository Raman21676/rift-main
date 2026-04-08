//! REST API and WebSocket server using axum

use axum::{
    Router,
    routing::{get, post},
    extract::{State, Query, WebSocketUpgrade, Path},
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::daemon::{Daemon, QueuedTask, TaskStatus, QueueStatus};
use super::auth::AuthManager;
use super::websocket::ws_handler;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub daemon: Arc<RwLock<Daemon>>,
    pub auth: Arc<AuthManager>,
}

/// Token query parameter
#[derive(Deserialize)]
pub struct TokenQuery {
    token: String,
}

/// Submit task request body
#[derive(Deserialize)]
pub struct SubmitTaskBody {
    goal: String,
    #[serde(default)]
    auto_correct: bool,
    #[serde(default)]
    verify: bool,
}

/// Task submission response
#[derive(Serialize)]
pub struct SubmitTaskResponse {
    task_id: String,
    status: String,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
}

/// Start the REST API server
pub async fn start(
    daemon: Arc<RwLock<Daemon>>,
    auth: Arc<AuthManager>,
    port: u16,
) -> anyhow::Result<()> {
    let state = AppState { daemon, auth };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(get_status))
        .route("/api/queue", get(get_queue))
        .route("/api/history", get(get_history))
        .route("/api/tasks", post(submit_task))
        .route("/api/tasks/:id/cancel", post(cancel_task))
        .route("/ws", get(ws_upgrade))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    println!("🚀 Rift Remote server on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Health check endpoint
async fn health() -> &'static str {
    "ok"
}

/// Get daemon status
async fn get_status(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "unauthorized".to_string() })).into_response();
    }
    
    let daemon = state.daemon.read().await;
    let daemon_state = daemon.get_state().await;
    let queue_status = match daemon.get_queue_status().await {
        Ok(s) => s,
        Err(_) => QueueStatus { pending: 0, running: 0, completed: 0, failed: 0, total: 0 },
    };
    
    Json(serde_json::json!({
        "running": daemon_state.running,
        "uptime_seconds": daemon_state.uptime_seconds,
        "tasks_completed": daemon_state.tasks_completed,
        "tasks_failed": daemon_state.tasks_failed,
        "current_task": daemon_state.current_task.map(|t| serde_json::json!({
            "id": t.id,
            "goal": t.goal,
            "status": t.status,
        })),
        "queue": {
            "pending": queue_status.pending,
            "running": queue_status.running,
            "completed": queue_status.completed,
            "failed": queue_status.failed,
        },
        "version": daemon_state.version,
    })).into_response()
}

/// Get task queue
async fn get_queue(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "unauthorized".to_string() })).into_response();
    }
    
    let daemon = state.daemon.read().await;
    match daemon.get_pending_tasks().await {
        Ok(tasks) => Json(tasks_to_json(tasks)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// Get task history
async fn get_history(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "unauthorized".to_string() })).into_response();
    }
    
    let daemon = state.daemon.read().await;
    match daemon.get_recent_tasks(50).await {
        Ok(tasks) => Json(tasks_to_json(tasks)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// Submit a new task
async fn submit_task(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
    Json(body): Json<SubmitTaskBody>,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "unauthorized".to_string() })).into_response();
    }
    
    let daemon = state.daemon.read().await;
    match daemon.submit_task(body.goal).await {
        Ok(task_id) => Json(SubmitTaskResponse { 
            task_id, 
            status: "queued".to_string() 
        }).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// Cancel a task
async fn cancel_task(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "unauthorized".to_string() })).into_response();
    }
    
    let daemon = state.daemon.read().await;
    match daemon.cancel_task(&task_id).await {
        Ok(cancelled) => Json(serde_json::json!({ 
            "cancelled": cancelled,
            "task_id": task_id 
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response(),
    }
}

/// WebSocket upgrade endpoint
async fn ws_upgrade(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !state.auth.validate(&q.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    
    ws.on_upgrade(move |socket| ws_handler(socket, state.daemon.clone(), state.auth.clone()))
}

/// Convert queued tasks to JSON-compatible format
fn tasks_to_json(tasks: Vec<QueuedTask>) -> Vec<serde_json::Value> {
    tasks.into_iter().map(|t| serde_json::json!({
        "id": t.id,
        "goal": t.goal,
        "status": t.status.to_string(),
        "created_at": t.created_at,
        "started_at": t.started_at,
        "completed_at": t.completed_at,
        "result": t.result,
        "priority": t.priority,
    })).collect()
}
