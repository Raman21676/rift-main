//! Deployment tool for publishing projects

use async_trait::async_trait;
use rift_core::capability::Capability;
use rift_core::plugin::{Tool, ToolError, ToolOutput};
use serde_json::Value;
use std::path::Path;

/// Deploy a project using various methods
#[derive(Debug)]
pub struct DeployTool;

impl DeployTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for DeployTool {
    fn name(&self) -> &str {
        "deploy"
    }

    fn description(&self) -> &str {
        "Deploy a project using git push, rsync, or a custom bash command"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "description": "Deployment method: git, rsync, or bash",
                    "enum": ["git", "rsync", "bash"]
                },
                "path": {
                    "type": "string",
                    "description": "Local path to deploy (default: current directory)",
                    "default": "."
                },
                "remote": {
                    "type": "string",
                    "description": "Remote destination for rsync (e.g., user@host:/var/www)"
                },
                "command": {
                    "type": "string",
                    "description": "Custom bash command for bash method"
                },
                "message": {
                    "type": "string",
                    "description": "Commit message for git method (default: Deploy)",
                    "default": "Deploy"
                }
            },
            "required": ["method"]
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ShellExecute, Capability::NetworkAccess]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let method = input
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'method' parameter".to_string()))?;

        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();

        match method {
            "git" => deploy_git(&path, input).await,
            "rsync" => deploy_rsync(&path, input).await,
            "bash" => deploy_bash(input).await,
            _ => Ok(ToolOutput::error(format!("Unknown deployment method: {}", method))),
        }
    }
}

async fn run_cmd(name: &str, args: &[&str], path: Option<&str>) -> Result<std::process::Output, ToolError> {
    let mut cmd = tokio::process::Command::new(name);
    if let Some(p) = path {
        cmd.current_dir(p);
    }
    cmd.args(args);
    cmd.output().await.map_err(|e| ToolError::Io(e))
}

async fn deploy_git(path: &str, input: Value) -> Result<ToolOutput, ToolError> {
    let message = input
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Deploy");

    if !Path::new(path).join(".git").exists() {
        return Ok(ToolOutput::error(format!(
            "No git repository found at {}. Initialize with: git init",
            path
        )));
    }

    let mut output_lines = Vec::new();

    // Stage all changes
    let add_result = run_cmd("git", &["add", "."], Some(path)).await?;
    if !add_result.status.success() {
        let stderr = String::from_utf8_lossy(&add_result.stderr);
        return Ok(ToolOutput::error(format!("git add failed: {}", stderr)));
    }

    // Commit
    let commit_result = run_cmd("git", &["commit", "-m", message], Some(path)).await?;
    let commit_stdout = String::from_utf8_lossy(&commit_result.stdout);
    let commit_stderr = String::from_utf8_lossy(&commit_result.stderr);

    if commit_result.status.success() {
        output_lines.push(format!("Committed: {}", commit_stdout.trim()));
    } else if commit_stderr.contains("nothing to commit") || commit_stdout.contains("nothing to commit") {
        output_lines.push("Nothing to commit".to_string());
    } else {
        output_lines.push(format!("Commit issue (continuing): {}", commit_stderr.trim()));
    }

    // Push
    let push_result = run_cmd("git", &["push"], Some(path)).await?;
    let push_stdout = String::from_utf8_lossy(&push_result.stdout);
    let push_stderr = String::from_utf8_lossy(&push_result.stderr);

    if push_result.status.success() {
        let result = if push_stdout.is_empty() { push_stderr.trim() } else { push_stdout.trim() };
        output_lines.push(format!("Pushed: {}", result));
    } else {
        let result = if push_stdout.is_empty() { push_stderr.trim() } else { push_stdout.trim() };
        return Ok(ToolOutput::error(format!("Push failed: {}", result)));
    }

    Ok(ToolOutput::success(output_lines.join("\n")))
}

async fn deploy_rsync(path: &str, input: Value) -> Result<ToolOutput, ToolError> {
    let remote = input
        .get("remote")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidInput("Missing 'remote' parameter for rsync".to_string()))?;

    // Ensure trailing slash on source for directory sync
    let source = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{}/", path)
    };

    let result = run_cmd("rsync", &["-avz", "--delete", &source, remote], None).await?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    if result.status.success() {
        Ok(ToolOutput::success(stdout.to_string()))
    } else {
        Ok(ToolOutput::error(format!("rsync failed: {}", stderr)))
    }
}

async fn deploy_bash(input: Value) -> Result<ToolOutput, ToolError> {
    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidInput("Missing 'command' parameter for bash deploy".to_string()))?;

    let result = run_cmd("bash", &["-c", command], None).await?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    if result.status.success() {
        let output = if stdout.is_empty() { "Command executed successfully".to_string() } else { stdout.to_string() };
        Ok(ToolOutput::success(output))
    } else {
        let error = if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() };
        Ok(ToolOutput::error(error))
    }
}
