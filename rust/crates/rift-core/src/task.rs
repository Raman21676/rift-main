//! Task-based execution system with DAG scheduling
//!
//! Jobs are decomposed into tasks with dependencies, allowing
//! parallel execution and proper ordering.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Trait for task executors
pub trait TaskExecutor: Send + Sync {
    /// Execute a task and return a boxed future
    fn execute(&self, task: &Task) -> Pin<Box<dyn Future<Output = Result<TaskResult, TaskError>> + Send>>;
}

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub uuid::Uuid);

impl TaskId {
    /// Generate a new random task ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A task represents a unit of work
#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub dependencies: Vec<TaskId>,
    pub status: TaskStatus,
    pub result: Option<TaskResult>,
}

impl Task {
    /// Create a new task
    pub fn new(
        name: impl Into<String>,
        tool_name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: String::new(),
            tool_name: tool_name.into(),
            input,
            dependencies: Vec::new(),
            status: TaskStatus::Pending,
            result: None,
        }
    }
    
    /// Add a dependency
    pub fn depends_on(mut self, task_id: TaskId) -> Self {
        self.dependencies.push(task_id);
        self
    }
    
    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// Result of task execution
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub success: bool,
    pub output: String,
    pub data: Option<serde_json::Value>,
    pub execution_time_ms: u64,
}

/// A job is a collection of tasks with dependencies
#[derive(Debug, Clone)]
pub struct Job {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub tasks: HashMap<TaskId, Task>,
}

impl Job {
    /// Create a new job
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            name: name.into(),
            description: String::new(),
            tasks: HashMap::new(),
        }
    }
    
    /// Add a task to the job
    pub fn add_task(&mut self, task: Task) -> TaskId {
        let id = task.id;
        self.tasks.insert(id, task);
        id
    }
    
    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
    
    /// Get tasks in dependency order (topological sort)
    pub fn execution_order(&self) -> Result<Vec<TaskId>, TaskError> {
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();
        let mut result = Vec::new();
        
        fn visit(
            id: TaskId,
            tasks: &HashMap<TaskId, Task>,
            visited: &mut HashSet<TaskId>,
            temp_mark: &mut HashSet<TaskId>,
            result: &mut Vec<TaskId>,
        ) -> Result<(), TaskError> {
            if temp_mark.contains(&id) {
                return Err(TaskError::CyclicDependency);
            }
            if visited.contains(&id) {
                return Ok(());
            }
            
            temp_mark.insert(id);
            
            if let Some(task) = tasks.get(&id) {
                for dep in &task.dependencies {
                    visit(*dep, tasks, visited, temp_mark, result)?;
                }
            }
            
            temp_mark.remove(&id);
            visited.insert(id);
            result.push(id);
            Ok(())
        }
        
        for id in self.tasks.keys() {
            visit(*id, &self.tasks, &mut visited, &mut temp_mark, &mut result)?;
        }
        
        Ok(result)
    }
}

/// Orchestrates task execution
pub struct TaskOrchestrator {
    max_concurrent: usize,
}

impl TaskOrchestrator {
    /// Create a new orchestrator
    pub fn new() -> Self {
        Self {
            max_concurrent: 4,
        }
    }
    
    /// Set maximum concurrent tasks
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }
    
    /// Run a job
    pub async fn run<E>(
        &self,
        job: &mut Job,
        executor: &E,
    ) -> Result<JobResult, TaskError>
    where
        E: TaskExecutor,
    {
        let order = job.execution_order()?;
        let completed = Arc::new(RwLock::new(HashSet::<TaskId>::new()));
        
        info!("Running job '{}' with {} tasks", job.name, order.len());
        
        for batch in self.batches(&order, job) {
            let batch_size = batch.len();
            debug!("Executing batch of {} tasks", batch_size);
            
            for id in &batch {
                let task = job.tasks.get_mut(id).unwrap();
                task.status = TaskStatus::Running;
            }
            
            // Clone batch IDs before consuming
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
                match result.await {
                    Ok(result) => {
                        let task = job.tasks.get_mut(&id).unwrap();
                        task.status = if result.success {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                        task.result = Some(result);
                        completed.write().await.insert(id);
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
        
        let all_succeeded = job.tasks.values().all(|t| {
            t.result.as_ref().map(|r| r.success).unwrap_or(false)
        });
        
        info!("Job '{}' completed, success={}", job.name, all_succeeded);
        
        Ok(JobResult {
            success: all_succeeded,
            job_id: job.id,
        })
    }
    
    /// Group tasks into batches that can run concurrently
    fn batches(&self, order: &[TaskId], job: &Job) -> Vec<Vec<TaskId>> {
        let mut batches = Vec::new();
        let mut completed = HashSet::new();
        let mut remaining: Vec<_> = order.to_vec();
        
        while !remaining.is_empty() {
            let mut batch = Vec::new();
            let mut still_pending = Vec::new();
            
            for id in remaining {
                if batch.len() >= self.max_concurrent {
                    still_pending.push(id);
                    continue;
                }
                
                let task = job.tasks.get(&id).unwrap();
                let deps_satisfied = task.dependencies.iter().all(|d| completed.contains(d));
                
                if deps_satisfied {
                    batch.push(id);
                } else {
                    still_pending.push(id);
                }
            }
            
            if batch.is_empty() && !still_pending.is_empty() {
                // This shouldn't happen if dependencies are valid
                warn!("Deadlock detected in task dependencies");
                break;
            }
            
            for id in &batch {
                completed.insert(*id);
            }
            
            if !batch.is_empty() {
                batches.push(batch);
            }
            remaining = still_pending;
        }
        
        batches
    }
}

impl Default for TaskOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of job execution
#[derive(Debug, Clone)]
pub struct JobResult {
    pub success: bool,
    pub job_id: TaskId,
}

/// Errors that can occur during task execution
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Cyclic dependency detected")]
    CyclicDependency,
    
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
    
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Tool error: {0}")]
    Tool(String),
}
