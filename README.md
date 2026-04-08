# Rift

An AI coding assistant with a plugin-based architecture and autonomous capabilities.

## Overview

Rift is a clean-room implementation of an AI coding assistant, designed with:

- **Plugin-based tools**: Extensible tool system using traits
- **Capability-based permissions**: Fine-grained security model
- **Task DAG execution**: Parallel task execution with dependency resolution
- **Streaming LLM support**: Real-time response processing
- **Autonomous mode**: Self-correcting, verifying task execution
- **Daemon mode**: 24/7 background operation with task queue

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed design documentation.

### Key Differences from Claw Code

Rift is a from-scratch implementation with a fundamentally different architecture:

| Aspect | Rift | Claw Code |
|--------|------|-----------|
| Tool system | Plugin traits | Enum-based |
| Permissions | Capability tokens | Permission modes |
| Execution | Task DAG | Linear conversation |
| Context | Semantic search | Message history |
| Architecture | Async-first | Mixed sync/async |
| Autonomy | Self-correct + verify | Manual approval |
| Daemon mode | Background queue | Not available |

## Installation

```bash
cd rust
cargo build --release
```

The binary will be at `target/release/rift`. Install to your PATH:

```bash
cp target/release/rift ~/.local/bin/
```

## Quick Start

```bash
# Set your API key
export OPENAI_API_KEY="your-key"

# Interactive chat
rift chat

# Single command
rift run "Explain this codebase"

# Autonomous execution
rift do "Refactor the authentication module"

# Start background daemon
rift daemon start

# Submit task to daemon queue
rift daemon submit "Review pull request #42"
```

## Usage Guide

See [USER_GUIDE.md](USER_GUIDE.md) for comprehensive documentation including:
- All CLI commands and options
- Daemon mode operation
- Autonomous mode features
- Configuration options
- Tips and best practices

## Features

### Autonomous Mode (`rift do`)

Execute tasks with full autonomy:

```bash
# Basic autonomous execution
rift do "Create a Python script for data processing"

# With self-correction on failures
rift do --self-correct "Fix the failing tests"

# With output verification
rift do --verify "Update API documentation"

# Full autonomy (recommended)
rift do --auto "Implement user authentication"
```

### Daemon Mode (`rift daemon`)

Run Rift continuously in the background:

```bash
# Start daemon
rift daemon start

# Submit tasks to queue
rift daemon submit "Task 1"
rift daemon submit "Task 2"

# Check status
rift daemon status

# View queue and history
rift daemon queue
rift daemon history

# Stop daemon
rift daemon stop
```

## Project Structure

```
rust/
├── crates/
│   ├── rift-core/       # Core engine
│   │   ├── daemon/       # Background daemon
│   │   ├── agent.rs      # Autonomous agent
│   │   ├── self_correct/ # Self-correction system
│   │   ├── verify/       # Output verification
│   │   └── ...
│   ├── rift-tools/      # Built-in tools
│   └── rift-cli/        # CLI application
└── Cargo.toml
```

## License

MIT License - See [LICENSE](LICENSE)
