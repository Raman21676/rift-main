//! Rift Core - AI coding assistant engine
//!
//! A clean-room implementation with:
//! - Plugin-based tool system
//! - Capability-based permissions
//! - Task DAG execution
//! - Streaming LLM support

pub mod agent;
pub mod capability;
pub mod llm;
pub mod plugin;
pub mod task;

pub use agent::{Agent, AgentError, ToolDefinition, ToolInvocation};
pub use capability::{Capability, CapabilityManager, CapabilityError};
pub use llm::{FunctionTool, LlmClient, LlmConfig, Message, Role, StreamingResponse, ChatResponse, ToolCall};
pub use plugin::{Plugin, PluginRegistry, Tool, ToolOutput, ToolError, ToolManifest};
pub use task::{Job, Task, TaskId, TaskOrchestrator, TaskResult, TaskStatus, TaskError, TaskExecutor};

use std::sync::Arc;

/// Main engine for Rift
pub struct RiftEngine {
    plugin_registry: PluginRegistry,
    orchestrator: TaskOrchestrator,
    capability_manager: CapabilityManager,
    llm_client: LlmClient,
}

impl RiftEngine {
    /// Create a new engine
    pub fn new(config: RiftConfig) -> Self {
        Self {
            plugin_registry: PluginRegistry::new(),
            orchestrator: TaskOrchestrator::new(),
            capability_manager: CapabilityManager::with_capabilities(config.capabilities.clone()),
            llm_client: LlmClient::new(config.llm.clone()),
        }
    }
    
    /// Get the plugin registry (immutable)
    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugin_registry
    }
    
    /// Get mutable plugin registry
    pub fn plugins_mut(&mut self) -> &mut PluginRegistry {
        &mut self.plugin_registry
    }
    
    /// Get the capability manager
    pub fn capabilities(&self) -> &CapabilityManager {
        &self.capability_manager
    }
    
    /// Get mutable capability manager
    pub fn capabilities_mut(&mut self) -> &mut CapabilityManager {
        &mut self.capability_manager
    }
    
    /// Get the LLM client
    pub fn llm(&self) -> &LlmClient {
        &self.llm_client
    }
    
    /// Create an agent with current tools
    pub fn agent(&self) -> Agent {
        Agent::new(
            self.llm_client.clone(),
            Arc::new(self.plugin_registry.clone()),
            Arc::new(self.capability_manager.clone()),
        )
    }
    
    /// Execute a job
    pub async fn execute_job(&self, job: &mut Job) -> Result<task::JobResult, TaskError> {
        self.orchestrator.run(job, self).await
    }
}

impl TaskExecutor for RiftEngine {
    fn execute(&self, task: &Task) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TaskResult, TaskError>> + Send>> {
        let tool = match self.plugin_registry.get_tool(&task.tool_name) {
            Some(t) => t,
            None => {
                let name = task.tool_name.clone();
                return Box::pin(async move {
                    Err(TaskError::Tool(format!("Tool '{}' not found", name)))
                });
            }
        };
        
        // Check capabilities
        if let Err(e) = self.capability_manager.verify(&tool.required_capabilities()) {
            return Box::pin(async move { Err(TaskError::Tool(e.to_string())) });
        }
        
        let input = task.input.clone();
        Box::pin(async move {
            let start = std::time::Instant::now();
            match tool.execute(input).await {
                Ok(output) => Ok(TaskResult {
                    success: output.success,
                    output: output.content,
                    data: output.data,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                }),
                Err(e) => Err(TaskError::Tool(e.to_string())),
            }
        })
    }
}

/// Configuration for Rift
#[derive(Debug, Clone)]
pub struct RiftConfig {
    /// LLM configuration
    pub llm: LlmConfig,
    /// Granted capabilities
    pub capabilities: Vec<Capability>,
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: usize,
}

impl RiftConfig {
    /// Create default configuration
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            llm: LlmConfig::new(api_key),
            capabilities: vec![
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ShellExecute,
                Capability::NetworkAccess,
            ],
            max_concurrent_tasks: 4,
        }
    }
    
    /// Set LLM model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.llm.model = model.into();
        self
    }
    
    /// Set API base URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.llm.base_url = url.into();
        self
    }
    
    /// Set capabilities
    pub fn with_capabilities(mut self, caps: Vec<Capability>) -> Self {
        self.capabilities = caps;
        self
    }
}

impl Default for RiftConfig {
    fn default() -> Self {
        Self::new("")
    }
}
