//! Self-correcting task orchestrator
//!
//! Wraps the standard TaskOrchestrator with self-correction capabilities.
//! When tasks fail, it analyzes the failure and attempts automatic recovery.

use crate::llm::LlmClient;
use crate::self_correct::{CorrectionResult, JobContext, SelfCorrector};
use crate::task::{Job, JobResult, TaskError, TaskExecutor, TaskId, TaskResult, TaskStatus};
use std::collections::HashSet;
use tracing::{error, info, warn};

/// Orchestrator with self-correction capabilities
pub struct SelfCorrectingOrchestrator {
    max_concurrent: usize,
    corrector: Option<SelfCorrector>,
    max_corrections: u32,
    available_tools: Vec<String>,
}

impl SelfCorrectingOrchestrator {
    /// Create a new self-correcting orchestrator
    pub fn new() -> Self {
        Self {
            max_concurrent: 4,
            corrector: None,
            max_corrections: 10,
            available_tools: Vec::new(),
        }
    }

    /// Enable self-correction with an LLM client
    pub fn with_self_correction(mut self, llm_client: LlmClient) -> Self {
        self.corrector = Some(SelfCorrector::new(llm_client));
        self
    }
    
    /// Set available tools for correction suggestions
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.available_tools = tools.clone();
        if let Some(ref mut corrector) = self.corrector {
            corrector.available_tools = tools;
        }
        self
    }

    /// Set maximum number of correction cycles
    pub fn with_max_corrections(mut self, max: u32) -> Self {
        self.max_corrections = max;
        self
    }

    /// Set max concurrent tasks
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Run a job with self-correction enabled
    pub async fn run<E>(
        &mut self,
        job: &mut Job,
        executor: &E,
    ) -> Result<JobResult, TaskError>
    where
        E: TaskExecutor,
    {
        let mut correction_count = 0;
        
        // Initialize retry states for all tasks if corrector exists
        if let Some(ref mut corrector) = self.corrector {
            for task in job.tasks.values() {
                corrector.init_task(task);
            }
        }

        loop {
            // Run the job (or continue from where we left off)
            let intermediate_result = self.run_batch(job, executor).await?;

            // Check if all tasks completed successfully
            let all_completed = job.tasks.values().all(|t| {
                t.status == TaskStatus::Completed
            });

            if all_completed {
                info!("All tasks completed successfully after {} corrections", correction_count);
                return Ok(intermediate_result);
            }

            // If no self-correction, return the result as-is
            if self.corrector.is_none() {
                return Ok(intermediate_result);
            }

            // Find failed tasks and attempt correction
            let failed_tasks: Vec<(TaskId, TaskResult)> = job.tasks.iter()
                .filter(|(_, t)| t.status == TaskStatus::Failed)
                .filter_map(|(id, t)| {
                    t.result.as_ref().map(|r| (*id, r.clone()))
                })
                .collect();

            if failed_tasks.is_empty() {
                // No failed tasks but not all completed (some might be pending due to deps)
                warn!("No failed tasks but job incomplete - possible dependency issue");
                return Ok(intermediate_result);
            }

            if correction_count >= self.max_corrections {
                warn!("Exceeded maximum correction cycles ({}), giving up", self.max_corrections);
                return Ok(intermediate_result);
            }

            // Try to correct the first failed task
            let (failed_id, failed_result) = &failed_tasks[0];
            let failed_task = job.tasks.get(failed_id).unwrap().clone();

            info!("Attempting to correct failed task '{}' (correction {})", 
                failed_task.name, correction_count + 1);

            // Build job context for analysis
            let job_context = self.build_job_context(job);
            
            // Get mutable reference to corrector for the correction phase
            let corrector = self.corrector.as_mut().unwrap();

            // Analyze the failure
            match corrector.analyze_failure(&failed_task, failed_result, &job_context).await {
                Ok(analysis) => {
                    if !analysis.recoverable {
                        info!("Task '{}' failure is unrecoverable: {}", 
                            failed_task.name, analysis.reason);
                        return Ok(intermediate_result);
                    }

                    // Apply the correction
                    match corrector.apply_correction(job, *failed_id, &analysis) {
                        Ok(correction_result) => {
                            correction_count += 1;
                            
                            match &correction_result {
                                CorrectionResult::Unrecoverable => {
                                    info!("Correction determined task is unrecoverable");
                                    return Ok(intermediate_result);
                                }
                                CorrectionResult::Skipped => {
                                    info!("Task skipped as optional");
                                }
                                _ => {
                                    info!("Applied correction: {:?}", correction_result);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to apply correction: {}", e);
                            return Ok(intermediate_result);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to analyze failure: {}", e);
                    return Ok(intermediate_result);
                }
            }
        }
    }

    /// Run a batch of tasks until no more can run
    async fn run_batch<E>(
        &self,
        job: &mut Job,
        executor: &E,
    ) -> Result<JobResult, TaskError>
    where
        E: TaskExecutor,
    {
        let order = job.execution_order()?;
        let mut succeeded = HashSet::<TaskId>::new();
        let mut executed = HashSet::<TaskId>::new();

        // Initialize succeeded set from already completed tasks
        for (id, task) in &job.tasks {
            if task.status == TaskStatus::Completed {
                succeeded.insert(*id);
            }
        }

        loop {
            let batch = self.next_batch(&order, job, &succeeded);

            if batch.is_empty() {
                break;
            }

            // Mark tasks as running
            for id in &batch {
                let task = job.tasks.get_mut(id).unwrap();
                if task.status == TaskStatus::Pending {
                    task.status = TaskStatus::Running;
                }
            }

            let batch_ids: Vec<_> = batch.clone();

            let futures: Vec<_> = batch
                .into_iter()
                .map(|id| {
                    let task = job.tasks.get(&id).unwrap();
                    executor.execute(task)
                })
                .collect();

            for (idx, result) in futures.into_iter().enumerate() {
                let id = batch_ids[idx];
                executed.insert(id);
                
                match result.await {
                    Ok(result) => {
                        let task = job.tasks.get_mut(&id).unwrap();
                        task.status = if result.success {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                        if result.success {
                            succeeded.insert(id);
                        }
                        task.result = Some(result);
                    }
                    Err(e) => {
                        let task = job.tasks.get_mut(&id).unwrap();
                        task.status = TaskStatus::Failed;
                        task.result = Some(TaskResult {
                            success: false,
                            output: format!("Error: {}", e),
                            data: None,
                            execution_time_ms: 0,
                        });
                        error!("Task {} failed: {}", id, e);
                    }
                }
            }
        }

        // Mark remaining unexecuted tasks
        for id in &order {
            if !executed.contains(id) {
                let task = job.tasks.get_mut(id).unwrap();
                if task.status == TaskStatus::Pending {
                    task.status = TaskStatus::Failed;
                    task.result = Some(TaskResult {
                        success: false,
                        output: "Skipped: dependency failed".to_string(),
                        data: None,
                        execution_time_ms: 0,
                    });
                }
            }
        }

        let all_succeeded = job.tasks.values().all(|t| {
            t.result.as_ref().map(|r| r.success).unwrap_or(false)
        });

        Ok(JobResult {
            success: all_succeeded,
            job_id: job.id,
        })
    }

    /// Get the next batch of tasks that can run
    fn next_batch(&self, order: &[TaskId], job: &Job, succeeded: &HashSet<TaskId>) -> Vec<TaskId> {
        let mut batch = Vec::new();

        for id in order {
            if batch.len() >= self.max_concurrent {
                break;
            }

            let task = job.tasks.get(id).unwrap();

            // Only consider pending tasks
            if task.status != TaskStatus::Pending {
                continue;
            }

            // Skip tasks with failed dependencies
            let has_failed_dep = task.dependencies.iter().any(|d| {
                job.tasks.get(d).map(|t| {
                    t.result.as_ref().map(|r| !r.success).unwrap_or(false)
                }).unwrap_or(false)
            });

            if has_failed_dep {
                continue;
            }

            // Check if all dependencies are satisfied
            let deps_satisfied = task.dependencies.iter().all(|d| succeeded.contains(d));

            if deps_satisfied {
                batch.push(*id);
            }
        }

        batch
    }

    /// Build context about the job for failure analysis
    fn build_job_context(&self, job: &Job) -> JobContext {
        let mut context = JobContext::new(&job.name);
        
        // Add available tools from corrector if present
        if let Some(ref corrector) = self.corrector {
            context.available_tools = corrector.available_tools.clone();
        } else {
            context.available_tools = self.available_tools.clone();
        }

        for (_, task) in &job.tasks {
            if let Some(ref result) = task.result {
                if result.success {
                    // Truncate output for context
                    let summary = if result.output.len() > 100 {
                        format!("{}...", &result.output[..100])
                    } else {
                        result.output.clone()
                    };
                    context.add_completed(&task.name, summary);
                } else {
                    context.add_failed(&task.name);
                }
            }
        }

        context
    }
}

impl Default for SelfCorrectingOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}
