# JNA finds the bindings' structures and callbacks by reflection, and WebRTC's
# native half finds its Java classes by name through JNI.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-keep class livekit.org.webrtc.** { *; }
-keep class app.tomoshibi.core.CoreNative { *; }
-dontwarn java.awt.**
