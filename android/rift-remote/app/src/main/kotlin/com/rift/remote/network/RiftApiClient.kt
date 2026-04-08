package com.rift.remote.network

import android.util.Log
import com.rift.remote.model.*
import kotlinx.serialization.json.Json
import okhttp3.*
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.util.concurrent.TimeUnit

private const val TAG = "RiftApi"
private val JSON_MEDIA_TYPE = "application/json".toMediaType()

/**
 * REST API client for Rift daemon
 */
class RiftApiClient {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .build()

    private val json = Json { ignoreUnknownKeys = true }
    
    private var baseUrl: String = ""
    private var token: String = ""

    fun configure(host: String, port: Int, token: String) {
        this.baseUrl = "http://$host:$port"
        this.token = token
    }

    fun isConfigured(): Boolean = baseUrl.isNotEmpty() && token.isNotEmpty()

    /**
     * Check if server is healthy
     */
    fun checkHealth(callback: (Boolean) -> Unit) {
        val request = Request.Builder()
            .url("$baseUrl/health")
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Health check failed: ${e.message}")
                callback(false)
            }

            override fun onResponse(call: Call, response: Response) {
                callback(response.isSuccessful && response.body?.string() == "ok")
            }
        })
    }

    /**
     * Get daemon status
     */
    fun getStatus(callback: (DaemonStatus?) -> Unit) {
        val request = Request.Builder()
            .url("$baseUrl/api/status?token=$token")
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Get status failed: ${e.message}")
                callback(null)
            }

            override fun onResponse(call: Call, response: Response) {
                if (!response.isSuccessful) {
                    Log.e(TAG, "Get status failed: ${response.code}")
                    callback(null)
                    return
                }
                
                response.body?.string()?.let { body ->
                    try {
                        val status = json.decodeFromString(DaemonStatus.serializer(), body)
                        callback(status)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to parse status: $e")
                        callback(null)
                    }
                } ?: callback(null)
            }
        })
    }

    /**
     * Get task queue
     */
    fun getQueue(callback: (List<QueuedTask>) -> Unit) {
        val request = Request.Builder()
            .url("$baseUrl/api/queue?token=$token")
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Get queue failed: ${e.message}")
                callback(emptyList())
            }

            override fun onResponse(call: Call, response: Response) {
                if (!response.isSuccessful) {
                    callback(emptyList())
                    return
                }
                
                response.body?.string()?.let { body ->
                    try {
                        val tasks = json.decodeFromString(List.serializer(QueuedTask.serializer()), body)
                        callback(tasks)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to parse queue: $e")
                        callback(emptyList())
                    }
                } ?: callback(emptyList())
            }
        })
    }

    /**
     * Get task history
     */
    fun getHistory(callback: (List<QueuedTask>) -> Unit) {
        val request = Request.Builder()
            .url("$baseUrl/api/history?token=$token")
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Get history failed: ${e.message}")
                callback(emptyList())
            }

            override fun onResponse(call: Call, response: Response) {
                if (!response.isSuccessful) {
                    callback(emptyList())
                    return
                }
                
                response.body?.string()?.let { body ->
                    try {
                        val tasks = json.decodeFromString(List.serializer(QueuedTask.serializer()), body)
                        callback(tasks)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to parse history: $e")
                        callback(emptyList())
                    }
                } ?: callback(emptyList())
            }
        })
    }

    /**
     * Submit a new task
     */
    fun submitTask(
        goal: String,
        autoCorrect: Boolean = true,
        callback: (SubmitTaskResponse?) -> Unit
    ) {
        val requestBody = json.encodeToString(
            SubmitTaskRequest.serializer(),
            SubmitTaskRequest(goal, autoCorrect, false)
        ).toRequestBody(JSON_MEDIA_TYPE)

        val request = Request.Builder()
            .url("$baseUrl/api/tasks?token=$token")
            .post(requestBody)
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Submit task failed: ${e.message}")
                callback(null)
            }

            override fun onResponse(call: Call, response: Response) {
                if (!response.isSuccessful) {
                    Log.e(TAG, "Submit task failed: ${response.code}")
                    callback(null)
                    return
                }
                
                response.body?.string()?.let { body ->
                    try {
                        val result = json.decodeFromString(SubmitTaskResponse.serializer(), body)
                        callback(result)
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to parse response: $e")
                        callback(null)
                    }
                } ?: callback(null)
            }
        })
    }

    /**
     * Cancel a task
     */
    fun cancelTask(taskId: String, callback: (Boolean) -> Unit) {
        val request = Request.Builder()
            .url("$baseUrl/api/tasks/$taskId/cancel?token=$token")
            .post("".toRequestBody(null))
            .build()

        client.newCall(request).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                Log.e(TAG, "Cancel task failed: ${e.message}")
                callback(false)
            }

            override fun onResponse(call: Call, response: Response) {
                callback(response.isSuccessful)
            }
        })
    }
}
