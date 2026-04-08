# Rift User Guide

Complete guide for using Rift - the AI coding assistant with autonomous capabilities.

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [CLI Commands](#cli-commands)
4. [Daemon Mode](#daemon-mode)
5. [Autonomous Mode](#autonomous-mode)
6. [Configuration](#configuration)
7. [Tips & Best Practices](#tips--best-practices)

---

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- API key from OpenRouter or compatible provider

### Build from Source

```bash
cd rust
cargo build --release
```

The binary will be at `target/release/rift`. Copy it to your PATH:

```bash
cp target/release/rift ~/.local/bin/
```

---

## Quick Start

### 1. Set Your API Key

```bash
export OPENAI_API_KEY="your-api-key"
```

Or add to your shell profile (`~/.zshrc`, `~/.bashrc`):

```bash
echo 'export OPENAI_API_KEY="your-api-key"' >> ~/.zshrc
```

### 2. Run Your First Command

```bash
# Interactive chat mode
rift chat

# Or execute a single task
rift run "Explain the current directory structure"

# Or use autonomous mode
rift do "Create a Python script that calculates fibonacci numbers"
```

---

## CLI Commands

### Core Commands

| Command | Description | Example |
|---------|-------------|---------|
| `rift chat` | Start interactive chat session | `rift chat --message "Hello"` |
| `rift run` | Execute a single command | `rift run "List all Rust files"` |
| `rift do` | Autonomous task execution | `rift do "Refactor the auth module"` |
| `rift tools` | List available tools | `rift tools` |
| `rift config` | Show configuration | `rift config` |

### Chat Mode Commands

When in interactive chat mode, you can use:

```
/help          - Show available commands
/tool <name>   - Execute a specific tool
/clear         - Clear conversation history
/exit          - Exit chat mode
```

---

## Daemon Mode

Daemon mode allows Rift to run continuously in the background, processing tasks from a queue. This enables:

- **24/7 operation**: Tasks execute even when you're away
- **Task queue**: Submit multiple tasks to be processed sequentially
- **Background processing**: Long-running tasks don't block your terminal

### Starting the Daemon

```bash
# Start daemon in background (detached)
rift daemon start

# Start daemon in foreground (for debugging)
rift daemon start --foreground
```

Output:
```
🚀 Starting Rift daemon...
✅ Daemon started successfully!
Socket: /Users/<user>/Library/Caches/rift/daemon.sock
```

### Daemon Commands

| Command | Description | Example |
|---------|-------------|---------|
| `daemon start` | Start the background daemon | `rift daemon start` |
| `daemon stop` | Stop the daemon | `rift daemon stop` |
| `daemon status` | Check daemon status | `rift daemon status` |
| `daemon submit` | Submit a task to queue | `rift daemon submit "Fix bug #123"` |
| `daemon queue` | List pending tasks | `rift daemon queue` |
| `daemon history` | Show completed tasks | `rift daemon history` |
| `daemon cancel` | Cancel a pending task | `rift daemon cancel <task-id>` |

### Example Workflow

```bash
# 1. Start the daemon
rift daemon start

# 2. Submit multiple tasks
rift daemon submit "Review pull request #42"
rift daemon submit "Update dependencies"
rift daemon submit "Run test suite"

# 3. Check status
rift daemon status
# Output:
# 📊 Daemon Status
#    Running: ✅ Yes
#    Uptime: 15 minutes
#    Tasks completed: 2
#    Tasks failed: 0

# 4. View pending queue
rift daemon queue
# Output:
# 📋 Pending Tasks:
#    ⏳ a1b2c3d4 - Run test suite (pending)

# 5. View completed tasks
rift daemon history
# Output:
# 📜 Recent Tasks:
#    ✅ e5f6g7h8 - Update dependencies (completed)
#    ✅ i9j0k1l2 - Review pull request #42 (completed)

# 6. Stop daemon when done
rift daemon stop
```

### Daemon Data Locations

| File | Location | Description |
|------|----------|-------------|
| Socket | `~/Library/Caches/rift/daemon.sock` | Unix socket for CLI communication |
| Database | `~/Library/Application Support/rift/daemon.db` | SQLite task queue |
| Config | `~/.config/rift/config.toml` | User configuration |

---

## Autonomous Mode

Autonomous mode (`rift do`) enables Rift to work independently with minimal supervision.

### Features

- **Context gathering**: Automatically analyzes project structure
- **Self-correction**: Retries failed tasks with modified approaches
- **Verification**: Validates task outputs before marking complete
- **Error recovery**: Handles failures gracefully

### Usage Levels

#### Level 1: Basic Autonomy
```bash
rift do "Refactor the authentication module"
```
- Plans and executes tasks
- Shows progress in real-time

#### Level 2: With Self-Correction
```bash
rift do --self-correct "Fix the failing tests"
```
- Automatically retries failed attempts
- Modifies approach based on errors

#### Level 3: With Verification
```bash
rift do --verify "Update API endpoints"
```
- Verifies outputs after completion
- Checks file existence, syntax, tests

#### Level 4: Full Autonomy
```bash
rift do --auto "Implement user dashboard"
```
- Combines all features: context + self-correction + verification
- Recommended for complex tasks

### Combining with Daemon

Submit autonomous tasks to the daemon queue:

```bash
# Submit an autonomous task for background processing
rift daemon submit "--auto Migrate database schema"
```

---

## Configuration

Rift uses a TOML configuration file at `~/.config/rift/config.toml`.

### Example Configuration

```toml
# API Configuration
[llm]
api_key = "sk-or-..."
base_url = "https://openrouter.ai/api/v1"
model = "qwen/qwen-2.5-coder-32b-instruct"
temperature = 0.7
max_tokens = 4096

# Autonomous Mode Settings
[autonomous]
max_iterations = 10
enable_self_correction = true
enable_verification = true
auto_confirm = false  # Set true to skip confirmations

# Context Settings
[context]
max_files_to_read = 20
enable_semantic_search = true

# Daemon Settings
[daemon]
poll_interval_seconds = 5
socket_path = "/Users/<user>/Library/Caches/rift/daemon.sock"
```

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `OPENAI_API_KEY` | API key for LLM provider | `sk-or-...` |
| `RIFT_MODEL` | Model to use | `qwen/qwen-2.5-coder-32b-instruct` |
| `RIFT_BASE_URL` | API base URL | `https://openrouter.ai/api/v1` |

---

## Tips & Best Practices

### Writing Effective Task Descriptions

✅ **Good examples:**
```bash
rift do "Add error handling to the login function in auth.rs"
rift do "Create a README.md with project description and usage instructions"
rift do "Refactor the user service to use dependency injection"
```

❌ **Avoid vague descriptions:**
```bash
rift do "Fix the code"           # Too vague
rift do "Make it better"         # Not actionable
rift do "Update files"           # No specific goal
```

### Session Management

Use named sessions to organize different projects:

```bash
# Work on project A
rift chat --session project-a

# Switch to project B
rift chat --session project-b
```

Sessions persist conversation history between restarts.

### Daemon Best Practices

1. **Start daemon at login**: Add `rift daemon start` to your shell profile
2. **Monitor with status**: Check `rift daemon status` periodically
3. **Review history**: Use `rift daemon history` to see completed work
4. **Clean up failed tasks**: Cancel stuck tasks with `rift daemon cancel`

### Safety Features

Rift includes several safety mechanisms:

- **Capability system**: Tools declare required permissions
- **Confirmation prompts**: Destructive actions require approval
- **Dry-run mode**: Preview changes before applying (when available)
- **Task verification**: Validates outputs before marking complete

### Troubleshooting

| Issue | Solution |
|-------|----------|
| "Daemon is not running" | Run `rift daemon start` |
| "API key required" | Set `OPENAI_API_KEY` environment variable |
| Task fails repeatedly | Use `--self-correct` flag or check logs |
| Socket permission denied | Check `~/.config/rift/` ownership |

---

## Advanced Usage

### Custom Tool Development

Extend Rift with custom tools by implementing the `Tool` trait:

```rust
use rift_core::{Tool, ToolOutput, ToolError, Capability};
use serde_json::Value;

pub struct MyCustomTool;

#[async_trait]
impl Tool for MyCustomTool {
    fn name(&self) -> &str { "MyTool" }
    fn description(&self) -> &str { "Does something useful" }
    fn parameters(&self) -> Value { /* JSON Schema */ }
    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::FileRead]
    }
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        // Implementation
    }
}
```

### Integration with CI/CD

Use Rift in your CI/CD pipelines:

```yaml
# GitHub Actions example
- name: Run Rift Tasks
  env:
    OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
  run: |
    rift daemon start
    rift daemon submit "Run code review"
    rift daemon submit "Check for security issues"
```

---

## Getting Help

- **Documentation**: See [ARCHITECTURE.md](ARCHITECTURE.md) for technical details
- **Status**: Check [STATUS.md](STATUS.md) for current development status
- **Issues**: Report bugs and feature requests on GitHub

---

Last Updated: 2026-04-08
