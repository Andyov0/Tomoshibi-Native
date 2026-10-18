plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("io.github.takahirom.roborazzi")
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

    buildTypes {
        debug {
            // `-PkitGallery`: a debug build installed beside the app rather
            // than over it, to look at the UI kit's gallery on a device where
            // somebody is testing the app itself.
            if (providers.gradleProperty("kitGallery").isPresent) applicationIdSuffix = ".kit"
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests {
            // The kit's screenshots read the phrases from assets and draw
            // with the real resources.
            isIncludeAndroidResources = true
            all {
                // Drawn through the hardware renderer, which carries
                // RenderEffect, rather than the software canvas.
                it.systemProperty("robolectric.pixelCopyRenderMode", "hardware")
                it.maxHeapSize = "3g"
            }
        }
    }

    packaging {
        // The core is loaded from the APK as it is; nothing in it is to be
        // extracted and nothing in it may be stripped twice.
        jniLibs { useLegacyPackaging = false }
    }

    androidResources {
        // The segmentation model is mapped from the APK as it lies, which a
        // compressed asset cannot be.
        noCompress += "tflite"
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
    // The call's notification (CallStyle) and its foreground service's types.
    implementation("androidx.core:core:1.18.0")
    // The web's own segmenter for blurring what is behind somebody: the same
    // MediaPipe selfie model the web serves from its deployment (live/blur.ts),
    // bundled here under assets/segment for the same reason the web bundles
    // it, which is that Google's storage does not answer from the mainland.
    // Chosen over ML Kit's selfie segmentation, which needs Play services,
    // reports its use to Google through Firebase's transport, and has been a
    // beta since 2022; 1.0.0 rather than 1.1 because 1.1 also brings an LLM
    // runtime nothing here uses.
    implementation("com.google.mediapipe:tasks-vision:1.0.0")
    // The picture of yourself before a call: CameraX's own view, which turns,
    // mirrors and crops the preview the way the camera delivers it.
    implementation("androidx.camera:camera-view:1.6.2")
    // Where a foldable's hinge is, so that a sheet or a column of the wide
    // lobby does not lie across it while the phone is half open.
    implementation("androidx.window:window:1.5.1")
    // The glass: a real backdrop blur (RenderEffect) on Android 12 and
    // later, a translucent fill of the same tone before it.
    implementation("dev.chrisbanes.haze:haze:2.0.1")
    implementation("dev.chrisbanes.haze:haze-blur:2.0.1")
    // The SF Symbols the Swift views name, drawn as their nearest Material
    // Rounded icons (ui/kit/Symbols.kt).
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    // The video watched together (feature/watch): files and playlists from
    // where they are or relayed, as AVPlayer plays them on Apple. HLS is a
    // module of its own; the relay's playlists say nothing in their address
    // that names HLS, so the player is told what each one is.
    implementation("androidx.media3:media3-exoplayer:1.11.1")
    implementation("androidx.media3:media3-exoplayer-hls:1.11.1")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.17")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.76.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.76.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-junit-rule:1.76.0")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    // The platform's org.json is a stub on the JVM; the records the web
    // writes (devices, hearing) are read with the real one in tests.
    testImplementation("org.json:json:20250517")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.11.0")
}
