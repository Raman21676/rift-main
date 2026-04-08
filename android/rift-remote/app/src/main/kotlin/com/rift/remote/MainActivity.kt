package com.rift.remote

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import com.rift.remote.model.ConnectionInfo
import com.rift.remote.model.DaemonStatus
import com.rift.remote.model.TaskEvent
import com.rift.remote.network.RiftApiClient
import com.rift.remote.network.RiftWebSocketClient
import com.rift.remote.ui.*
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

class MainActivity : ComponentActivity() {
    private val apiClient = RiftApiClient()
    private lateinit var wsClient: RiftWebSocketClient
    
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        
        // Initialize WebSocket client
        wsClient = RiftWebSocketClient(
            onStatus = { handleStatusUpdate(it) },
            onTaskEvent = { handleTaskEvent(it) },
            onConnected = { /* Handle connected */ },
            onDisconnected = { /* Handle disconnected */ }
        )
        
        setContent {
            RiftRemoteTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    RiftRemoteApp(
                        apiClient = apiClient,
                        wsClient = wsClient
                    )
                }
            }
        }
    }
    
    private fun handleStatusUpdate(status: JsonObject) {
        // Update UI with status
    }
    
    private fun handleTaskEvent(event: TaskEvent) {
        // Update UI with task event
    }
}

@Composable
fun RiftRemoteApp(
    apiClient: RiftApiClient,
    wsClient: RiftWebSocketClient
) {
    var currentScreen by remember { mutableStateOf(Screen.QRScan) }
    var connectionInfo by remember { mutableStateOf<ConnectionInfo?>(null) }
    var daemonStatus by remember { mutableStateOf<DaemonStatus?>(null) }
    var logLines by remember { mutableStateOf(listOf<String>()) }
    var pendingTasks by remember { mutableStateOf(listOf<com.rift.remote.model.QueuedTask>()) }
    var historyTasks by remember { mutableStateOf(listOf<com.rift.remote.model.QueuedTask>()) }
    
    // Connect to daemon when connection info is available
    LaunchedEffect(connectionInfo) {
        connectionInfo?.let { info ->
            apiClient.configure(info.host, info.port, info.token)
            
            // Test connection
            apiClient.checkHealth { healthy ->
                if (healthy) {
                    // Connect WebSocket
                    wsClient.connect(info.host, info.port, info.token)
                    
                    // Load initial data
                    apiClient.getStatus { status ->
                        daemonStatus = status
                    }
                    apiClient.getQueue { tasks ->
                        pendingTasks = tasks
                    }
                    apiClient.getHistory { tasks ->
                        historyTasks = tasks
                    }
                    
                    currentScreen = Screen.Dashboard
                }
            }
        }
    }
    
    when (currentScreen) {
        Screen.QRScan -> {
            QRScanScreen(
                onQRScanned = { qrData ->
                    try {
                        val info = Json.decodeFromString(ConnectionInfo.serializer(), qrData)
                        connectionInfo = info
                    } catch (e: Exception) {
                        // Handle error
                    }
                },
                onManualEntry = {
                    currentScreen = Screen.ManualEntry
                }
            )
        }
        
        Screen.ManualEntry -> {
            ManualEntryScreen(
                onConnect = { host, port, token ->
                    connectionInfo = ConnectionInfo(
                        version = "1",
                        host = host,
                        port = port,
                        token = token
                    )
                },
                onBack = {
                    currentScreen = Screen.QRScan
                }
            )
        }
        
        Screen.Dashboard -> {
            DashboardScreen(
                wsClient = wsClient,
                apiStatus = daemonStatus,
                onSubmitTask = { currentScreen = Screen.SubmitTask },
                onViewQueue = { currentScreen = Screen.Queue },
                onViewLogs = { currentScreen = Screen.LiveLogs }
            )
        }
        
        Screen.SubmitTask -> {
            SubmitTaskScreen(
                onSubmit = { goal ->
                    apiClient.submitTask(goal) { response ->
                        // Handle response
                        apiClient.getQueue { tasks ->
                            pendingTasks = tasks
                        }
                    }
                },
                onBack = { currentScreen = Screen.Dashboard }
            )
        }
        
        Screen.Queue -> {
            QueueScreen(
                pendingTasks = pendingTasks,
                historyTasks = historyTasks,
                onCancelTask = { taskId ->
                    apiClient.cancelTask(taskId) { success ->
                        if (success) {
                            apiClient.getQueue { tasks ->
                                pendingTasks = tasks
                            }
                        }
                    }
                },
                onBack = { currentScreen = Screen.Dashboard }
            )
        }
        
        Screen.LiveLogs -> {
            LiveLogScreen(
                logLines = logLines,
                isConnected = wsClient.isConnected(),
                onBack = { currentScreen = Screen.Dashboard },
                onClear = { logLines = emptyList() }
            )
        }
    }
}

enum class Screen {
    QRScan,
    ManualEntry,
    Dashboard,
    SubmitTask,
    Queue,
    LiveLogs
}

@Composable
fun RiftRemoteTheme(
    content: @Composable () -> Unit
) {
    MaterialTheme(
        colorScheme = MaterialTheme.colorScheme,
        typography = MaterialTheme.typography,
        content = content
    )
}
