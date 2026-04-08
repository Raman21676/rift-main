//! Agent system - tool calling and execution loop

use crate::capability::CapabilityManager;
use crate::llm::{FunctionTool, LlmClient, Message, Role, ToolCall};
use crate::planner::Planner;
use crate::plugin::{PluginRegistry, Tool, ToolOutput, ToolError};
use crate::task::Job;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// Tool definition for LLM
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    /// Create from a tool
    pub fn from_tool(tool: &Arc<dyn Tool>) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters(),
        }
    }
}

/// A tool invocation request (parsed from LLM response)
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInvocation {
    pub name: String,
    pub arguments: Value,
}

impl ToolInvocation {
    /// Parse from a ToolCall (from OpenAI function calling)
    pub fn from_tool_call(call: &ToolCall) -> Result<Self, AgentError> {
        let args: Value = serde_json::from_str(&call.function.arguments)
            .map_err(|e| AgentError::Parse(format!("Failed to parse tool arguments: {}", e)))?;
        
        Ok(Self {
            name: call.function.name.clone(),
            arguments: args,
        })
    }
}

/// Agent that can execute tools
pub struct Agent {
    llm_client: LlmClient,
    plugin_registry: Arc<PluginRegistry>,
    capability_manager: Arc<CapabilityManager>,
    max_iterations: usize,
}

impl Agent {
    /// Create new agent
    pub fn new(
        llm_client: LlmClient,
        plugin_registry: Arc<PluginRegistry>,
        capability_manager: Arc<CapabilityManager>,
    ) -> Self {
        Self {
            llm_client,
            plugin_registry,
            capability_manager,
            max_iterations: 10,
        }
    }
    
    /// Set max iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
    
