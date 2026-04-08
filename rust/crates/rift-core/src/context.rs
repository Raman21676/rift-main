//! Context-aware project analysis
//!
//! Before planning, the agent examines the existing project structure
//! to generate more informed plans that work with what's already there.

use crate::task::{Job, Task};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Maximum files to read for context (prevent overwhelming the LLM)
const MAX_FILES_TO_READ: usize = 20;
const MAX_FILE_SIZE: usize = 50_000; // 50KB max per file

/// Project context gathered before planning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Current working directory
    pub working_dir: PathBuf,
    /// List of files in the project (relative paths)
    pub files: Vec<String>,
    /// Directory structure (top-level only for brevity)
    pub directories: Vec<String>,
    /// Content of important config files
    pub config_files: HashMap<String, String>,
    /// Detected project type
    pub project_type: Option<ProjectType>,
    /// Git repository info
    pub git_info: Option<GitInfo>,
    /// Summary of key files
    pub key_files_summary: String,
}

/// Detected project type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    NodeJs,
    Python,
    Go,
    Java,
    Docker,
    StaticSite,
    Mixed,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust"),
            ProjectType::NodeJs => write!(f, "Node.js"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Docker => write!(f, "Docker"),
            ProjectType::StaticSite => write!(f, "Static Site"),
            ProjectType::Mixed => write!(f, "Mixed"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Git repository information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub is_repo: bool,
    pub current_branch: Option<String>,
    pub has_uncommitted_changes: bool,
    pub remote_url: Option<String>,
}

/// Context gatherer
pub struct ContextGatherer;

impl ContextGatherer {
    /// Gather context from the current directory
    pub async fn gather(working_dir: impl AsRef<Path>) -> Result<ProjectContext, ContextError> {
        let working_dir = working_dir.as_ref().to_path_buf();
        info!("Gathering project context from {}", working_dir.display());

        // List files and directories
        let (files, directories) = Self::list_files(&working_dir).await?;
        
        // Detect project type
        let project_type = Self::detect_project_type(&files);
        info!("Detected project type: {}", project_type);

        // Read important config files
        let config_files = Self::read_config_files(&working_dir, &files).await?;

        // Get git info
        let git_info = Self::gather_git_info(&working_dir).await?;

        // Generate summary
        let key_files_summary = Self::generate_summary(&files, &config_files, &project_type);

        Ok(ProjectContext {
            working_dir,
            files,
            directories,
            config_files,
            project_type: Some(project_type),
            git_info: Some(git_info),
            key_files_summary,
        })
    }

    /// List files and directories (non-recursive for top level)
    async fn list_files(working_dir: &Path) -> Result<(Vec<String>, Vec<String>), ContextError> {
        let mut files = Vec::new();
        let mut directories = Vec::new();

        let mut entries = tokio::fs::read_dir(working_dir)
            .await
            .map_err(|e| ContextError::Io(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ContextError::Io(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files and common ignore patterns
            if name.starts_with('.') 
                || name == "node_modules" 
                || name == "target"
                || name == "__pycache__"
                || name == "dist"
                || name == "build"
            {
                continue;
            }

            let file_type = entry.file_type().await
                .map_err(|e| ContextError::Io(e.to_string()))?;

            if file_type.is_file() {
                files.push(name);
            } else if file_type.is_dir() {
                directories.push(name);
            }
        }

        // Also check for some common files in subdirectories (limited depth)
        let common_patterns = [
            "src/main.rs", "src/lib.rs",
            "src/index.js", "src/main.js",
            "src/main.py", "src/app.py",
            "main.go", "cmd/main.go",
        ];

        for pattern in &common_patterns {
            let path = working_dir.join(pattern);
            if path.exists() && path.is_file() {
                let pattern_str = pattern.to_string();
                if !files.contains(&pattern_str) {
                    files.push(pattern_str);
                }
            }
        }

        Ok((files, directories))
    }

    /// Detect project type from files
    fn detect_project_type(files: &[String]) -> ProjectType {
        let has_cargo = files.contains(&"Cargo.toml".to_string());
        let has_package_json = files.contains(&"package.json".to_string());
        let has_requirements = files.contains(&"requirements.txt".to_string()) 
            || files.contains(&"pyproject.toml".to_string());
        let has_go_mod = files.contains(&"go.mod".to_string());
        let has_pom = files.contains(&"pom.xml".to_string()) 
            || files.contains(&"build.gradle".to_string());
        let has_docker = files.contains(&"Dockerfile".to_string()) 
            || files.contains(&"docker-compose.yml".to_string());
        let has_html = files.iter().any(|f| f.ends_with(".html"));

        let mut detected = Vec::new();
        if has_cargo { detected.push(ProjectType::Rust); }
        if has_package_json { detected.push(ProjectType::NodeJs); }
        if has_requirements { detected.push(ProjectType::Python); }
        if has_go_mod { detected.push(ProjectType::Go); }
        if has_pom { detected.push(ProjectType::Java); }
        if has_docker { detected.push(ProjectType::Docker); }
        if has_html { detected.push(ProjectType::StaticSite); }

        match detected.len() {
            0 => ProjectType::Unknown,
            1 => detected[0].clone(),
            _ => ProjectType::Mixed,
        }
    }

    /// Read important config files
    async fn read_config_files(
        working_dir: &Path, 
        files: &[String]
    ) -> Result<HashMap<String, String>, ContextError> {
        let mut configs = HashMap::new();
        
        let important_files = [
            "Cargo.toml",
            "package.json",
            "requirements.txt",
            "pyproject.toml",
            "go.mod",
            "README.md",
            "Dockerfile",
            "docker-compose.yml",
            ".gitignore",
        ];

        for filename in &important_files {
            if files.contains(&filename.to_string()) {
                let path = working_dir.join(filename);
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        // Truncate very large files
                        let truncated = if content.len() > MAX_FILE_SIZE {
                            format!("{}... [truncated]", &content[..MAX_FILE_SIZE])
                        } else {
                            content
                        };
                        configs.insert(filename.to_string(), truncated);
                    }
                    Err(e) => {
                        warn!("Could not read {}: {}", filename, e);
                    }
                }
            }
        }

        Ok(configs)
    }

    /// Gather git repository information
    async fn gather_git_info(working_dir: &Path) -> Result<GitInfo, ContextError> {
        let git_dir = working_dir.join(".git");
        let is_repo = git_dir.exists();

        if !is_repo {
            return Ok(GitInfo {
                is_repo: false,
                current_branch: None,
                has_uncommitted_changes: false,
                remote_url: None,
            });
        }

        // Get current branch
        let branch_output = tokio::process::Command::new("git")
            .args(&["-C", &working_dir.to_string_lossy(), "branch", "--show-current"])
            .output()
            .await
            .ok();

        let current_branch = branch_output.and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

        // Check for uncommitted changes
        let status_output = tokio::process::Command::new("git")
            .args(&["-C", &working_dir.to_string_lossy(), "status", "--porcelain"])
            .output()
            .await
            .ok();

        let has_uncommitted_changes = status_output.map(|o| {
            !o.stdout.is_empty()
        }).unwrap_or(false);

        // Get remote URL
        let remote_output = tokio::process::Command::new("git")
            .args(&["-C", &working_dir.to_string_lossy(), "remote", "get-url", "origin"])
            .output()
            .await
            .ok();

        let remote_url = remote_output.and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

        Ok(GitInfo {
            is_repo: true,
            current_branch,
            has_uncommitted_changes,
            remote_url,
        })
    }

    /// Generate a human-readable summary of key files
    fn generate_summary(
        files: &[String], 
        configs: &HashMap<String, String>,
        project_type: &ProjectType
    ) -> String {
        let mut summary = format!("Project Type: {}\n", project_type);
        
        summary.push_str(&format!("Total files: {}\n", files.len()));
        
        // List key source files
        let source_files: Vec<&String> = files.iter()
            .filter(|f| {
                f.ends_with(".rs") || 
                f.ends_with(".js") || 
                f.ends_with(".ts") ||
                f.ends_with(".py") ||
                f.ends_with(".go") ||
                f.ends_with(".java") ||
                f.ends_with(".html")
            })
            .take(10)
            .collect();
        
        if !source_files.is_empty() {
            summary.push_str("Key source files:\n");
            for f in source_files {
                summary.push_str(&format!("  - {}\n", f));
            }
        }

        // Config file summaries
        if !configs.is_empty() {
            summary.push_str("\nConfiguration files present:\n");
            for (name, content) in configs.iter().take(3) {
                summary.push_str(&format!("  {} ({} bytes)\n", name, content.len()));
            }
        }

        summary
    }

    /// Format context for inclusion in planning prompt
    pub fn format_for_prompt(context: &ProjectContext) -> String {
        let mut prompt = String::from("CURRENT PROJECT CONTEXT:\n");
        prompt.push_str("========================\n\n");
        
        prompt.push_str(&format!("Working Directory: {}\n", context.working_dir.display()));
        prompt.push_str(&format!("Project Type: {}\n\n", context.project_type.as_ref().unwrap_or(&ProjectType::Unknown)));
        
        if let Some(ref git) = context.git_info {
            if git.is_repo {
                prompt.push_str(&format!("Git Branch: {}\n", 
                    git.current_branch.as_ref().unwrap_or(&"unknown".to_string())));
                if git.has_uncommitted_changes {
                    prompt.push_str("⚠️  Has uncommitted changes\n");
                }
                prompt.push('\n');
            }
        }

        prompt.push_str("Files in project:\n");
        for file in context.files.iter().take(15) {
            prompt.push_str(&format!("  - {}\n", file));
        }
        if context.files.len() > 15 {
            prompt.push_str(&format!("  ... and {} more files\n", context.files.len() - 15));
        }
        prompt.push('\n');

        if !context.config_files.is_empty() {
            prompt.push_str("Configuration files:\n");
            for (name, content) in &context.config_files {
                prompt.push_str(&format!("\n--- {} ---\n", name));
                // Show first 30 lines or 2000 chars
                let preview: String = content.lines().take(30).collect::<Vec<_>>().join("\n");
                let preview = if preview.len() > 2000 {
                    format!("{}... [truncated]", &preview[..2000])
                } else {
                    preview
                };
                prompt.push_str(&preview);
                prompt.push('\n');
            }
        }

        prompt.push_str("\n========================\n");
        prompt.push_str("IMPORTANT: When planning tasks, consider the existing project structure above.\n");
        prompt.push_str("Do not create files that already exist unless explicitly asked to modify them.\n");
        prompt.push_str("Work WITH the existing project structure, not against it.\n");

        prompt
    }
}

/// Context-related errors
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("IO error: {0}")]
    Io(String),
    
    #[error("Failed to gather context: {0}")]
    GatherFailed(String),
}
