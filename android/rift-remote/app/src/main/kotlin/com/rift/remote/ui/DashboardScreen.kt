package com.rift.remote.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.rift.remote.model.DaemonStatus
import com.rift.remote.network.RiftWebSocketClient
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive

@Composable
fun DashboardScreen(
    wsClient: RiftWebSocketClient,
    apiStatus: DaemonStatus?,
    onSubmitTask: () -> Unit,
    onViewQueue: () -> Unit,
    onViewLogs: () -> Unit
) {
    var uptime by remember { mutableStateOf("--") }
    var completed by remember { mutableIntStateOf(0) }
    var failed by remember { mutableIntStateOf(0) }
    var currentTask by remember { mutableStateOf<String?>(null) }
    var connected by remember { mutableStateOf(false) }
    var pending by remember { mutableIntStateOf(0) }
    var running by remember { mutableIntStateOf(0) }

    // Update from API status
    LaunchedEffect(apiStatus) {
        apiStatus?.let { status ->
            uptime = formatUptime(status.uptime_seconds)
            completed = status.tasks_completed.toInt()
            failed = status.tasks_failed.toInt()
            pending = status.queue.pending
            running = status.queue.running
            currentTask = status.current_task?.goal
            connected = status.running
        }
    }

    // Update from WebSocket status
    LaunchedEffect(wsClient) {
        // The WebSocket callbacks are set up in the ViewModel/MainActivity
        // This is just for the initial state
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Text(
            text = "Rift Remote",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold
        )

        // Connection indicator
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(12.dp),
            colors = CardDefaults.cardColors(
                containerColor = if (connected) Color(0xFF4CAF50) else Color(0xFFF44336)
            )
        ) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = if (connected) "●  Daemon Online" else "●  Disconnected",
                    color = Color.White,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Medium
                )
            }
        }

        // Stats
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(12.dp)
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Text(
                    text = "Statistics",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
                
                StatRow("Uptime", uptime)
                StatRow("Completed", completed.toString())
                StatRow("Failed", failed.toString())
                StatRow("Pending", pending.toString())
                StatRow("Running", running.toString())
                
                if (currentTask != null) {
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        text = "▶ Current Task:",
                        style = MaterialTheme.typography.bodyMedium,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.primary
                    )
                    Text(
                        text = currentTask!!,
                        style = MaterialTheme.typography.bodyMedium
                    )
                }
            }
        }

        Spacer(modifier = Modifier.weight(1f))

        // Actions
        Button(
            onClick = onSubmitTask,
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
            shape = RoundedCornerShape(12.dp)
        ) {
            Text("+ Submit New Task", fontSize = 16.sp)
        }
        
        Button(
            onClick = onViewQueue,
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = MaterialTheme.colorScheme.secondary
            )
        ) {
            Text("View Task Queue", fontSize = 16.sp)
        }
        
        Button(
            onClick = onViewLogs,
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = MaterialTheme.colorScheme.tertiary
            )
        ) {
            Text("Live Logs", fontSize = 16.sp)
        }
    }
}

@Composable
private fun StatRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyLarge,
            fontWeight = FontWeight.Medium
        )
    }
}

private fun formatUptime(seconds: Long): String {
    val hours = seconds / 3600
    val minutes = (seconds % 3600) / 60
    val secs = seconds % 60
    
    return when {
        hours > 0 -> "${hours}h ${minutes}m ${secs}s"
        minutes > 0 -> "${minutes}m ${secs}s"
        else -> "${secs}s"
    }
}
