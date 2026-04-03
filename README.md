# Rift

An AI coding assistant with a plugin-based architecture and capability-based security model.

## Overview

Rift is a clean-room implementation of an AI coding assistant, designed with:

- **Plugin-based tools**: Extensible tool system using traits
- **Capability-based permissions**: Fine-grained security model
- **Task DAG execution**: Parallel task execution with dependency resolution
- **Streaming LLM support**: Real-time response processing

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

## Installation

```bash
cd rust
cargo build --release
```

## Usage

```bash
# Set your API key
export OPENAI_API_KEY="your-key"

# Interactive chat
rift chat

# Single command
rift run "Explain this codebase"

# List available tools
rift tools
```

## License

MIT License - See [LICENSE](LICENSE)
