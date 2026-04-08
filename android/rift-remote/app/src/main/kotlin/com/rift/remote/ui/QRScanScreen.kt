package com.rift.remote.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
fun QRScanScreen(
    onQRScanned: (String) -> Unit,
    onManualEntry: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        Text(
            text = "Connect to Rift",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(bottom = 16.dp)
        )

        Text(
            text = "Scan the QR code displayed in your terminal when you run:\n\nrift daemon start --remote",
            style = MaterialTheme.typography.bodyLarge,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 32.dp)
        )

        // QR Scanner placeholder
        Card(
            modifier = Modifier
                .size(280.dp)
                .padding(16.dp),
            shape = RoundedCornerShape(16.dp),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant
            )
        ) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Text(
                        text = "📷",
                        fontSize = 64.sp
                    )
                    Text(
                        text = "Camera Preview",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Text(
                        text = "(QR Scanner integration here)",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }

        Spacer(modifier = Modifier.height(32.dp))

        // Manual entry option
        TextButton(onClick = onManualEntry) {
            Text("Enter connection details manually")
        }

        Spacer(modifier = Modifier.height(16.dp))

        // Demo button for testing
        OutlinedButton(
            onClick = {
                // Demo connection for testing
                val demoJson = """{"version":"1","host":"192.168.1.5","port":7788,"token":"ABCD1234EFGH5678IJKL9012MNOP3456"}"""
                onQRScanned(demoJson)
            },
            modifier = Modifier.padding(top = 16.dp)
        ) {
            Text("Use Demo Connection")
        }
    }
}

@Composable
fun ManualEntryScreen(
    onConnect: (host: String, port: Int, token: String) -> Unit,
    onBack: () -> Unit
) {
    var host by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("7788") }
    var token by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text(
            text = "Manual Connection",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold
        )

        OutlinedTextField(
            value = host,
            onValueChange = { host = it },
            label = { Text("Host IP") },
            placeholder = { Text("192.168.1.5") },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true
        )

        OutlinedTextField(
            value = port,
            onValueChange = { port = it },
            label = { Text("Port") },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true
        )

        OutlinedTextField(
            value = token,
            onValueChange = { token = it },
            label = { Text("Token") },
            placeholder = { Text("32-character token from daemon") },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true
        )

        Spacer(modifier = Modifier.weight(1f))

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            OutlinedButton(
                onClick = onBack,
                modifier = Modifier.weight(1f)
            ) {
                Text("Back")
            }
            
            Button(
                onClick = {
                    val portNum = port.toIntOrNull() ?: 7788
                    onConnect(host, portNum, token)
                },
                modifier = Modifier.weight(2f),
                enabled = host.isNotBlank() && token.isNotBlank()
            ) {
                Text("Connect")
            }
        }
    }
}
