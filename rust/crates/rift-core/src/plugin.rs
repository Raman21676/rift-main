//! Plugin system for extensible tools
//!
//! Tools are implemented as plugins that declare their capabilities
//! and can be dynamically registered.

use crate::capability::Capability;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A plugin provides tools and capabilities
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name
    fn name(&self) -> &str;
    
    /// Plugin version
    fn version(&self) -> &str;
    
    /// Get tools provided by this plugin
    fn tools(&self) -> Vec<Arc<dyn Tool>>;
    
    /// Initialize the plugin
    async fn init(&self) -> Result<(), PluginError> {
        Ok(())
    }
    
    /// Shutdown the plugin
    async fn shutdown(&self) -> Result<(), PluginError> {
        Ok(())
    }
}

/// A tool that can be executed
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name
    fn name(&self) -> &str;
    
    /// Tool description
    fn description(&self) -> &str;
    
    /// JSON schema for tool parameters
    fn parameters(&self) -> Value;
    
    /// Capabilities required by this tool
    fn required_capabilities(&self) -> Vec<Capability>;
    
    /// Execute the tool
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError>;
}

/// Output from a tool execution
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Success status
    pub success: bool,
    /// Output content
    pub content: String,
    /// Structured data if available
    pub data: Option<Value>,
}

impl ToolOutput {
    /// Create a successful output
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            data: None,
        }
    }
    
    /// Create an error output
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            success: false,
            content: content.into(),
            data: None,
        }
    }
    
    /// Add structured data
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Errors that can occur during tool execution
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Capability denied: {0}")]
    CapabilityDenied(String),
    
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Errors that can occur during plugin operations
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    
    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),
    
    #[error("Init failed: {0}")]
    InitFailed(String),
    
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
}

/// Registry of plugins and tools
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn Plugin>>,
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl PluginRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            tools: HashMap::new(),
        }
    }
    
    /// Register a plugin
    pub async fn register_plugin(&mut self, plugin: Arc<dyn Plugin>) -> Result<(), PluginError> {
        let name = plugin.name().to_string();
        
        if self.plugins.contains_key(&name) {
            return Err(PluginError::AlreadyLoaded(name));
        }
        
        plugin.init().await?;
        
        for tool in plugin.tools() {
            self.tools.insert(tool.name().to_string(), tool);
        }
        
        self.plugins.insert(name, plugin);
        Ok(())
    }
    
    /// Register a tool directly
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    
    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    
    /// List all available tools
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
    
    /// Get all tools
    pub fn tools(&self) -> &HashMap<String, Arc<dyn Tool>> {
        &self.tools
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        let mut new = Self::new();
        for (_, tool) in &self.tools {
            new.register_tool(tool.clone());
        }
        new
    }
}

/// Manifest for tool serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
