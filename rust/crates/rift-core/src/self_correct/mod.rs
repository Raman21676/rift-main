//! Self-correction system for autonomous task recovery
//!
//! When tasks fail, the system analyzes the failure and attempts to:
//! 1. Retry the same task (for transient failures)
//! 2. Modify the task based on error analysis
//! 3. Generate new corrective tasks
//! 4. Skip optional tasks that can't be completed

pub mod orchestrator;

use crate::llm::{LlmClient, Message};
use crate::task::{Job, Task, TaskId, TaskResult, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Maximum number of retry attempts per task
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Strategy for correcting a failed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrectionStrategy {
    /// Retry the same task (for transient failures like network issues)
    Retry,
    /// Modify the task parameters based on error analysis
    Modify { new_input: serde_json::Value },
    /// Add new tasks to fix the underlying issue, then retry
    AddPrerequisite { new_tasks: Vec<CorrectiveTask> },
    /// Skip this task if it's optional
    Skip,
    /// Fail the entire job - this error is unrecoverable
    Fail,
}

/// A corrective task to add to the job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectiveTask {
    pub name: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub description: String,
}

/// Analysis result from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    /// Why the task failed
    pub reason: String,
    /// Whether this is recoverable
    pub recoverable: bool,
    /// The correction strategy to use
    pub strategy: CorrectionStrategy,
    /// Explanation of the fix
    pub explanation: String,
}

/// Tracks retry state for a task
#[derive(Debug, Clone)]
struct RetryState {
    attempt: u32,
    original_input: serde_json::Value,
}

/// Self-correction engine
pub struct SelfCorrector {
    llm_client: LlmClient,
    max_retries: u32,
    retry_states: HashMap<TaskId, RetryState>,
    enabled: bool,
}

impl SelfCorrector {
    /// Create a new self-corrector
    pub fn new(llm_client: LlmClient) -> Self {
        Self {
            llm_client,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_states: HashMap::new(),
            enabled: true,
        }
    }

    /// Disable self-correction
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Check if self-correction is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Initialize retry state for a task before first execution
    pub fn init_task(&mut self, task: &Task) {
        self.retry_states.insert(
            task.id,
            RetryState {
                attempt: 0,
                original_input: task.input.clone(),
            },
        );
    }

    /// Analyze a task failure and determine correction strategy
    pub async fn analyze_failure(
        &self,
        task: &Task,
        result: &TaskResult,
        job_context: &JobContext,
    ) -> Result<FailureAnalysis, CorrectionError> {
        if !self.enabled {
            return Ok(FailureAnalysis {
                reason: "Self-correction disabled".to_string(),
                recoverable: false,
                strategy: CorrectionStrategy::Fail,
                explanation: "Automatic correction is disabled".to_string(),
            });
        }

        let retry_state = self.retry_states.get(&task.id)
            .ok_or_else(|| CorrectionError::StateNotFound(task.id))?;

        // Check if we've exceeded max retries
        if retry_state.attempt >= self.max_retries {
            warn!("Task {} exceeded max retries ({})", task.name, self.max_retries);
            return Ok(FailureAnalysis {
                reason: format!("Exceeded maximum retry attempts ({})", self.max_retries),
                recoverable: false,
                strategy: CorrectionStrategy::Fail,
                explanation: "Task failed too many times".to_string(),
            });
        }

        let prompt = self.build_analysis_prompt(task, result, job_context, retry_state.attempt);
        
        debug!("Analyzing failure for task '{}' (attempt {})", task.name, retry_state.attempt);

        let response = self.llm_client
            .chat(vec![Message::user(prompt)])
            .await
            .map_err(|e| CorrectionError::Llm(e.to_string()))?;

        // Parse the response
        let analysis = self.parse_analysis_response(&response, task)?;
        
        info!(
            "Failure analysis for '{}': {} (recoverable: {})",
            task.name, analysis.reason, analysis.recoverable
        );

        Ok(analysis)
    }

