//! Goal planner - converts natural language goals into executable Task DAGs
//!
//! Uses a simple line-based format that is easier for weaker LLMs to generate
//! correctly than nested JSON.

use crate::context::ProjectContext;
use crate::llm::{LlmClient, Message};
use crate::task::{Job, Task, TaskId};
use serde_json::Value;
use std::collections::HashMap;

/// Converts goals into Jobs
pub struct Planner {
    llm_client: LlmClient,
    available_tools: Vec<String>,
}

impl Planner {
    /// Create a new planner
    pub fn new(llm_client: LlmClient, available_tools: Vec<String>) -> Self {
        Self {
            llm_client,
            available_tools,
        }
    }

    /// Create a planning prompt using a simple line-based format
    fn build_prompt(&self, goal: &str) -> String {
        let tools_list = self.available_tools.join(", ");
        format!(
            "You are a task planner. Break the goal into small executable tasks.\n\
            Available tools: {}\n\n\
            Rules:\n\
            1. One tool per task\n\
            2. Task names should be short and use only letters, numbers, and underscores\n\
            3. Include complete file content when using write_file\n\
            4. Use empty dependencies if the task has no prerequisites\n\
            5. Each task MUST be on its own line starting with TASK:\n\n\
            Format for each task (exactly one per line):\n\
            TASK: name | tool | key1=value1;key2=value2 | dependency1,dependency2\n\n\
            Example 1:\n\
            TASK: create_hello | write_file | path=hello.txt;content=hello world | \n\
            TASK: show_file | read_file | path=hello.txt | create_hello\n\n\
            Example 2:\n\
            TASK: make_dir | bash | command=mkdir -p games | \n\
            TASK: create_index | write_file | path=games/index.html;content=<html></html> | make_dir\n\n\
            Example 3:\n\
            TASK: search_rust | web_search | query=rust async tutorial | \n\
            TASK: fetch_page | web_fetch | url=https://example.com | search_rust\n\n\
            Important:\n\
            - The | symbols are STRUCTURAL DELIMITERS, not part of the values\n\
            - Separate parameters with semicolons (;)\n\
            - Do NOT use JSON, just key=value pairs\n\
            - For multi-line content, use \\n for newlines in write_file content\n\
            - For bash commands: keep them simple, no unclosed quotes or parentheses\n\
            - If a value contains a semicolon, use bash instead\n\n\
            Goal: {}\n",
            tools_list, goal
        )
    }
    
    /// Create a planning prompt that includes project context
    fn build_prompt_with_context(&self, goal: &str, context: &ProjectContext) -> String {
        use crate::context::ContextGatherer;
        
        let tools_list = self.available_tools.join(", ");
        let context_str = ContextGatherer::format_for_prompt(context);
        
        format!(
            "You are a task planner. Break the goal into small executable tasks.\n\
            Available tools: {}\n\n\
            {}\n\n\
            Rules:\n\
            1. One tool per task\n\
            2. Task names should be short and use only letters, numbers, and underscores\n\
            3. Include complete file content when using write_file\n\
            4. Use empty dependencies if the task has no prerequisites\n\
            5. Each task MUST be on its own line starting with TASK:\n\
            6. Check if files already exist before creating them (see context above)\n\
            7. Use existing project structure when possible\n\
            8. For existing files, prefer edit_file over write_file to avoid overwriting\n\n\
            Format for each task (exactly one per line):\n\
            TASK: name | tool | key1=value1;key2=value2 | dependency1,dependency2\n\n\
            Example 1:\n\
            TASK: create_hello | write_file | path=hello.txt;content=hello world | \n\
            TASK: show_file | read_file | path=hello.txt | create_hello\n\n\
            Example 2:\n\
            TASK: make_dir | bash | command=mkdir -p games | \n\
            TASK: create_index | write_file | path=games/index.html;content=<html></html> | make_dir\n\n\
            Example 3:\n\
            TASK: search_rust | web_search | query=rust async tutorial | \n\
            TASK: fetch_page | web_fetch | url=https://example.com | search_rust\n\n\
            Important:\n\
            - The | symbols are STRUCTURAL DELIMITERS, not part of the values\n\
            - Separate parameters with semicolons (;)\n\
            - Do NOT use JSON, just key=value pairs\n\
            - For multi-line content, use \\n for newlines in write_file content\n\
            - For bash commands: keep them simple, no unclosed quotes or parentheses\n\
            - If a value contains a semicolon, use bash instead\n\
            - DO NOT recreate files that already exist (check the context above)\n\
            - Use edit_file to modify existing files, write_file only for NEW files\n\n\
            Goal: {}",
            tools_list, context_str, goal
        )
    }

