package com.rift.remote.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable
fun SubmitTaskScreen(
    onSubmit: (String) -> Unit,
    onBack: () -> Unit,
    isSubmitting: Boolean = false
) {
    var goal by remember { mutableStateOf("") }
    var submitted by remember { mutableStateOf(false) }
    var submittedGoal by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text(
            text = "Submit Task",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold
        )

        Text(
            text = "Describe what you want Rift to accomplish:",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )

        OutlinedTextField(
            value = goal,
            onValueChange = { 
                goal = it
                submitted = false
            },
            label = { Text("Goal") },
            placeholder = { 
                Text("e.g. Refactor the authentication module to use JWT tokens") 
            },
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            maxLines = 10,
            shape = RoundedCornerShape(12.dp)
        )

        if (submitted) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer
                )
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text(
                        text = "✅ Task submitted!",
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onPrimaryContainer
                    )
                    Text(
                        text = submittedGoal,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onPrimaryContainer
                    )
                }
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            OutlinedButton(
                onClick = onBack,
                modifier = Modifier.weight(1f),
                shape = RoundedCornerShape(12.dp),
                enabled = !isSubmitting
            ) {
                Text("Back")
            }
            
            Button(
                onClick = {
                    if (goal.isNotBlank()) {
                        submittedGoal = goal
                        onSubmit(goal)
                        submitted = true
                        goal = ""
                    }
                },
                modifier = Modifier.weight(2f),
                shape = RoundedCornerShape(12.dp),
                enabled = goal.isNotBlank() && !isSubmitting
            ) {
                if (isSubmitting) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(20.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onPrimary
                    )
                } else {
                    Text("Submit Task")
                }
            }
        }
    }
}