    /// Build the prompt for failure analysis
    fn build_analysis_prompt(
        &self,
        task: &Task,
        result: &TaskResult,
        job_context: &JobContext,
        attempt: u32,
    ) -> String {
        let completed_tasks: Vec<String> = job_context
            .completed_tasks
            .iter()
            .map(|(name, output)| format!("- {}: {}", name, output))
            .collect();
        
        let completed_str = if completed_tasks.is_empty() {
            "  (none)".to_string()
        } else {
            completed_tasks.join("\n")
        };

        format!(
            "You are analyzing a task failure in an autonomous AI system.

TASK DETAILS:
- Name: {}
- Tool: {}
- Description: {}
- Attempt: {}/{}
- Input: {}

FAILURE OUTPUT:
{}

JOB CONTEXT:
- Job name: {}
- Completed tasks:
{}
- Failed tasks:
{}

Analyze the failure and respond in this exact JSON format:
{{
    \"reason\": \"Brief explanation of why it failed\",
    \"recoverable\": true/false,
    \"strategy\": \"Retry\" | \"Modify\" | \"AddPrerequisite\" | \"Skip\" | \"Fail\",
    \"explanation\": \"What you plan to do to fix it\"
}}

For strategy:
- \"Retry\": Use for transient failures (network timeout, rate limit) - same input
- \"Modify\": Use when input needs fixing (wrong path, syntax error) - provide corrected input
- \"AddPrerequisite\": Use when something else needs to be done first (missing directory, wrong branch)
- \"Skip\": Use only if task is truly optional
- \"Fail\": Use when error is unrecoverable (insufficient permissions, fundamental impossibility)

If using \"Modify\", also include:
- \"corrected_input\": {{\"key\": \"value\"}} // The fixed input parameters

If using \"AddPrerequisite\", also include:
- \"prerequisite_tasks\": [
    {{\"name\": \"task_name\", \"tool_name\": \"tool\", \"input\": {{}}, \"description\": \"what this does\"}}
  ]

Respond with ONLY the JSON object, no other text.",
            task.name,
            task.tool_name,
            task.description,
            attempt + 1,
            self.max_retries,
            serde_json::to_string_pretty(&task.input).unwrap_or_default(),
            result.output,
            job_context.job_name,
            completed_str,
            job_context.failed_tasks.join(", ")
        )
    }

    /// Parse the LLM's analysis response
    fn parse_analysis_response(&self, response: &str, _task: &Task) -> Result<FailureAnalysis, CorrectionError> {
        // Extract JSON from response (handle markdown code blocks)
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            response
        };

        #[derive(Deserialize)]
        struct PartialAnalysis {
            reason: String,
            recoverable: bool,
            strategy: String,
            explanation: String,
            #[serde(default)]
            corrected_input: Option<serde_json::Value>,
            #[serde(default)]
            prerequisite_tasks: Option<Vec<CorrectiveTask>>,
        }

        let partial: PartialAnalysis = serde_json::from_str(json_str)
            .map_err(|e| CorrectionError::Parse(format!("Failed to parse analysis: {}\nResponse: {}", e, response)))?;

        let strategy = match partial.strategy.as_str() {
            "Retry" => CorrectionStrategy::Retry,
            "Modify" => {
                if let Some(input) = partial.corrected_input {
                    CorrectionStrategy::Modify { new_input: input }
                } else {
                    warn!("Modify strategy requested but no corrected_input provided, falling back to Retry");
                    CorrectionStrategy::Retry
                }
            }
            "AddPrerequisite" => {
                if let Some(tasks) = partial.prerequisite_tasks {
                    CorrectionStrategy::AddPrerequisite { new_tasks: tasks }
                } else {
                    warn!("AddPrerequisite strategy requested but no tasks provided, falling back to Retry");
                    CorrectionStrategy::Retry
                }
            }
            "Skip" => CorrectionStrategy::Skip,
            "Fail" | _ => CorrectionStrategy::Fail,
        };

        Ok(FailureAnalysis {
            reason: partial.reason,
            recoverable: partial.recoverable,
            strategy,
            explanation: partial.explanation,
        })
    }

