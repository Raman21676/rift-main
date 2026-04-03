//! Session persistence using SQLite

use crate::llm::{Message, Role};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

/// Persistent session store
pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    /// Open or create the session database
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let conn = Connection::open(path)
            .map_err(|e| SessionError::Database(e.to_string()))?;
        
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }
    
    /// Open the default database at ~/.config/rift/sessions.db
    pub fn default() -> Result<Self, SessionError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SessionError::Database("Could not find config dir".to_string()))?
            .join("rift");
        
        std::fs::create_dir_all(&dir)
            .map_err(|e| SessionError::Database(e.to_string()))?;
        
        Self::open(dir.join("sessions.db"))
    }
    
    fn init(&self) -> Result<(), SessionError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )",
            [],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id)",
            [],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Create or get a session by name
    pub fn get_or_create(&self, name: &str) -> Result<String, SessionError> {
        let existing: Option<String> = self.conn.query_row(
            "SELECT id FROM sessions WHERE name = ?1",
            params![name],
            |row| row.get(0),
        ).optional()
        .map_err(|e| SessionError::Database(e.to_string()))?;
        
        if let Some(id) = existing {
            return Ok(id);
        }
        
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        self.conn.execute(
            "INSERT INTO sessions (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![&id, name, now],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        Ok(id)
    }
    
    /// Save a message to a session
    pub fn save_message(&self, session_id: &str, message: &Message) -> Result<(), SessionError> {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        self.conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, &message.content, now],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Load all messages for a session
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY created_at ASC, id ASC"
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        let rows = stmt.query_map(
            params![session_id],
            |row| {
                let role_str: String = row.get(0)?;
                let content: String = row.get(1)?;
                let role = match role_str.as_str() {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Ok(Message { role, content })
            }
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| SessionError::Database(e.to_string()))?);
        }
        
        Ok(messages)
    }
    
    /// Clear all messages for a session (but keep the session itself)
    pub fn clear_messages(&self, session_id: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        Ok(())
    }
    
    /// Delete a session and all its messages
    pub fn delete_session(&self, name: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "DELETE FROM sessions WHERE name = ?1",
            params![name],
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        Ok(())
    }
    
    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<(String, String, i64)>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at FROM sessions ORDER BY created_at DESC"
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        let rows = stmt.query_map(
            [],
            |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let created_at: i64 = row.get(2)?;
                Ok((id, name, created_at))
            }
        ).map_err(|e| SessionError::Database(e.to_string()))?;
        
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| SessionError::Database(e.to_string()))?);
        }
        
        Ok(sessions)
    }
}

/// Session-related errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Database error: {0}")]
    Database(String),
}
