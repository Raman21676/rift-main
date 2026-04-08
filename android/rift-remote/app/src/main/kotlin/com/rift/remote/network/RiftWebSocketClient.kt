package com.rift.remote.network

import android.util.Log
import com.rift.remote.model.TaskEvent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.*
import okio.ByteString
import java.util.concurrent.TimeUnit

private const val TAG = "RiftWS"

// Must match server constants
private const val MSG_STATUS: Byte = 0x01
private const val MSG_TASK_EVENT: Byte = 0x02
private const val MSG_COMMAND: Byte = 0x03
private const val MSG_PING: Byte = 0x04
private const val MSG_PONG: Byte = 0x05

/**
 * WebSocket client for real-time communication with Rift daemon
 */
class RiftWebSocketClient(
    private val onStatus: (JsonObject) -> Unit,
    private val onTaskEvent: (TaskEvent) -> Unit,
    private val onConnected: () -> Unit,
    private val onDisconnected: (String) -> Unit
) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS) // no timeout for streaming
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    private var ws: WebSocket? = null
    private var isConnected = false
    private val json = Json { ignoreUnknownKeys = true }

    fun connect(host: String, port: Int, token: String) {
        disconnect() // Close any existing connection
        
        val url = "ws://$host:$port/ws?token=$token"
        Log.i(TAG, "Connecting to $url")
        val request = Request.Builder().url(url).build()

        ws = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Log.i(TAG, "Connected")
                isConnected = true
                onConnected()
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                handleBinaryMessage(bytes)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                // Handle text messages for debugging
                Log.d(TAG, "Text message: $text")
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.e(TAG, "WS failure: ${t.message}")
                isConnected = false
                onDisconnected(t.message ?: "Connection failed")
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WS closed: $reason")
                isConnected = false
                onDisconnected(reason)
            }
        })
    }

    private fun handleBinaryMessage(bytes: ByteString) {
        val data = bytes.toByteArray()
        if (data.isEmpty()) return

        val payload = data.copyOfRange(1, data.size)

        when (data[0]) {
            MSG_STATUS -> {
                try {
                    val jsonStr = String(payload)
                    val jsonObj = json.parseToJsonElement(jsonStr).jsonObject
                    onStatus(jsonObj)
                } catch (e: Exception) {
                    Log.w(TAG, "Bad status JSON: $e")
                }
            }
            MSG_TASK_EVENT -> {
                try {
                    val event = json.decodeFromString(TaskEvent.serializer(), String(payload))
                    onTaskEvent(event)
                } catch (e: Exception) {
                    Log.w(TAG, "Bad event JSON: $e")
                }
            }
            MSG_PONG -> {
                // Pong received, connection is alive
                Log.d(TAG, "Pong received")
            }
            else -> {
                Log.w(TAG, "Unknown message type: ${data[0]}")
            }
        }
    }

    fun submitTask(goal: String) {
        val cmd = mapOf(
            "action" to "submit_task",
            "goal" to goal
        )
        sendCommand(cmd)
    }

    fun cancelTask(taskId: String) {
        val cmd = mapOf(
            "action" to "cancel_task",
            "task_id" to taskId
        )
        sendCommand(cmd)
    }

    fun requestStatus() {
        val cmd = mapOf("action" to "get_status")
        sendCommand(cmd)
    }

    private fun sendCommand(cmd: Map<String, String>) {
        if (!isConnected) {
            Log.w(TAG, "Cannot send command: not connected")
            return
        }
        
        try {
            val jsonStr = json.encodeToString(
                kotlinx.serialization.json.JsonObject.serializer(),
                JsonObject(cmd.mapValues { it.value.jsonPrimitive })
            )
            val payload = jsonStr.toByteArray()
            val msg = ByteArray(1 + payload.size)
            msg[0] = MSG_COMMAND
            payload.copyInto(msg, 1)
            ws?.send(ByteString.of(*msg))
        } catch (e: Exception) {
            Log.e(TAG, "Failed to send command: $e")
        }
    }

    fun disconnect() {
        isConnected = false
        ws?.close(1000, "User disconnected")
        ws = null
    }

    fun isConnected(): Boolean = isConnected
}
