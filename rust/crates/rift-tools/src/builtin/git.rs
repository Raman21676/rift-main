//! Git tools for version control operations

use async_trait::async_trait;
use rift_core::capability::Capability;
use rift_core::plugin::{Tool, ToolError, ToolOutput};
use serde_json::Value;
use std::path::Path;

/// Get git repository status
#[derive(Debug)]
pub struct GitStatusTool;

impl GitStatusTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Get the working tree status of a git repository"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the git repository (default: current directory)",
                    "default": "."
                }
            },
            "required": []
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let repo = git2::Repository::discover(Path::new(path))
            .map_err(|e| ToolError::ExecutionFailed(format!("Not a git repository: {}", e)))?;

        let mut output = String::new();

        // Branch info
        if let Ok(head) = repo.head() {
            if let Some(name) = head.shorthand() {
                output.push_str(&format!("On branch {}\n", name));
            }
        }

        // Status
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);

        let statuses = repo.statuses(Some(&mut opts))
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("???");
            let status = entry.status();

            if status.is_index_new() || status.is_index_modified() || status.is_index_deleted() || status.is_index_renamed() || status.is_index_typechange() {
                staged.push(path.to_string());
            }
            if status.is_wt_modified() || status.is_wt_deleted() || status.is_wt_renamed() || status.is_wt_typechange() {
                unstaged.push(path.to_string());
            }
            if status.is_wt_new() {
                untracked.push(path.to_string());
            }
        }

        if staged.is_empty() && unstaged.is_empty() && untracked.is_empty() {
            output.push_str("nothing to commit, working tree clean\n");
        } else {
            if !staged.is_empty() {
                output.push_str("\nChanges to be committed:\n");
                for f in &staged {
                    output.push_str(&format!("  modified: {}\n", f));
                }
            }
            if !unstaged.is_empty() {
                output.push_str("\nChanges not staged for commit:\n");
                for f in &unstaged {
                    output.push_str(&format!("  modified: {}\n", f));
                }
            }
            if !untracked.is_empty() {
                output.push_str("\nUntracked files:\n");
                for f in &untracked {
                    output.push_str(&format!("  {}\n", f));
                }
            }
        }

        Ok(ToolOutput::success(output).with_data(serde_json::json!({
            "staged": staged,
            "unstaged": unstaged,
            "untracked": untracked,
        })))
    }
}

/// Commit changes in a git repository
#[derive(Debug)]
pub struct GitCommitTool;

impl GitCommitTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Commit changes in a git repository with a message"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the git repository (default: current directory)",
                    "default": "."
                },
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "author_name": {
                    "type": "string",
                    "description": "Author name (optional, defaults to git config)",
                    "default": "Rift Agent"
                },
                "author_email": {
                    "type": "string",
                    "description": "Author email (optional, defaults to git config)",
                    "default": "rift@localhost"
                }
            },
            "required": ["message"]
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileWrite]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("Missing 'message' parameter".to_string()))?;

        let author_name = input
            .get("author_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Rift Agent");

        let author_email = input
            .get("author_email")
            .and_then(|v| v.as_str())
            .unwrap_or("rift@localhost");

        let repo = git2::Repository::discover(Path::new(path))
            .map_err(|e| ToolError::ExecutionFailed(format!("Not a git repository: {}", e)))?;

        let mut index = repo.index()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to stage files: {}", e)))?;

        index.write()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write index: {}", e)))?;

        let tree_id = index.write_tree()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let tree = repo.find_tree(tree_id)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let sig = git2::Signature::now(author_name, author_email)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let parent = match repo.head() {
            Ok(head) => {
                let target = head.target()
                    .ok_or_else(|| ToolError::ExecutionFailed("Invalid HEAD".to_string()))?;
                repo.find_commit(target)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            }
            Err(_) => {
                // Initial commit
                return repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                    .map(|id| ToolOutput::success(format!("Created initial commit: {}", id)))
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()));
            }
        };

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .map(|id| ToolOutput::success(format!("Created commit: {}", id)))
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

/// Push commits to remote
#[derive(Debug)]
pub struct GitPushTool;

impl GitPushTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitPushTool {
    fn name(&self) -> &str {
        "git_push"
    }

    fn description(&self) -> &str {
        "Push commits to a remote repository (uses system git binary for auth compatibility)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the git repository (default: current directory)",
                    "default": "."
                },
                "remote": {
                    "type": "string",
                    "description": "Remote name (default: origin)",
                    "default": "origin"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch to push (default: current branch)"
                }
            },
            "required": []
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::NetworkAccess, Capability::ShellExecute]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let remote = input
            .get("remote")
            .and_then(|v| v.as_str())
            .unwrap_or("origin");

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("-C").arg(path).arg("push").arg(remote);

        if let Some(branch) = input.get("branch").and_then(|v| v.as_str()) {
            cmd.arg(branch);
        }

        let output = cmd.output()
            .await
            .map_err(|e| ToolError::Io(e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if stdout.is_empty() { stderr.to_string() } else { stdout.to_string() };

        if output.status.success() {
            Ok(ToolOutput::success(result))
        } else {
            Ok(ToolOutput::error(result))
        }
    }
}

/// Get git diff
#[derive(Debug)]
pub struct GitDiffTool;

impl GitDiffTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show changes between working tree and index (or last commit)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the git repository (default: current directory)",
                    "default": "."
                },
                "staged": {
                    "type": "boolean",
                    "description": "Show staged diff instead of unstaged",
                    "default": false
                }
            },
            "required": []
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let staged = input
            .get("staged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("-C").arg(path).arg("diff");
        if staged {
            cmd.arg("--staged");
        }

        let output = cmd.output()
            .await
            .map_err(|e| ToolError::Io(e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.is_empty() {
            Ok(ToolOutput::success("No diff to show".to_string()))
        } else {
            Ok(ToolOutput::success(stdout.to_string()))
        }
    }
}

/// List or create git branches
#[derive(Debug)]
pub struct GitBranchTool;

impl GitBranchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GitBranchTool {
    fn name(&self) -> &str {
        "git_branch"
    }

    fn description(&self) -> &str {
        "List git branches or create a new branch"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the git repository (default: current directory)",
                    "default": "."
                },
                "create": {
                    "type": "string",
                    "description": "Create a new branch with this name"
                }
            },
            "required": []
        })
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("-C").arg(path).arg("branch");

        if let Some(name) = input.get("create").and_then(|v| v.as_str()) {
            cmd.arg(name);
        } else {
            cmd.arg("--list").arg("-a");
        }

        let output = cmd.output()
            .await
            .map_err(|e| ToolError::Io(e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(ToolOutput::success(stdout.to_string()))
        } else {
            Ok(ToolOutput::error(stderr.to_string()))
        }
    }
}
