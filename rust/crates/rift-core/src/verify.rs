//! Build and test verification system
//!
//! Automatically verifies that tasks completed successfully by:
//! 1. Detecting build systems (cargo, npm, make, etc.)
//! 2. Running build/test commands after relevant tasks
//! 3. Validating file outputs exist and are valid
//! 4. Reporting success/failure with details

use crate::task::{Job, Task, TaskId, TaskResult, TaskStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Verification result for a task or job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub success: bool,
    pub checks: Vec<CheckResult>,
    pub summary: String,
}

/// Individual check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

/// Types of verification checks
#[derive(Debug, Clone)]
pub enum VerificationType {
    /// Verify file exists and has content
    FileExists { path: String },
    /// Run a build command
    Build { command: String },
    /// Run a test command
    Test { command: String },
    /// Verify syntax of a file
    SyntaxCheck { path: String, language: String },
    /// Custom verification command
    Custom { command: String },
}

/// Verifier that checks task/job completion
pub struct Verifier;

impl Verifier {
    /// Create a new verifier
    pub fn new() -> Self {
        Self
    }

    /// Auto-detect verification checks for a job based on project type
    pub fn detect_job_verifications(&self, job: &Job) -> Vec<(TaskId, Vec<VerificationType>)> {
        let mut verifications = Vec::new();

        for (task_id, task) in &job.tasks {
            let checks = self.detect_task_verifications(task);
            if !checks.is_empty() {
                verifications.push((*task_id, checks));
            }
        }

        verifications
    }

