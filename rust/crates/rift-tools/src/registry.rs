//! Tool registry for managing available tools

use crate::{Tool, ToolManifest};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }
    
    /// Create with built-in tools
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(crate::builtin::BashTool::new()));
        registry.register(Arc::new(crate::builtin::ReadFileTool::new()));
        registry.register(Arc::new(crate::builtin::WriteFileTool::new()));
        registry.register(Arc::new(crate::builtin::GlobTool::new()));
        registry.register(Arc::new(crate::builtin::GrepTool::new()));
        registry.register(Arc::new(crate::builtin::EditFileTool::new()));
        registry.register(Arc::new(crate::builtin::InsertAtLineTool::new()));
        registry.register(Arc::new(crate::builtin::GitStatusTool::new()));
        registry.register(Arc::new(crate::builtin::GitDiffTool::new()));
        registry.register(Arc::new(crate::builtin::GitCommitTool::new()));
        registry.register(Arc::new(crate::builtin::GitPushTool::new()));
        registry.register(Arc::new(crate::builtin::GitBranchTool::new()));
        registry.register(Arc::new(crate::builtin::DeployTool::new()));
        registry.register(Arc::new(crate::builtin::WebFetchTool::new()));
        registry.register(Arc::new(crate::builtin::WebSearchTool::new()));
        registry
    }
    
    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }
    
    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    
    /// List all available tools
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
    
    /// Get manifests for all tools (for API schema)
    pub fn manifests(&self) -> Vec<ToolManifest> {
        self.tools
            .values()
            .map(|t| ToolManifest {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }
    
    /// Check if a tool exists
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