    /// Apply a correction to the job
    pub fn apply_correction(
        &mut self,
        job: &mut Job,
        failed_task_id: TaskId,
        analysis: &FailureAnalysis,
    ) -> Result<CorrectionResult, CorrectionError> {
        let retry_state = self.retry_states.get_mut(&failed_task_id)
            .ok_or_else(|| CorrectionError::StateNotFound(failed_task_id))?;

        retry_state.attempt += 1;

        match &analysis.strategy {
            CorrectionStrategy::Retry => {
                info!("Retrying task '{}' (attempt {})", 
                    job.tasks.get(&failed_task_id).map(|t| &t.name).unwrap_or(&"?".to_string()),
                    retry_state.attempt
                );
                
                // Reset task status to pending for retry
                if let Some(task) = job.tasks.get_mut(&failed_task_id) {
                    task.status = TaskStatus::Pending;
                    task.result = None;
                }
                
                Ok(CorrectionResult::Retry)
            }
            CorrectionStrategy::Modify { new_input } => {
                info!("Modifying task '{}' with corrected input", 
                    job.tasks.get(&failed_task_id).map(|t| &t.name).unwrap_or(&"?".to_string())
                );
                
                if let Some(task) = job.tasks.get_mut(&failed_task_id) {
                    task.input = new_input.clone();
                    task.status = TaskStatus::Pending;
                    task.result = None;
                }
                
                Ok(CorrectionResult::Modified)
            }
            CorrectionStrategy::AddPrerequisite { new_tasks } => {
                let failed_task = job.tasks.get(&failed_task_id)
                    .ok_or_else(|| CorrectionError::TaskNotFound(failed_task_id))?;
                
                info!("Adding {} prerequisite task(s) for '{}'", 
                    new_tasks.len(), failed_task.name
                );

                let mut new_task_ids = Vec::new();
                let mut last_new_id = failed_task_id;

                // Add new tasks before the failed task
                for corrective in new_tasks {
                    let mut new_task = Task::new(
                        corrective.name.clone(),
                        corrective.tool_name.clone(),
                        corrective.input.clone(),
                    );
                    new_task.description = corrective.description.clone();
                    new_task.dependencies = vec![last_new_id]; // Chain them
                    
                    let new_id = new_task.id;
                    job.add_task(new_task);
                    new_task_ids.push(new_id);
                    last_new_id = new_id;
                }

                // Update failed task to depend on the last new task
                if let Some(task) = job.tasks.get_mut(&failed_task_id) {
                    // Remove old dependencies that might conflict
                    task.dependencies.retain(|id| !new_task_ids.contains(id));
                    // Add dependency on the last prerequisite
                    if !task.dependencies.contains(&last_new_id) {
                        task.dependencies.push(last_new_id);
                    }
                    task.status = TaskStatus::Pending;
                    task.result = None;
                }

                // Initialize retry state for new tasks
                for id in &new_task_ids {
                    self.init_task(job.tasks.get(id).unwrap());
                }

                Ok(CorrectionResult::AddedPrerequisites(new_task_ids))
            }
            CorrectionStrategy::Skip => {
                info!("Skipping optional task '{}'", 
                    job.tasks.get(&failed_task_id).map(|t| &t.name).unwrap_or(&"?".to_string())
                );
                
                // Mark as completed with a note
                if let Some(task) = job.tasks.get_mut(&failed_task_id) {
                    task.status = TaskStatus::Completed;
                    task.result = Some(TaskResult {
                        success: true,
                        output: "Skipped (optional task)".to_string(),
                        data: None,
                        execution_time_ms: 0,
                    });
                }
                
                Ok(CorrectionResult::Skipped)
            }
            CorrectionStrategy::Fail => {
                Ok(CorrectionResult::Unrecoverable)
            }
        }
    }

    /// Get retry count for a task
    pub fn get_retry_count(&self, task_id: TaskId) -> u32 {
        self.retry_states.get(&task_id).map(|s| s.attempt).unwrap_or(0)
    }
}

/// Context about the job for failure analysis
#[derive(Debug, Default)]
pub struct JobContext {
    pub job_name: String,
    pub completed_tasks: Vec<(String, String)>, // (name, output_summary)
    pub failed_tasks: Vec<String>,
}

impl JobContext {
    pub fn new(job_name: impl Into<String>) -> Self {
        Self {
            job_name: job_name.into(),
            completed_tasks: Vec::new(),
            failed_tasks: Vec::new(),
        }
    }

    pub fn add_completed(&mut self, name: impl Into<String>, output: impl Into<String>) {
        self.completed_tasks.push((name.into(), output.into()));
    }

    pub fn add_failed(&mut self, name: impl Into<String>) {
        self.failed_tasks.push(name.into());
    }
}

/// Result of applying a correction
#[derive(Debug, Clone)]
pub enum CorrectionResult {
    /// Task will be retried with same input
    Retry,
    /// Task was modified and will be retried
    Modified,
    /// New prerequisite tasks were added
    AddedPrerequisites(Vec<TaskId>),
    /// Task was skipped
    Skipped,
    /// Error is unrecoverable
    Unrecoverable,
}

/// Errors during correction
#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Failed to parse analysis: {0}")]
    Parse(String),

    #[error("Retry state not found for task {0}")]
    StateNotFound(TaskId),

    #[error("Task {0} not found in job")]
    TaskNotFound(TaskId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correction_result_display() {
        let result = CorrectionResult::Retry;
        assert!(matches!(result, CorrectionResult::Retry));
    }
}
