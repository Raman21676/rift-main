//! Built-in tools for Rift

pub mod edit;
pub mod web;

use async_trait::async_trait;
use rift_core::capability::Capability;
use rift_core::plugin::{Tool, ToolError, ToolOutput};
use serde_json::Value;
use std::path::Path;

pub use edit::{EditFileTool, InsertAtLineTool};
pub use web::{WebFetchTool, WebSearchTool};

/// Bash shell tool
#[derive(Debug)]
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    
    fn description(&self) -> &str {
        "Execute bash shell commands"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60)",
                    "default": 60
                }
            },
            "required": ["command"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ShellExecute]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'command' parameter".to_string()))?;
        
        let timeout = input
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(60) as u64;
        
        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await
        .map_err(|_| ToolError::ExecutionFailed("Command timed out".to_string()))?
        .map_err(|e| ToolError::Io(e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        if output.status.success() {
            Ok(ToolOutput::success(stdout.to_string()))
        } else {
            let error = if stderr.is_empty() {
                format!("Command failed with exit code {:?}", output.status.code())
            } else {
                stderr.to_string()
            };
            Ok(ToolOutput::error(error))
        }
    }
}

/// File read tool
#[derive(Debug)]
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }
    
    fn description(&self) -> &str {
        "Read the contents of a file"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: all)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed)"
                }
            },
            "required": ["path"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'path' parameter".to_string()))?;
        
        let path = Path::new(path);
        
        if !path.exists() {
            return Ok(ToolOutput::error(format!("File not found: {}", path.display())));
        }
        
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|o| o.saturating_sub(1) as usize) // Convert to 0-indexed
            .unwrap_or(0);
        
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|l| l as usize);
        
        let lines: Vec<&str> = content.lines().collect();
        let start = offset.min(lines.len());
        let end = limit
            .map(|l| (start + l).min(lines.len()))
            .unwrap_or(lines.len());
        
        let selected: Vec<&str> = lines[start..end].to_vec();
        let result = selected.join("\n");
        
        Ok(ToolOutput::success(result).with_data(serde_json::json!({
            "total_lines": lines.len(),
            "lines_read": selected.len(),
            "start_line": start + 1,
        })))
    }
}

/// File write tool
#[derive(Debug)]
pub struct WriteFileTool;

impl WriteFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    
    fn description(&self) -> &str {
        "Write content to a file"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "append": {
                    "type": "boolean",
                    "description": "Append to file instead of overwriting",
                    "default": false
                }
            },
            "required": ["path", "content"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'path' parameter".to_string()))?;
        
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'content' parameter".to_string()))?;
        
        let append = input
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let path = Path::new(path);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::Io(e))?;
        }
        
        if append {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .map_err(|e| ToolError::Io(e))?
        } else {
            tokio::fs::File::create(path)
                .await
                .map_err(|e| ToolError::Io(e))?
        };
        
        tokio::fs::write(path, content)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        Ok(ToolOutput::success(format!("File written: {}", path.display())))
    }
}

/// Glob search tool
#[derive(Debug)]
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    
    fn description(&self) -> &str {
        "Find files matching a glob pattern"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files (e.g., '**/*.rs')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory to search from (default: current directory)",
                    "default": "."
                }
            },
            "required": ["pattern"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'pattern' parameter".to_string()))?;
        
        let base_path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        
        let pattern_with_base = if base_path == "." {
            pattern.to_string()
        } else {
            format!("{}/{}", base_path.trim_end_matches('/'), pattern)
        };
        
        let paths: Vec<String> = glob::glob(&pattern_with_base)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?
            .filter_map(Result::ok)
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        
        Ok(ToolOutput::success(format!("Found {} files", paths.len()))
            .with_data(serde_json::json!({"files": paths})))
    }
}

/// Grep tool for searching file contents
#[derive(Debug)]
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    
    fn description(&self) -> &str {
        "Search for a pattern in file contents using ripgrep"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: current directory)",
                    "default": "."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., '*.rs')"
                }
            },
            "required": ["pattern"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'pattern' parameter".to_string()))?;
        
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        
        let glob = input
            .get("glob")
            .and_then(|v| v.as_str());
        
        let mut cmd = tokio::process::Command::new("rg");
        cmd.arg("--line-number")
            .arg("--color=never")
            .arg(pattern)
            .arg(path);
        
        if let Some(g) = glob {
            cmd.arg("--glob").arg(g);
        }
        
        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        
        let matches: Vec<serde_json::Value> = lines
            .iter()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    Some(serde_json::json!({
                        "file": parts[0],
                        "line": parts[1].parse::<usize>().ok(),
                        "content": parts.get(2).unwrap_or(&"")
                    }))
                } else {
                    None
                }
            })
            .collect();
        
        Ok(ToolOutput::success(format!("Found {} matches", matches.len()))
            .with_data(serde_json::json!({"matches": matches})))
    }
}