    /// Get available tools as definitions
    fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.plugin_registry
            .tools()
            .values()
            .map(|t| ToolDefinition::from_tool(t))
            .collect()
    }
    
    /// Convert tool definitions to FunctionTools for OpenAI API
    fn get_function_tools(&self) -> Vec<FunctionTool> {
        self.get_tool_definitions()
            .into_iter()
            .map(FunctionTool::from)
            .collect()
    }
    
    /// Build system prompt for native function calling
    fn build_system_prompt_with_tools(&self) -> String {
        String::from("You are a helpful AI coding assistant. Use the available tools when needed.")
    }
    
    /// Build system prompt for text-based tool parsing
    fn build_system_prompt_with_text_tools(&self) -> String {
        let tools = self.get_tool_definitions();
        
        let mut prompt = String::from(
            "You are a helpful AI coding assistant with access to tools.\n\n"
        );
        
        if !tools.is_empty() {
            prompt.push_str("AVAILABLE TOOLS:\n");
            prompt.push_str("================\n\n");
            
            for tool in &tools {
                prompt.push_str(&format!(
                    "TOOL: {}\n  Description: {}\n  Parameters: {}\n\n",
                    tool.name,
                    tool.description,
                    serde_json::to_string_pretty(&tool.parameters).unwrap_or_default()
                ));
            }
            
            prompt.push_str("\nIMPORTANT INSTRUCTIONS:\n");
            prompt.push_str("1. When you need to use a tool, respond ONLY with the tool call format:\n");
            prompt.push_str("   ```tool\n");
            prompt.push_str(r#"   {"name": "tool_name", "arguments": {"param": "value"}}"#);
            prompt.push_str("\n   ```\n\n");
            prompt.push_str("2. Do not add any other text when making a tool call.\n");
            prompt.push_str("3. After the tool executes, you'll see the result and can then provide your response.\n");
        }
        
        prompt
    }
    
    /// Execute a conversation with potential tool calls
    pub async fn chat(&self, user_message: &str) -> Result<String, AgentError> {
        let system_prompt = self.build_system_prompt_with_tools();
        let tools = self.get_function_tools();
        
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(user_message),
        ];
        
        let mut iteration = 0;
        
        loop {
            if iteration >= self.max_iterations {
                return Err(AgentError::MaxIterationsReached);
            }
            iteration += 1;
            
            // Try native function calling first
            let response = match self.llm_client.chat_with_tools(messages.clone(), tools.clone()).await {
                Ok(resp) => resp,
                Err(e) => {
                    // If function calling fails (model doesn't support it), fall back to regular chat
                    let error_msg = e.to_string();
                    if error_msg.contains("tool use") || error_msg.contains("function") {
                        return self.chat_with_text_parsing(user_message).await;
                    }
                    return Err(AgentError::Llm(error_msg));
                }
            };
            
            // Check if there are tool calls
            if let Some(tool_calls) = response.tool_calls {
                // Add assistant message with tool calls
                messages.push(Message {
                    role: Role::Assistant,
                    content: response.content.clone(),
                });
                
                // Execute each tool call
                for tool_call in tool_calls {
                    let result = self.execute_tool_call(&tool_call).await;
                    
                    // Add tool result as a message
                    let result_text = match result {
                        Ok(output) => {
                            if output.success {
                                format!("Tool '{}' result: {}", tool_call.function.name, output.content)
                            } else {
                                format!("Tool '{}' error: {}", tool_call.function.name, output.content)
                            }
                        }
                        Err(e) => format!("Tool '{}' failed: {}", tool_call.function.name, e),
                    };
                    
                    messages.push(Message {
                        role: Role::User,
                        content: result_text,
                    });
                }
                
                // Continue loop to let LLM process tool results
                continue;
            }
            
            // No tool calls - return the final response
            return Ok(response.content);
        }
    }
    
    /// Fallback chat using text-based tool parsing (for models without function calling)
    async fn chat_with_text_parsing(&self, user_message: &str) -> Result<String, AgentError> {
        let system_prompt = self.build_system_prompt_with_text_tools();
        
        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(user_message),
        ];
        
        let mut iteration = 0;
        
        loop {
            if iteration >= self.max_iterations {
                return Err(AgentError::MaxIterationsReached);
            }
            iteration += 1;
            
            // Use regular chat
            let response = self.llm_client.chat(messages.clone()).await
                .map_err(|e| AgentError::Llm(e.to_string()))?;
            
            // Try to parse tool calls from text
            if let Some(tool_invocation) = self.parse_tool_from_text(&response) {
                // Execute the tool
                let result = self.execute_tool_direct(&tool_invocation.name, tool_invocation.arguments).await;
                
                // Add messages
                messages.push(Message::assistant(response));
                
                let result_text = match result {
                    Ok(output) => {
                        if output.success {
                            format!("Tool result: {}", output.content)
                        } else {
                            format!("Tool error: {}", output.content)
                        }
                    }
                    Err(e) => format!("Tool failed: {}", e),
                };
                
                messages.push(Message::user(result_text));
                continue;
            }
            
            return Ok(response);
        }
    }
    
    /// Parse tool invocation from text response
    fn parse_tool_from_text(&self, text: &str) -> Option<ToolInvocation> {
        // Look for pattern: ```tool\n{"name": "...", "arguments": {...}}\n```
        if let Some(start) = text.find("```tool") {
            let json_start = text[start..].find('\n').map(|i| start + i + 1)?;
            let json_end = text[json_start..].find("```").map(|i| json_start + i)?;
            let json_str = &text[json_start..json_end];
            
            serde_json::from_str(json_str).ok()
        } else {
            None
        }
    }
    
    /// Execute a single tool call
    async fn execute_tool_call(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let tool = self.plugin_registry
            .get_tool(&call.function.name)
            .ok_or_else(|| ToolError::InvalidInput(format!("Tool '{}' not found", call.function.name)))?;
        
        // Parse arguments
        let args: Value = serde_json::from_str(&call.function.arguments)
            .map_err(|e| ToolError::InvalidInput(format!("Invalid arguments: {}", e)))?;
        
        // Check capabilities
        self.capability_manager
            .verify(&tool.required_capabilities())
            .map_err(|e| ToolError::CapabilityDenied(e.to_string()))?;
        
        // Execute
        tool.execute(args).await
    }
    
    /// Execute a tool directly (bypassing LLM)
    pub async fn execute_tool_direct(&self, name: &str, args: Value) -> Result<ToolOutput, AgentError> {
        let tool = self.plugin_registry
            .get_tool(name)
            .ok_or_else(|| AgentError::ToolNotFound(name.to_string()))?;
        
        // Check capabilities
        self.capability_manager
            .verify(&tool.required_capabilities())
            .map_err(|e| AgentError::CapabilityDenied(e.to_string()))?;
        
        tool.execute(args)
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))
    }
    
    /// Plan a natural language goal into an executable Job
    /// 
    /// This method gathers project context first (files, config, git status)
    /// to generate more informed plans that work with the existing project structure.
    pub async fn plan_job(&self, goal: &str) -> Result<Job, AgentError> {
        let tools: Vec<String> = self.plugin_registry
            .tools()
            .keys()
            .cloned()
            .collect();
        
        // Gather project context before planning
        let context: Option<crate::context::ProjectContext> = crate::context::ContextGatherer::gather(".")
            .await
            .ok(); // Context gathering is best-effort
        
        let planner = Planner::new(self.llm_client.clone(), tools);
        
        if let Some(ref ctx) = context {
            planner.plan_with_context(goal, ctx).await
        } else {
            planner.plan(goal).await
        }
            .map_err(|e| AgentError::Planner(e.to_string()))
    }
    
    /// Plan with explicit context (for testing or custom workflows)
    pub async fn plan_job_with_context(&self, goal: &str, context: &crate::context::ProjectContext) -> Result<Job, AgentError> {
        let tools: Vec<String> = self.plugin_registry
            .tools()
            .keys()
            .cloned()
            .collect();
        
        let planner = Planner::new(self.llm_client.clone(), tools);
        planner.plan_with_context(goal, context).await
            .map_err(|e| AgentError::Planner(e.to_string()))
    }
}

/// Agent errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(String),
    
    #[error("Tool execution failed: {0}")]
    Tool(String),
    
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("Max iterations reached")]
    MaxIterationsReached,
    
    #[error("Capability denied: {0}")]
    CapabilityDenied(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Planner error: {0}")]
    Planner(String),
}
