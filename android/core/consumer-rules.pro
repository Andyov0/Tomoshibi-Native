# JNA finds the bindings' structures and callbacks by reflection, and WebRTC's
# native half finds its Java classes by name through JNI.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
# All of libwebrtc.jar, not only livekit.org.webrtc: its JNI glue (livekit.org.jni_zero.JniZero)
# is found by name from native code too, and a shrunk build aborted at the first call without it.
-keep class livekit.org.** { *; }
-keepclassmembers class livekit.org.** { native <methods>; }
-keep class app.tomoshibi.core.CoreNative { *; }
-dontwarn java.awt.**
# JniZero refers to JniZeroJni, which libwebrtc.jar does not carry: its methods are registered
# from native code, and R8 refuses to finish over the missing class unless told it is expected.
-dontwarn livekit.org.jni_zero.JniZeroJni
