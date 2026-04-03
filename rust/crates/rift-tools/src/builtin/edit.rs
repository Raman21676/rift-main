//! Edit file tool - find and replace operations

use async_trait::async_trait;
use rift_core::capability::Capability;
use rift_core::plugin::{Tool, ToolError, ToolOutput};
use serde_json::Value;
use std::path::Path;

/// Edit a file by finding and replacing text
#[derive(Debug)]
pub struct EditFileTool;

impl EditFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }
    
    fn description(&self) -> &str {
        "Find and replace text in a file. Supports multi-line replacements."
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "Text to find and replace (exact match)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text"
                }
            },
            "required": ["path", "old_string", "new_string"]
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
        
        let old_string = input
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'old_string' parameter".to_string()))?;
        
        let new_string = input
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'new_string' parameter".to_string()))?;
        
        let path = Path::new(path);
        
        if !path.exists() {
            return Ok(ToolOutput::error(format!("File not found: {}", path.display())));
        }
        
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        // Find and replace
        if !content.contains(old_string) {
            return Ok(ToolOutput::error(format!(
                "Could not find the text to replace in {}",
                path.display()
            )));
        }
        
        let new_content = content.replacen(old_string, new_string, 1);
        
        // Write back
        tokio::fs::write(path, new_content)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        // Count lines changed
        let old_lines = old_string.lines().count();
        let new_lines = new_string.lines().count();
        
        Ok(ToolOutput::success(format!(
            "Successfully edited {} (replaced {} line(s) with {} line(s))",
            path.display(),
            old_lines,
            new_lines
        )))
    }
}

/// Insert text at a specific line
#[derive(Debug)]
pub struct InsertAtLineTool;

impl InsertAtLineTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for InsertAtLineTool {
    fn name(&self) -> &str {
        "insert_at_line"
    }
    
    fn description(&self) -> &str {
        "Insert text at a specific line number"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number to insert at (1-indexed)"
                },
                "content": {
                    "type": "string",
                    "description": "Text to insert"
                }
            },
            "required": ["path", "line", "content"]
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
        
        let line_num = input
            .get("line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'line' parameter".to_string()))? as usize;
        
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'content' parameter".to_string()))?;
        
        let path = Path::new(path);
        let file_content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        let lines: Vec<&str> = file_content.lines().collect();
        let insert_pos = line_num.saturating_sub(1).min(lines.len());
        
        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        new_lines.insert(insert_pos, content.to_string());
        
        let new_content = new_lines.join("\n");
        tokio::fs::write(path, new_content)
            .await
            .map_err(|e| ToolError::Io(e))?;
        
        Ok(ToolOutput::success(format!(
            "Inserted {} line(s) at line {} in {}",
            content.lines().count(),
            line_num,
            path.display()
        )))
    }
}
