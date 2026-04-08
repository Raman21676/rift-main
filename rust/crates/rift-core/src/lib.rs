//! Rift Core - AI coding assistant engine
//!
//! A clean-room implementation with:
//! - Plugin-based tool system
//! - Capability-based permissions
//! - Task DAG execution
//! - Streaming LLM support

pub mod agent;
pub mod capability;
pub mod config;
pub mod context;
pub mod daemon;
pub mod llm;
pub mod planner;
pub mod plugin;
pub mod self_correct;
pub mod server;
pub mod session;
pub mod task;
pub mod verify;

pub use agent::{Agent, AgentError, ToolDefinition, ToolInvocation};
pub use capability::{Capability, CapabilityManager, CapabilityError};
pub use config::{ConfigFile, create_sample_config, ensure_config_dir};
pub use context::{ContextGatherer, ProjectContext, ProjectType, GitInfo, ContextError};
pub use llm::{FunctionTool, LlmClient, LlmConfig, Message, Role, StreamingResponse, ChatResponse, ToolCall};
pub use planner::{Planner, PlannerError};
pub use plugin::{Plugin, PluginRegistry, Tool, ToolOutput, ToolError, ToolManifest};
pub use self_correct::{SelfCorrector, JobContext, FailureAnalysis, CorrectionStrategy, CorrectionResult, CorrectionError, CorrectiveTask};
pub use self_correct::orchestrator::SelfCorrectingOrchestrator;
pub use session::{SessionStore, SessionError};
pub use task::{Job, Task, TaskId, TaskOrchestrator, TaskResult, TaskStatus, TaskError, TaskExecutor};
pub use verify::{Verifier, VerificationResult, CheckResult, VerificationType};
pub use daemon::{Daemon, DaemonState, DaemonError, DaemonCommand, DaemonResponse, TaskQueue, QueuedTask, QueueStatus, DaemonClient};
pub use server::{RemoteServer, AuthManager, generate_token, ConnectionInfo};

use std::sync::Arc;

/// Main engine for Rift
pub struct RiftEngine {
    plugin_registry: PluginRegistry,
    orchestrator: TaskOrchestrator,
    capability_manager: CapabilityManager,
    llm_client: LlmClient,
    max_iterations: usize,
}

impl RiftEngine {
    /// Create a new engine
    pub fn new(config: RiftConfig) -> Self {
        Self {
            plugin_registry: PluginRegistry::new(),
            orchestrator: TaskOrchestrator::new(),
            capability_manager: CapabilityManager::with_capabilities(config.capabilities.clone()),
            llm_client: LlmClient::new(config.llm.clone()),
            max_iterations: config.max_iterations,
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
        .with_max_iterations(self.max_iterations)
    }
    
    /// Execute a job
    pub async fn execute_job(&self, job: &mut Job) -> Result<task::JobResult, TaskError> {
        self.orchestrator.run(job, self).await
    }
    
    /// Execute a job with self-correction enabled
    /// 
    /// When tasks fail, the system will:
    /// 1. Analyze the failure using the LLM
    /// 2. Attempt to fix the issue (retry, modify, or add prerequisite tasks)
    /// 3. Continue execution with corrections
    /// 
    /// This makes the agent more resilient to transient failures and recoverable errors.
    pub async fn execute_job_with_self_correction(&self, job: &mut Job) -> Result<task::JobResult, TaskError> {
        use self_correct::orchestrator::SelfCorrectingOrchestrator;
        
        // Get list of available tools
        let tools: Vec<String> = self.plugin_registry.list_tools()
            .iter()
            .map(|&s| s.to_string())
            .collect();
        
        let mut orchestrator = SelfCorrectingOrchestrator::new()
            .with_self_correction(self.llm_client.clone())
            .with_max_concurrent(4)
            .with_tools(tools);
            
        orchestrator.run(job, self).await
    }
    
    /// Execute a job with automatic verification
    ///
    /// After the job completes, verification checks are run to ensure:
    /// - Files that were written actually exist
    /// - Syntax of written files is valid
    /// - Build commands succeeded
    /// - Tests pass (if test commands were run)
    ///
    /// Returns both the job result and verification results.
    pub async fn execute_job_with_verification(&self, job: &mut Job) -> Result<(task::JobResult, VerificationResult), TaskError> {
        // Execute the job first
        let result = self.execute_job(job).await?;
        
        // Run verification
        let verifier = Verifier::new();
        let verification = verifier.verify_job(job).await;
        
        Ok((result, verification))
    }
    
    /// Execute a job with both self-correction AND verification
    ///
    /// This is the "full autonomous mode" that:
    /// 1. Gathers context before planning
    /// 2. Self-corrects any failures during execution
    /// 3. Verifies all outputs after completion
    pub async fn execute_job_autonomous(&self, job: &mut Job) -> Result<(task::JobResult, VerificationResult), TaskError> {
        // Get list of available tools
        let tools: Vec<String> = self.plugin_registry.list_tools()
            .iter()
            .map(|&s| s.to_string())
            .collect();
        
        // Run with self-correction
        let mut orchestrator = self_correct::orchestrator::SelfCorrectingOrchestrator::new()
            .with_self_correction(self.llm_client.clone())
            .with_max_concurrent(4)
            .with_tools(tools);
            
        let result = orchestrator.run(job, self).await?;
        
        // Run verification
        let verifier = Verifier::new();
        let verification = verifier.verify_job(job).await;
        
        Ok((result, verification))
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
    /// Maximum agent iterations
    pub max_iterations: usize,
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
            max_iterations: 10,
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

    /// Load configuration from file and environment
    pub fn load() -> Self {
        let file_config = ConfigFile::load();

        // API key priority: env var > config file
        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| file_config.api.key.clone())
            .unwrap_or_default();

        let mut llm = LlmConfig::new(api_key);
        llm.model = file_config.api.model.clone();
        llm.base_url = file_config.api.base_url.clone();

        // Allow env overrides for model and base_url
        if let Ok(model) = std::env::var("RIFT_MODEL") {
            llm.model = model;
        }
        if let Ok(base_url) = std::env::var("RIFT_BASE_URL") {
            llm.base_url = base_url;
        }

        let capabilities = file_config.parse_capabilities();

        Self {
            llm,
            capabilities,
            max_concurrent_tasks: file_config.runtime.max_concurrent_tasks,
            max_iterations: file_config.runtime.max_iterations,
        }
    }
}

impl Default for RiftConfig {
    fn default() -> Self {
        Self::load()
    }
}
