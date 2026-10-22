// The Rust core as an Android library: the shared library and its Kotlin
// bindings, both made by ../build-core.sh, with WebRTC's Java classes beside
// them.
plugins {
    id("com.android.library")
}

android {
    namespace = "app.tomoshibi.core"
    compileSdk = 37

    defaultConfig {
        minSdk = 29
        ndk { abiFilters += "arm64-v8a" }
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // The bindings call the core through JNA; the AAR carries its own
    // libjnidispatch for every ABI.
    api("net.java.dev.jna:jna:5.19.1@aar")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    // WebRTC's Java classes, which the app uses too: the screen's frames are
    // its TextureBuffers, made in a context shared with its encoders.
    api(files("libs/libwebrtc.jar"))
}
