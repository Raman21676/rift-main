//! LLM client with streaming support

use crate::agent::ToolDefinition;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Configuration for LLM
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// API key
    pub api_key: String,
    /// Model name
    pub model: String,
    /// API base URL
    pub base_url: String,
    /// Default temperature
    pub temperature: f32,
    /// Default max tokens
    pub max_tokens: Option<u32>,
}

impl LlmConfig {
    /// Create new config
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-4o-mini".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            temperature: 0.7,
            max_tokens: None,
        }
    }
    
    /// Set model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
    
    /// Set base URL
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
    
    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
    
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// LLM client
#[derive(Debug, Clone)]
pub struct LlmClient {
    config: LlmConfig,
    client: reqwest::Client,
}

impl LlmClient {
    /// Create a new client
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }
    
    /// Send a chat request (non-streaming)
    pub async fn chat(&self, messages: Vec<Message>) -> Result<String, LlmError> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            stream: Some(false),
            temperature: Some(self.config.temperature),
            max_tokens: self.config.max_tokens,
            tools: None,
            tool_choice: None,
        };
        
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;
        
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(text));
        }
        
        let completion: ChatCompletion = response
            .json()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;
        
        completion
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::EmptyResponse)
    }
    
    /// Send a chat request with tool support
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<FunctionTool>,
    ) -> Result<ChatResponse, LlmError> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            stream: Some(false),
            temperature: Some(self.config.temperature),
            max_tokens: self.config.max_tokens,
            tools: Some(tools),
            tool_choice: Some("auto".to_string()),
        };
        
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;
        
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(text));
        }
        
        let completion: ChatCompletionWithTools = response
            .json()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;
        
        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or(LlmError::EmptyResponse)?;
        
        Ok(ChatResponse {
            content: choice.message.content.clone().unwrap_or_default(),
            tool_calls: choice.message.tool_calls,
        })
    }
    
    /// Send a streaming chat request
    pub async fn chat_stream(
        &self,
        messages: Vec<Message>,
    ) -> Result<StreamingResponse, LlmError> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            stream: Some(true),
            temperature: Some(self.config.temperature),
            max_tokens: self.config.max_tokens,
            tools: None,
            tool_choice: None,
        };
        
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::Request(e.to_string()))?;
        
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(text));
        }
        
        let stream = response.bytes_stream();
        
        let mapped_stream = stream.filter_map(|result| async move {
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if data == "[DONE]" {
                                return None;
                            }
                            if let Ok(completion) = serde_json::from_str::<ChatCompletion>(data) {
                                if let Some(choice) = completion.choices.into_iter().next() {
                                    if let Some(delta) = choice.delta {
                                        let content = delta.content;
                                        if !content.is_empty() {
                                            return Some(Ok(content));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None
                }
                Err(e) => Some(Err(LlmError::Stream(e.to_string()))),
            }
        });
        
        Ok(StreamingResponse {
            inner: Box::pin(mapped_stream),
        })
    }
}

/// Streaming response wrapper
pub struct StreamingResponse {
    inner: Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>,
}

impl Stream for StreamingResponse {
    type Item = Result<String, LlmError>;
    
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

/// Tool definition for OpenAI function calling
#[derive(Debug, Clone, Serialize)]
pub struct FunctionTool {
    pub r#type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl From<ToolDefinition> for FunctionTool {
    fn from(tool: ToolDefinition) -> Self {
        Self {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            },
        }
    }
}

/// Chat request body
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<FunctionTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

/// Tool call from LLM
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string that needs parsing
}

/// Chat completion response
#[derive(Debug, Deserialize)]
struct ChatCompletion {
    choices: Vec<Choice>,
}

/// Choice in completion
#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
    #[serde(default)]
    delta: Option<Message>,
}

/// Extended message with tool calls
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    pub role: Role,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Chat response with optional tool calls
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Chat completion with tools
#[derive(Debug, Deserialize)]
struct ChatCompletionWithTools {
    choices: Vec<ChoiceWithTools>,
}

#[derive(Debug, Deserialize)]
struct ChoiceWithTools {
    message: AssistantMessage,
}

/// LLM errors
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Request failed: {0}")]
    Request(String),
    
    #[error("API error: {0}")]
    Api(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Stream error: {0}")]
    Stream(String),
    
    #[error("Empty response")]
    EmptyResponse,
}
