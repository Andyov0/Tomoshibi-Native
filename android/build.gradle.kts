plugins {
    id("com.android.application") version "9.4.1" apply false
    id("com.android.library") version "9.4.1" apply false
    // On the classpath so that the Kotlin built into the Android plugin and
    // the Compose compiler are the same version.
    id("org.jetbrains.kotlin.android") version "2.4.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false
    // JVM screenshots of the UI kit, drawn by Robolectric's native graphics.
    id("io.github.takahirom.roborazzi") version "1.76.0" apply false
}
