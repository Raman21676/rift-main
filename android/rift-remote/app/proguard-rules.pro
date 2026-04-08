# ProGuard rules for Rift Remote

# Keep model classes for serialization
-keep class com.rift.remote.model.** { *; }

# Keep Kotlin serialization
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt
-keepclassmembers class kotlinx.serialization.json.** { *; }

# OkHttp
-dontwarn okhttp3.**
-dontwarn okio.**