    /// Plan a goal into a Job
    pub async fn plan(&self, goal: &str) -> Result<Job, PlannerError> {
        let prompt = self.build_prompt(goal);
        self.execute_plan(goal, prompt).await
    }
    
    /// Plan a goal with project context for better results
    pub async fn plan_with_context(&self, goal: &str, context: &ProjectContext) -> Result<Job, PlannerError> {
        let prompt = self.build_prompt_with_context(goal, context);
        self.execute_plan(goal, prompt).await
    }
    
    /// Execute the planning with a given prompt
    async fn execute_plan(&self, goal: &str, prompt: String) -> Result<Job, PlannerError> {
        let response = self.llm_client.chat(vec![Message::user(prompt)]).await
            .map_err(|e| PlannerError::Llm(e.to_string()))?;

        let tasks = Self::parse_tasks(&response)?;

        if tasks.is_empty() {
            return Err(PlannerError::EmptyPlan);
        }

        let mut job = Job::new(goal);
        let mut id_map: HashMap<String, TaskId> = HashMap::new();

        // First pass: create all tasks
        for (name, tool, input, _) in &tasks {
            let task = Task::new(name.clone(), tool.clone(), input.clone());
            let id = task.id;
            id_map.insert(name.clone(), id);
            job.add_task(task);
        }

        // Second pass: resolve dependencies
        for (name, _, _, deps) in &tasks {
            let task_id = *id_map.get(name)
                .ok_or_else(|| PlannerError::Parse(format!("Missing task: {}", name)))?;

            let mut dep_ids = Vec::new();
            for dep_name in deps {
                if dep_name.is_empty() {
                    continue;
                }
                if let Some(dep_id) = id_map.get(dep_name) {
                    dep_ids.push(*dep_id);
                } else {
                    return Err(PlannerError::Parse(
                        format!("Task '{}' depends on unknown task '{}'", name, dep_name)
                    ));
                }
            }

            if let Some(task) = job.tasks.get_mut(&task_id) {
                task.dependencies = dep_ids;
            }
        }

        Ok(job)
    }

    /// Parse the line-based task format from LLM response
    fn parse_tasks(response: &str) -> Result<Vec<(String, String, Value, Vec<String>)>, PlannerError> {
        let mut tasks = Vec::new();

        for line in response.lines() {
            let line = line.trim();
            if !line.starts_with("TASK:") {
                continue;
            }

            let content = line[5..].trim();
            // Split by | and trim each part. This handles both "a | b | c" and "a|b|c"
            let parts: Vec<&str> = content.split('|').map(|s| s.trim()).collect();

            if parts.len() < 3 {
                return Err(PlannerError::Parse(format!("Invalid task line: {}", line)));
            }

            let name = parts[0].to_string();
            let tool = parts[1].to_string();
            let input_str = parts[2];

            let deps = if parts.len() >= 4 {
                parts[3]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                Vec::new()
            };

            let input = Self::parse_input(input_str)
                .map_err(|e| PlannerError::Parse(format!(
                    "Invalid input in task '{}': {} (input: {})",
                    name, e, input_str
                )))?;

            tasks.push((name, tool, input, deps));
        }

        Ok(tasks)
    }

    /// Parse key=value;key2=value2 into a JSON object
    fn parse_input(input: &str) -> Result<Value, String> {
        if input.is_empty() {
            return Ok(Value::Object(serde_json::Map::new()));
        }

        let mut map = serde_json::Map::new();

        for pair in input.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }

            let eq_pos = pair.find('=')
                .ok_or_else(|| format!("Missing '=' in pair: {}", pair))?;

            let key = pair[..eq_pos].trim().to_string();
            let value = pair[eq_pos + 1..].trim().to_string();

            // Convert value to appropriate JSON type
            let json_value = if value == "true" {
                Value::Bool(true)
            } else if value == "false" {
                Value::Bool(false)
            } else if let Ok(n) = value.parse::<i64>() {
                Value::Number(serde_json::Number::from(n))
            } else {
                // Replace escaped newlines with actual newlines
                let unescaped = value.replace("\\n", "\n");
                Value::String(unescaped)
            };

            map.insert(key, json_value);
        }

        Ok(Value::Object(map))
    }
}

/// Planner errors
#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Empty plan returned")]
    EmptyPlan,
}
