plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "app.tomoshibi"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.tomoshibi.android"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += "arm64-v8a" }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        // The core is loaded from the APK as it is; nothing in it is to be
        // extracted and nothing in it may be stripped twice.
        jniLibs { useLegacyPackaging = false }
    }

    testOptions {
        // The models log what they do through android.util.Log, which the
        // JVM's stub of the platform otherwise answers with an exception;
        // nothing a test asserts on goes through the platform.
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    implementation(project(":core"))
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("androidx.camera:camera-camera2:1.6.2")
    implementation("androidx.camera:camera-lifecycle:1.6.2")

    testImplementation("junit:junit:4.13.2")
    // The platform's org.json is a stub on the JVM; the records the web
    // writes (devices, hearing) are read with the real one in tests.
    testImplementation("org.json:json:20250517")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.11.0")
}
