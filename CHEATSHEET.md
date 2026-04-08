# Rift Command Cheatsheet

Quick reference for Rift CLI commands.

---

## Core Commands

```bash
# Interactive chat
rift chat
rift chat --message "Hello"

# Execute single command
rift run "Explain this codebase"

# Autonomous execution
rift do "Create a Python script"
rift do --self-correct "Fix tests"    # With retry
rift do --verify "Update docs"        # With validation
rift do --auto "Implement feature"    # Full autonomy

# List available tools
rift tools

# Show configuration
rift config
```

---

## Daemon Commands

```bash
# Start daemon
rift daemon start
rift daemon start --foreground    # Debug mode

# Stop daemon
rift daemon stop

# Check status
rift daemon status

# Submit tasks
rift daemon submit "Task description"

# View tasks
rift daemon queue      # Pending tasks
rift daemon history    # Completed tasks

# Cancel task
rift daemon cancel <task-id>
```

---

## Environment Variables

```bash
export OPENAI_API_KEY="sk-or-..."
export RIFT_MODEL="qwen/qwen-2.5-coder-32b-instruct"
export RIFT_BASE_URL="https://openrouter.ai/api/v1"
```

---

## Configuration File

Location: `~/.config/rift/config.toml`

```toml
[llm]
api_key = "sk-or-..."
model = "qwen/qwen-2.5-coder-32b-instruct"
base_url = "https://openrouter.ai/api/v1"

[autonomous]
max_iterations = 10
enable_self_correction = true
enable_verification = true
```

---

## Session Management

```bash
# Named sessions
rift chat --session project-a
rift chat --session project-b

# Sessions persist at: ~/.config/rift/sessions.db
```

---

## Daemon Data Locations

| Type | Path |
|------|------|
| Socket | `~/Library/Caches/rift/daemon.sock` (macOS) |
| Database | `~/Library/Application Support/rift/daemon.db` (macOS) |
| Config | `~/.config/rift/config.toml` |
| Sessions | `~/.config/rift/sessions.db` |

**Linux:** Replace `~/Library/Caches` with `~/.cache` and `~/Library/Application Support` with `~/.local/share`.

---

## Common Patterns

### Background Development Workflow

```bash
# 1. Start daemon
rift daemon start

# 2. Queue multiple tasks
rift daemon submit "Review PR #42"
rift daemon submit "Update dependencies"
rift daemon submit "Run tests"

# 3. Check progress
rift daemon status
rift daemon queue

# 4. View results
rift daemon history

# 5. Stop when done
rift daemon stop
```

### Quick Task Execution

```bash
# One-off autonomous task
rift do --auto "Refactor auth module"

# With specific flags
rift do --self-correct --verify "Fix build errors"
```

### Interactive Development

```bash
# Start chat with context
rift chat --session myproject

# In chat:
# /help        - Show commands
# /tool <name> - Execute tool
# /clear       - Clear history
# /exit        - Exit
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Daemon not running" | `rift daemon start` |
| "API key required" | `export OPENAI_API_KEY=...` |
| Task keeps failing | Use `--self-correct` flag |
| Socket error | `rm ~/Library/Caches/rift/daemon.sock` |
| Stuck task | `rift daemon cancel <id>` |

---

## Flags Reference

### Global Flags
| Flag | Description |
|------|-------------|
| `--api-key` | API key override |
| `--model` | Model override |
| `--base-url` | API base URL override |
| `--session` | Session name |

### `do` Command Flags
| Flag | Description |
|------|-------------|
| `--self-correct` | Enable retry on failure |
| `--verify` | Enable output verification |
| `--auto` | Full autonomy (all features) |

### `daemon start` Flags
| Flag | Description |
|------|-------------|
| `--foreground` | Run in current terminal |

---

## Keyboard Shortcuts (Chat Mode)

| Shortcut | Action |
|----------|--------|
| `Ctrl+C` | Cancel current operation |
| `Ctrl+D` | Exit chat |
| `↑/↓` | Navigate history |
| `Tab` | Autocomplete |
