//! Web tools - fetch and search

use async_trait::async_trait;
use rift_core::capability::Capability;
use rift_core::plugin::{Tool, ToolError, ToolOutput};
use serde_json::Value;

/// Fetch content from a URL
#[derive(Debug)]
pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }
    
    /// Extract readable text from HTML
    fn extract_text(&self, html: &str) -> String {
        // Simple HTML to text extraction
        // Remove script and style tags with content
        let mut text = html.to_string();
        
        // Remove script tags
        while let Some(start) = text.find("<script") {
            if let Some(end) = text[start..].find("</script>") {
                text.replace_range(start..start + end + 9, "");
            } else {
                break;
            }
        }
        
        // Remove style tags
        while let Some(start) = text.find("<style") {
            if let Some(end) = text[start..].find("</style>") {
                text.replace_range(start..start + end + 8, "");
            } else {
                break;
            }
        }
        
        // Simple tag removal
        let mut result = String::new();
        let mut in_tag = false;
        let mut in_entity = false;
        let mut entity = String::new();
        
        for ch in text.chars() {
            if in_entity {
                if ch == ';' {
                    // Decode common HTML entities
                    match entity.as_str() {
                        "amp" => result.push('&'),
                        "lt" => result.push('<'),
                        "gt" => result.push('>'),
                        "quot" => result.push('"'),
                        "apos" | "#39" => result.push('\''),
                        "nbsp" => result.push(' '),
                        _ => result.push_str(&format!("&{};", entity)),
                    }
                    in_entity = false;
                    entity.clear();
                } else {
                    entity.push(ch);
                }
            } else if ch == '&' {
                in_entity = true;
            } else if ch == '<' {
                in_tag = true;
            } else if ch == '>' {
                in_tag = false;
            } else if !in_tag {
                result.push(ch);
            }
        }
        
        // Clean up whitespace
        result.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    
    fn description(&self) -> &str {
        "Fetch content from a URL and extract readable text"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters to return (default: 5000)",
                    "default": 5000
                }
            },
            "required": ["url"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::NetworkAccess]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'url' parameter".to_string()))?;
        
        let max_length = input
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000) as usize;
        
        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolOutput::error("URL must start with http:// or https://".to_string()));
        }
        
        // Fetch content
        let response = self.client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to fetch URL: {}", e)))?;
        
        let status = response.status();
        if !status.is_success() {
            return Ok(ToolOutput::error(format!(
                "HTTP error {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }
        
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();
        
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {}", e)))?;
        
        let total_length = body.len();
        
        // Extract text based on content type
        let text = if content_type.contains("text/html") {
            self.extract_text(&body)
        } else {
            body
        };
        
        // Truncate if needed
        let truncated = if text.len() > max_length {
            format!("{}...\n[truncated from {} chars]", &text[..max_length], text.len())
        } else {
            text
        };
        
        Ok(ToolOutput::success(truncated).with_data(serde_json::json!({
            "url": url,
            "content_type": content_type,
            "total_length": total_length,
        })))
    }
}

/// Search the web using DuckDuckGo
#[derive(Debug)]
pub struct WebSearchTool {
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Rift AI Assistant (github.com/rift-ai)")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    
    fn description(&self) -> &str {
        "Search the web using DuckDuckGo"
    }
    
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results (default: 5, max: 10)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }
    
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::NetworkAccess]
    }
    
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'query' parameter".to_string()))?;
        
        let num_results = input
            .get("num_results")
            .and_then(|v| v.as_u64())
            .map(|n| n.min(10) as usize)
            .unwrap_or(5);
        
        // Use DuckDuckGo HTML interface
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search failed: {}", e)))?;
        
        let html = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {}", e)))?;
        
        // Parse results (simple regex-based parsing)
        let mut results = Vec::new();
        
        // DuckDuckGo result format
        for cap in regex::Regex::new(r#"<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#)
            .unwrap()
            .captures_iter(&html)
        {
            if let (Some(url_match), Some(title_match)) = (cap.get(1), cap.get(2)) {
                let url = url_match.as_str();
                let title = title_match.as_str()
                    .replace("<b>", "")
                    .replace("</b>", "")
                    .replace("&quot;", "\"");
                
                // Skip ads and internal links
                if !url.contains("duckduckgo.com") && !url.starts_with("/") {
                    results.push(serde_json::json!({
                        "title": title,
                        "url": url,
                    }));
                }
                
                if results.len() >= num_results {
                    break;
                }
            }
        }
        
        if results.is_empty() {
            return Ok(ToolOutput::error("No search results found".to_string()));
        }
        
        // Format output
        let mut output = format!("Search results for '{}'\n\n", query);
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}\n   {}\n\n",
                i + 1,
                result["title"].as_str().unwrap_or(""),
                result["url"].as_str().unwrap_or("")
            ));
        }
        
        Ok(ToolOutput::success(output).with_data(serde_json::json!({
            "query": query,
            "results": results,
        })))
    }
}
