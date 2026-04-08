package com.rift.remote.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

@Composable
fun LiveLogScreen(
    logLines: List<String>,
    isConnected: Boolean,
    onBack: () -> Unit,
    onClear: () -> Unit = {}
) {
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    
    // Auto-scroll to bottom when new log lines arrive
    LaunchedEffect(logLines.size) {
        if (logLines.isNotEmpty()) {
            scope.launch {
                listState.animateScrollToItem(logLines.size - 1)
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF0D0D0D))
            .padding(8.dp)
    ) {
        // Header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 8.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically
        ) {
            Row(
                verticalAlignment = androidx.compose.ui.Alignment.CenterVertically
            ) {
                Text(
                    text = "●",
                    color = if (isConnected) Color(0xFF4CAF50) else Color(0xFFF44336),
                    fontSize = 12.sp,
                    modifier = Modifier.padding(end = 8.dp)
                )
                Text(
                    text = "Live Logs",
                    color = Color.White,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
            }
            
            Row {
                TextButton(onClick = onClear) {
                    Text("Clear", color = Color.Gray)
                }
                TextButton(onClick = onBack) {
                    Text("Back", color = Color.White)
                }
            }
        }

        // Log output
        LazyColumn(
            state = listState,
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .background(Color(0xFF1A1A1A), RoundedCornerShape(8.dp))
                .padding(8.dp)
        ) {
            items(logLines) { line ->
                LogLine(line)
            }
        }

        // Stats footer
        Text(
            text = "${logLines.size} lines | ${if (isConnected) "Streaming" else "Disconnected"}",
            color = Color.Gray,
            fontSize = 12.sp,
            modifier = Modifier.padding(top = 8.dp)
        )
    }
}

@Composable
private fun LogLine(line: String) {
    val color = when {
        line.contains("ERROR", ignoreCase = true) || 
        line.contains("❌") -> Color(0xFFFF5252)
        
        line.contains("WARN", ignoreCase = true) || 
        line.contains("⚠️") -> Color(0xFFFFD740)
        
        line.contains("✅") || 
        line.contains("SUCCESS", ignoreCase = true) -> Color(0xFF69F0AE)
        
        line.contains("INFO", ignoreCase = true) || 
        line.contains("ℹ️") -> Color(0xFF90CAF9)
        
        line.startsWith("[") -> Color(0xFFB0BEC5) // Timestamp
        
        else -> Color(0xFFE0E0E0)
    }
    
    Text(
        text = line,
        color = color,
        fontFamily = FontFamily.Monospace,
        fontSize = 11.sp,
        lineHeight = 14.sp,
        modifier = Modifier.padding(vertical = 1.dp)
    )
}

@Composable
fun LogLinePreview() {
    val sampleLogs = listOf(
        "[2024/01/15 10:23:45] ℹ️  Daemon starting...",
        "[2024/01/15 10:23:46] ✅ Task a1b2c3d4 submitted: Refactor auth module",
        "[2024/01/15 10:23:47] ℹ️  Planning job...",
        "[2024/01/15 10:23:48] ⚠️  Rate limit approaching",
        "[2024/01/15 10:23:49] ❌ Connection timeout",
        "▶️  Running: cargo build --release",
        "    Compiling rift-core v0.1.0",
        "    Finished dev [unoptimized + debuginfo]",
        "✅ Task completed successfully"
    )
    
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xFF0D0D0D))
            .padding(8.dp)
    ) {
        sampleLogs.forEach { line ->
            LogLine(line)
        }
    }
}
