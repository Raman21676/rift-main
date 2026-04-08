# Rift Phase 3: Remote Control Implementation

This document describes the Phase 3 implementation of Rift Remote Control, allowing users to monitor and control the Rift daemon from an Android app.

## Architecture

### Server Side (Rust)

New module: `rift-core/src/server/`

| File | Purpose |
|------|---------|
| `mod.rs` | RemoteServer struct, UPnP port mapping |
| `auth.rs` | Token generation and validation |
| `rest_api.rs` | Axum REST API endpoints |
| `websocket.rs` | WebSocket streaming for real-time logs |

### Android Side (Kotlin)

Location: `android/rift-remote/`

| File | Purpose |
|------|---------|
| `MainActivity.kt` | Main activity with navigation |
| `network/RiftWebSocketClient.kt` | WebSocket client |
| `network/RiftApiClient.kt` | REST API client |
| `model/` | Data models (DaemonStatus, TaskEvent, etc.) |
| `ui/` | Compose UI screens |

## Usage

### Start Rift with Remote Control

```bash
# Start daemon with remote API on port 7788
rift daemon start --remote --port 7788

# Or in foreground for testing
rift daemon start --foreground --remote --port 7788
```

The daemon will print:
- A 32-character token
- A QR code containing connection info
- The local and (if UPnP succeeds) public IP addresses

### Test with curl

```bash
TOKEN=YOUR32CHARTOKEN
PORT=7788

# Health check
curl http://localhost:$PORT/health

# Get status
curl "http://localhost:$PORT/api/status?token=$TOKEN"

# Submit task
curl -X POST "http://localhost:$PORT/api/tasks?token=$TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"goal":"List files in current directory"}'

# Get queue
curl "http://localhost:$PORT/api/queue?token=$TOKEN"

# Get history
curl "http://localhost:$PORT/api/history?token=$TOKEN"
```

### Android App

1. Open Android Studio
2. Open the `android/rift-remote/` folder
3. Build and run on device
4. Scan the QR code from the daemon terminal
5. Or enter connection details manually

## API Endpoints

### REST Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/status` | GET | Daemon status |
| `/api/queue` | GET | Pending tasks |
| `/api/history` | GET | Completed tasks |
| `/api/tasks` | POST | Submit new task |
| `/api/tasks/:id/cancel` | POST | Cancel a task |

### WebSocket Endpoint

| Endpoint | Description |
|----------|-------------|
| `/ws` | Real-time log streaming and status updates |

WebSocket message format:
- Byte 0: Message type (0x01=status, 0x02=task_event, 0x03=command, 0x04=ping, 0x05=pong)
- Bytes 1+: JSON payload

## Security

- **Token-based auth**: 32-character alphanumeric token
- **Constant-time comparison**: Prevents timing attacks
- **No special characters**: URL-safe tokens (A-Z, 2-9)
- **UPnP optional**: Falls back to local network if router doesn't support UPnP

## Features

### Dashboard
- Connection status indicator
- Daemon statistics (uptime, tasks completed/failed)
- Current task display
- Quick actions (Submit Task, View Queue, Live Logs)

### Submit Task
- Text input for goal description
- Auto-correct option (default: true)
- Submit confirmation

### Task Queue
- Tabbed interface: Pending / History
- Task cards with status badges
- Cancel button for pending tasks

### Live Logs
- Real-time log streaming via WebSocket
- Color-coded log levels (ERROR=red, WARN=yellow, INFO=blue, SUCCESS=green)
- Auto-scroll to latest
- Line count indicator

## Future Enhancements

- [ ] QR code scanning (CameraX + ML Kit)
- [ ] Push notifications for task completion
- [ ] Terminal/SSH mode for direct command execution
- [ ] Task approval workflow for destructive operations
- [ ] Multiple daemon connections
- [ ] Dark/light theme toggle

## Troubleshooting

### "Connection refused"
- Check that the daemon is running with `--remote`
- Verify the port is correct
- Check firewall settings

### "Unauthorized"
- Verify the token matches exactly
- Tokens are 32 characters, alphanumeric only

### UPnP not working
- Some routers don't support UPnP
- Use the local IP when on the same network
- Consider manual port forwarding for remote access

### Android app won't connect
- Ensure `android:usesCleartextTraffic="true"` in manifest
- Check that host IP is reachable from the phone
- Try the demo connection button to verify UI works
