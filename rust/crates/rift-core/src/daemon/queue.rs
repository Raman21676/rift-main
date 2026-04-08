//! Task queue for the daemon using SQLite

use chrono;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};

/// A task in the queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedTask {
    pub id: String,
    pub goal: String,
    pub status: TaskStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result: Option<String>,
    pub priority: i32, // Higher = more priority
}

/// Task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Task queue backed by SQLite
pub struct TaskQueue {
    db_path: PathBuf,
}

impl TaskQueue {
    /// Create or open the task queue database
    pub async fn new() -> Result<Self, QueueError> {
        let db_path = Self::db_path()?;
        
        // Ensure directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| QueueError::Io(e.to_string()))?;
        }

        info!("Task queue initialized at {}", db_path.display());
        
        let queue = Self { db_path };
        queue.init().await?;
        
        Ok(queue)
    }

    /// Get the database path
    fn db_path() -> Result<PathBuf, QueueError> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| QueueError::Config("Could not find config directory".to_string()))?;
        Ok(config_dir.join("rift").join("daemon.db"))
    }

    /// Get a database connection
    fn conn(&self) -> Result<Connection, QueueError> {
        Connection::open(&self.db_path)
            .map_err(|e| QueueError::Database(e.to_string()))
    }

    /// Initialize the database schema
    async fn init(&self) -> Result<(), QueueError> {
        let conn = self.conn()?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                result TEXT,
                priority INTEGER DEFAULT 0
            )",
            [],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        // Index for efficient querying
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)",
            [],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority DESC, created_at ASC)",
            [],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(())
    }

    /// Add a new task to the queue
    pub async fn enqueue(&self, goal: impl Into<String>) -> Result<String, QueueError> {
        let id = uuid::Uuid::new_v4().to_string();
        let goal = goal.into();
        let created_at = chrono::Local::now().to_rfc3339();
        
        let conn = self.conn()?;

        conn.execute(
            "INSERT INTO tasks (id, goal, status, created_at, priority) 
             VALUES (?1, ?2, 'pending', ?3, 0)",
            params![&id, &goal, &created_at],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        debug!("Enqueued task {}: {}", id, goal);
        Ok(id)
    }

    /// Get the next pending task (highest priority, oldest first)
    pub async fn dequeue(&self) -> Result<Option<QueuedTask>, QueueError> {
        let conn = self.conn()?;
        
        // Find the next pending task
        let task: Option<(String, String, String, i32)> = conn.query_row(
            "SELECT id, goal, created_at, priority 
             FROM tasks 
             WHERE status = 'pending'
             ORDER BY priority DESC, created_at ASC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                ))
            },
        ).optional().map_err(|e| QueueError::Database(e.to_string()))?;

        if let Some((id, goal, created_at, priority)) = task {
            let started_at = Some(chrono::Local::now().to_rfc3339());
            
            // Mark as running
            conn.execute(
                "UPDATE tasks SET status = 'running', started_at = ?1 WHERE id = ?2",
                params![&started_at, &id],
            ).map_err(|e| QueueError::Database(e.to_string()))?;

            Ok(Some(QueuedTask {
                id,
                goal,
                status: TaskStatus::Running,
                created_at,
                started_at,
                completed_at: None,
                result: None,
                priority,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update task status
    pub async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<(), QueueError> {
        let conn = self.conn()?;
        
        conn.execute(
            "UPDATE tasks SET status = ?1 WHERE id = ?2",
            params![status.to_string(), task_id],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mark task as completed with result
    pub async fn mark_completed(&self, task_id: &str, result: impl Into<String>) -> Result<(), QueueError> {
        let conn = self.conn()?;
        let completed_at = chrono::Local::now().to_rfc3339();
        let result = result.into();

        conn.execute(
            "UPDATE tasks 
             SET status = 'completed', completed_at = ?1, result = ?2 
             WHERE id = ?3",
            params![&completed_at, &result, task_id],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mark task as failed
    pub async fn mark_failed(&self, task_id: &str, error: impl Into<String>) -> Result<(), QueueError> {
        let conn = self.conn()?;
        let completed_at = chrono::Local::now().to_rfc3339();
        let error = error.into();

        conn.execute(
            "UPDATE tasks 
             SET status = 'failed', completed_at = ?1, result = ?2 
             WHERE id = ?3",
            params![&completed_at, &error, task_id],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(())
    }

    /// Cancel a pending task
    pub async fn cancel(&self, task_id: &str) -> Result<bool, QueueError> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE tasks SET status = 'cancelled' WHERE id = ?1 AND status = 'pending'",
            params![task_id],
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(affected > 0)
    }

    /// Get queue status summary
    pub async fn get_status(&self) -> Result<super::QueueStatus, QueueError> {
        let conn = self.conn()?;
        
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*) FROM tasks GROUP BY status"
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        let counts: Vec<(String, i64)> = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
            ))
        }).map_err(|e| QueueError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;

        for (status, count) in counts {
            match status.as_str() {
                "pending" => pending = count as usize,
                "running" => running = count as usize,
                "completed" => completed = count as usize,
                "failed" => failed = count as usize,
                _ => {}
            }
        }

        let total = pending + running + completed + failed;

        Ok(super::QueueStatus {
            pending,
            running,
            completed,
            failed,
            total,
        })
    }

    /// List pending tasks
    pub async fn list_pending(&self) -> Result<Vec<QueuedTask>, QueueError> {
        let conn = self.conn()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, goal, status, created_at, started_at, completed_at, result, priority
             FROM tasks 
             WHERE status IN ('pending', 'running')
             ORDER BY priority DESC, created_at ASC"
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        let tasks = stmt.query_map([], |row| {
            Ok(QueuedTask {
                id: row.get(0)?,
                goal: row.get(1)?,
                status: Self::parse_status(&row.get::<_, String>(2)?),
                created_at: row.get(3)?,
                started_at: row.get(4)?,
                completed_at: row.get(5)?,
                result: row.get(6)?,
                priority: row.get(7)?,
            })
        }).map_err(|e| QueueError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// List recent completed/failed tasks
    pub async fn list_recent(&self, limit: usize) -> Result<Vec<QueuedTask>, QueueError> {
        let conn = self.conn()?;
        
        let mut stmt = conn.prepare(
            "SELECT id, goal, status, created_at, started_at, completed_at, result, priority
             FROM tasks 
             WHERE status IN ('completed', 'failed', 'cancelled')
             ORDER BY completed_at DESC
             LIMIT ?1"
        ).map_err(|e| QueueError::Database(e.to_string()))?;

        let tasks = stmt.query_map(params![limit as i64], |row| {
            Ok(QueuedTask {
                id: row.get(0)?,
                goal: row.get(1)?,
                status: Self::parse_status(&row.get::<_, String>(2)?),
                created_at: row.get(3)?,
                started_at: row.get(4)?,
                completed_at: row.get(5)?,
                result: row.get(6)?,
                priority: row.get(7)?,
            })
        }).map_err(|e| QueueError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Get a specific task
    pub async fn get_task(&self, task_id: &str) -> Result<Option<QueuedTask>, QueueError> {
        let conn = self.conn()?;
        
        let task: Option<QueuedTask> = conn.query_row(
            "SELECT id, goal, status, created_at, started_at, completed_at, result, priority
             FROM tasks 
             WHERE id = ?1",
            params![task_id],
            |row| {
                Ok(QueuedTask {
                    id: row.get(0)?,
                    goal: row.get(1)?,
                    status: Self::parse_status(&row.get::<_, String>(2)?),
                    created_at: row.get(3)?,
                    started_at: row.get(4)?,
                    completed_at: row.get(5)?,
                    result: row.get(6)?,
                    priority: row.get(7)?,
                })
            },
        ).optional().map_err(|e| QueueError::Database(e.to_string()))?;

        Ok(task)
    }

    /// Clear old completed tasks (keep last N)
    pub async fn cleanup(&self, keep: usize) -> Result<usize, QueueError> {
        let conn = self.conn()?;
        
        // Get IDs of tasks to keep
        let keep_ids: Vec<String> = conn.prepare(
            "SELECT id FROM tasks WHERE status IN ('completed', 'failed', 'cancelled')
             ORDER BY completed_at DESC LIMIT ?1"
        ).map_err(|e| QueueError::Database(e.to_string()))?
            .query_map(params![keep as i64], |row| row.get::<_, String>(0))
            .map_err(|e| QueueError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if keep_ids.is_empty() {
            return Ok(0);
        }

        // Delete tasks not in keep list
        let placeholders: Vec<String> = keep_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM tasks WHERE status IN ('completed', 'failed', 'cancelled')
             AND id NOT IN ({})",
            placeholders.join(",")
        );

        let params: Vec<&dyn rusqlite::ToSql> = keep_ids.iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        let affected = conn.execute(&sql, &*params)
            .map_err(|e| QueueError::Database(e.to_string()))?;

        info!("Cleaned up {} old tasks", affected);
        Ok(affected as usize)
    }

    /// Parse status string
    fn parse_status(s: &str) -> TaskStatus {
        match s {
            "pending" => TaskStatus::Pending,
            "running" => TaskStatus::Running,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "cancelled" => TaskStatus::Cancelled,
            _ => TaskStatus::Pending,
        }
    }
}

/// Queue-related errors
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("IO error: {0}")]
    Io(String),
}
