//! Rift Tools - Built-in tools for the Rift assistant
//!
//! This crate provides the standard set of tools for code assistance.

use rift_core::plugin::{Tool, ToolError};
use serde_json::Value;

pub mod builtin;
pub mod registry;

pub use registry::ToolRegistry;
pub use rift_core::plugin::ToolManifest;

/// Helper trait for parameter extraction
pub trait ExtractParams: Sized {
    fn extract(value: &Value) -> Result<Self, ToolError>;
}

/// Macro to define a tool with schema
#[macro_export]
macro_rules! define_tool {
    (
        name: $name:expr,
        description: $desc:expr,
        params: $params:tt,
        capabilities: $caps:expr,
        execute: |$input:ident| $body:expr
    ) => {
        #[derive(Debug)]
        pub struct $name;
        
        #[async_trait]
        impl Tool for $name {
            fn name(&self) -> &str { $name }
            
            fn description(&self) -> &str { $desc }
            
            fn parameters(&self) -> Value {
                serde_json::json!($params)
            }
            
            fn required_capabilities(&self) -> Vec<Capability> {
                $caps
            }
            
            async fn execute(&self, $input: Value) -> Result<ToolOutput, ToolError> {
                $body
            }
        }
    };
}

/// Helper to create parameter schema
pub fn string_param(description: impl Into<String>) -> Value {
    serde_json::json!({
        "type": "string",
        "description": description.into()
    })
}

pub fn number_param(description: impl Into<String>) -> Value {
    serde_json::json!({
        "type": "number",
        "description": description.into()
    })
}

pub fn boolean_param(description: impl Into<String>) -> Value {
    serde_json::json!({
        "type": "boolean",
        "description": description.into()
    })
}

pub fn array_param(description: impl Into<String>, items: Value) -> Value {
    serde_json::json!({
        "type": "array",
        "description": description.into(),
        "items": items
    })
}
