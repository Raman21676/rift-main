package com.rift.remote.model

import kotlinx.serialization.Serializable

/**
 * Daemon status response from REST API
 */
@Serializable
data class DaemonStatus(
    val running: Boolean,
    val uptime_seconds: Long,
    val tasks_completed: Long,
    val tasks_failed: Long,
    val current_task: CurrentTaskInfo? = null,
    val queue: QueueInfo,
    val version: String
)

@Serializable
data class CurrentTaskInfo(
    val id: String,
    val goal: String,
    val status: String
)

@Serializable
data class QueueInfo(
    val pending: Int,
    val running: Int,
    val completed: Int,
    val failed: Int
)

/**
 * Task event from WebSocket
 */
@Serializable
data class TaskEvent(
    val task_id: String,
    val status: String,
    val log_line: String? = null,
    val timestamp: Long
)

/**
 * Queued task from REST API
 */
@Serializable
data class QueuedTask(
    val id: String,
    val goal: String,
    val status: String,
    val created_at: String,
    val started_at: String? = null,
    val completed_at: String? = null,
    val result: String? = null,
    val priority: Int
)

/**
 * Submit task request
 */
@Serializable
data class SubmitTaskRequest(
    val goal: String,
    val auto_correct: Boolean = true,
    val verify: Boolean = false
)

/**
 * Submit task response
 */
@Serializable
data class SubmitTaskResponse(
    val task_id: String,
    val status: String
)

/**
 * Connection info from QR code
 */
@Serializable
data class ConnectionInfo(
    val version: String,
    val host: String,
    val port: Int,
    val token: String,
    val public_ip: String? = null
) {
    fun getEffectiveHost(): String {
        // Prefer public IP if available, otherwise use local IP
        return public_ip ?: host
    }
}