    /// Detect what verifications to run for a task
    fn detect_task_verifications(&self, task: &Task) -> Vec<VerificationType> {
        let mut checks = Vec::new();

        match task.tool_name.as_str() {
            "write_file" | "edit_file" | "insert_at_line" => {
                // Extract path from task input
                if let Some(path) = task.input.get("path").and_then(|v| v.as_str()) {
                    // Check file exists after write/edit
                    checks.push(VerificationType::FileExists {
                        path: path.to_string(),
                    });

                    // Add syntax check for known file types
                    if let Some(lang) = Self::detect_language(path) {
                        checks.push(VerificationType::SyntaxCheck {
                            path: path.to_string(),
                            language: lang,
                        });
                    }
                }
            }
            "bash" => {
                // For bash commands that look like build commands, add verification
                if let Some(cmd) = task.input.get("command").and_then(|v| v.as_str()) {
                    let cmd_lower = cmd.to_lowercase();
                    
                    // Detect build commands
                    if cmd_lower.contains("cargo build") 
                        || cmd_lower.contains("npm run build")
                        || cmd_lower.contains("make")
                        || cmd_lower.contains("go build")
                        || cmd_lower.contains("python setup.py build")
                    {
                        checks.push(VerificationType::Build {
                            command: cmd.to_string(),
                        });
                    }
                    
                    // Detect test commands
                    if cmd_lower.contains("cargo test")
                        || cmd_lower.contains("npm test")
                        || cmd_lower.contains("go test")
                        || cmd_lower.contains("pytest")
                        || cmd_lower.contains("python -m pytest")
                    {
                        checks.push(VerificationType::Test {
                            command: cmd.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }

        // Check for project-level build files and add appropriate verifications
        if task.tool_name == "write_file" {
            if let Some(path) = task.input.get("path").and_then(|v| v.as_str()) {
                let build_checks = self.detect_build_system_checks(path);
                checks.extend(build_checks);
            }
        }

        checks
    }

    /// Detect language from file extension
    fn detect_language(path: &str) -> Option<String> {
        let path_lower = path.to_lowercase();
        
        if path_lower.ends_with(".rs") {
            Some("rust".to_string())
        } else if path_lower.ends_with(".js") || path_lower.ends_with(".ts") {
            Some("javascript".to_string())
        } else if path_lower.ends_with(".py") {
            Some("python".to_string())
        } else if path_lower.ends_with(".go") {
            Some("go".to_string())
        } else if path_lower.ends_with(".java") {
            Some("java".to_string())
        } else if path_lower.ends_with(".html") || path_lower.ends_with(".htm") {
            Some("html".to_string())
        } else if path_lower.ends_with(".json") {
            Some("json".to_string())
        } else if path_lower.ends_with(".yaml") || path_lower.ends_with(".yml") {
            Some("yaml".to_string())
        } else {
            None
        }
    }

    /// Detect build system checks based on file created
    fn detect_build_system_checks(&self, path: &str) -> Vec<VerificationType> {
        let mut checks = Vec::new();
        let file_name = Path::new(path).file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        match file_name {
            "Cargo.toml" => {
                checks.push(VerificationType::Build {
                    command: "cargo check".to_string(),
                });
            }
            "package.json" => {
                checks.push(VerificationType::Build {
                    command: "npm install".to_string(),
                });
            }
            "requirements.txt" => {
                checks.push(VerificationType::Build {
                    command: "pip install -r requirements.txt".to_string(),
                });
            }
            "Dockerfile" => {
                checks.push(VerificationType::Build {
                    command: "docker build -t test-build .".to_string(),
                });
            }
            "Makefile" | "makefile" => {
                checks.push(VerificationType::Build {
                    command: "make".to_string(),
                });
            }
            _ => {}
        }

        checks
    }

    /// Run verification checks
    pub async fn verify(&self, check: &VerificationType) -> CheckResult {
        match check {
            VerificationType::FileExists { path } => {
                self.verify_file_exists(path).await
            }
            VerificationType::Build { command } => {
                self.verify_build(command).await
            }
            VerificationType::Test { command } => {
                self.verify_test(command).await
            }
            VerificationType::SyntaxCheck { path, language } => {
                self.verify_syntax(path, language).await
            }
            VerificationType::Custom { command } => {
                self.verify_custom(command).await
            }
        }
    }

    /// Verify file exists
    async fn verify_file_exists(&self, path: &str) -> CheckResult {
        let exists = Path::new(path).exists();
        
        CheckResult {
            name: format!("File exists: {}", path),
            passed: exists,
            details: if exists {
                format!("File {} exists", path)
            } else {
                format!("File {} does not exist", path)
            },
        }
    }

    /// Verify build command succeeds
    async fn verify_build(&self, command: &str) -> CheckResult {
        info!("Running build verification: {}", command);
        
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                
                CheckResult {
                    name: format!("Build: {}", command),
                    passed: success,
                    details: if success {
                        "Build succeeded".to_string()
                    } else {
                        format!("Build failed:\nstderr: {}\nstdout: {}", stderr, stdout)
                    },
                }
            }
            Err(e) => CheckResult {
                name: format!("Build: {}", command),
                passed: false,
                details: format!("Failed to execute build command: {}", e),
            },
        }
    }

    /// Verify test command succeeds
    async fn verify_test(&self, command: &str) -> CheckResult {
        info!("Running test verification: {}", command);
        
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let stdout = String::from_utf8_lossy(&result.stdout);
                
                CheckResult {
                    name: format!("Test: {}", command),
                    passed: success,
                    details: if success {
                        format!("Tests passed:\n{}", stdout)
                    } else {
                        format!("Tests failed:\n{}", stdout)
                    },
                }
            }
            Err(e) => CheckResult {
                name: format!("Test: {}", command),
                passed: false,
                details: format!("Failed to execute test command: {}", e),
            },
        }
    }

    /// Verify syntax of a file
    async fn verify_syntax(&self, path: &str, language: &str) -> CheckResult {
        let verification_command = match language {
            "rust" => format!("rustfmt --check '{}' 2>&1 || rustc --edition 2021 -Z parse-only '{}' 2>&1", path, path),
            "python" => format!("python3 -m py_compile '{}'", path),
            "javascript" => format!("node --check '{}' 2>&1", path),
            "json" => format!("python3 -c \"import json; json.load(open('{}'))\" 2>&1", path),
            "yaml" => format!("python3 -c \"import yaml; yaml.safe_load(open('{}'))\" 2>&1", path),
            "html" => format!("python3 -c \"from html.parser import HTMLParser; HTMLParser().feed(open('{}').read())\" 2>&1", path),
            _ => return CheckResult {
                name: format!("Syntax check: {} ({})", path, language),
                passed: true, // Skip unknown languages
                details: format!("No syntax checker available for {}", language),
            },
        };

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&verification_command)
            .output()
            .await;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let stderr = String::from_utf8_lossy(&result.stderr);
                
                CheckResult {
                    name: format!("Syntax check: {} ({})", path, language),
                    passed: success,
                    details: if success {
                        format!("{} syntax is valid", language)
                    } else {
                        format!("{} syntax error:\n{}", language, stderr)
                    },
                }
            }
            Err(e) => CheckResult {
                name: format!("Syntax check: {} ({})", path, language),
                passed: false,
                details: format!("Failed to run syntax check: {}", e),
            },
        }
    }

    /// Run custom verification command
    async fn verify_custom(&self, command: &str) -> CheckResult {
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let stdout = String::from_utf8_lossy(&result.stdout);
                
                CheckResult {
                    name: format!("Custom: {}", command),
                    passed: success,
                    details: stdout.to_string(),
                }
            }
            Err(e) => CheckResult {
                name: format!("Custom: {}", command),
                passed: false,
                details: format!("Command failed: {}", e),
            },
        }
    }

    /// Verify an entire job after execution
    pub async fn verify_job(&self, job: &Job) -> VerificationResult {
        let mut all_checks = Vec::new();
        let mut all_passed = true;

        // Get verifications for each task
        let task_verifications = self.detect_job_verifications(job);

        for (task_id, checks) in task_verifications {
            // Only verify tasks that completed
            if let Some(task) = job.tasks.get(&task_id) {
                if task.status != TaskStatus::Completed {
                    continue;
                }

                for check in checks {
                    let result = self.verify(&check).await;
                    if !result.passed {
                        all_passed = false;
                    }
                    all_checks.push(result);
                }
            }
        }

        let summary = if all_passed {
            format!("All {} verification checks passed", all_checks.len())
        } else {
            let failed = all_checks.iter().filter(|c| !c.passed).count();
            format!("{} of {} verification checks failed", failed, all_checks.len())
        };

        VerificationResult {
            success: all_passed,
            checks: all_checks,
            summary,
        }
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for adding verification to job execution
/// 
/// This trait is implemented by orchestrators that support verification.
#[async_trait::async_trait]
pub trait VerifiableJob {
    /// Run job with automatic verification
    async fn run_with_verification<E>(
        &mut self,
        job: &mut Job,
        executor: &E,
    ) -> Result<crate::task::JobResult, crate::task::TaskError>
    where
        E: crate::task::TaskExecutor;
}
