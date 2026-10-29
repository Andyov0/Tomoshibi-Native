import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("io.github.takahirom.roborazzi")
}

// The version people are told, and which build of it this is. A small fix
// keeps the version and takes the next build; a new version is for new
// features or a large update (owner, 2026-10-22).
val release = "0.1.6"
val build = 2

// The version name joins them as the core's version_with_build does
// (core/protocol/src/api/updates.rs), written out here because the build
// cannot call the core: the version alone for the first build, the build as a
// fourth part after it (0.1.4.2) for every later one. The name is what an
// installed app reports and compares in its update check, never the version
// code, so a second build named 0.1.4 like the first would be offered to
// nobody; and the first has no fourth part because a 0.1.4.1 is newer than
// 0.1.4 by the core's rule, and would be offered to the 0.1.4 already there.
val joined = if (build > 1) "$release.$build" else release

// The deployment the app is built for, whose links open in it: TOMOSHIBI_HOST
// from the environment, else deployment.local at the repository's root (not
// committed), else deployment.example's meet.example.com, as the core reads it
// (core/protocol/build.rs). Kept out of the source so that the published
// source names no deployment (owner, 2026-10-22).
val deploymentHost: String = System.getenv("TOMOSHIBI_HOST")?.trim()?.takeIf { it.isNotEmpty() }
    ?: listOf("deployment.local", "deployment.example")
        .map { rootDir.parentFile.resolve(it) }
        .firstNotNullOfOrNull { file ->
            file.takeIf { it.isFile }?.readLines()?.firstNotNullOfOrNull { line ->
                val said = line.substringBefore("//").split("=", limit = 2)
                said.takeIf { it.size == 2 && it[0].trim() == "TOMOSHIBI_HOST" }?.get(1)?.trim()?.takeIf { it.isNotEmpty() }
            }
        }
    ?: "meet.example.com"

// The people the lobby's camera card draws (ui/lobby/PortraitArt.kt): the
// repository's art/portraits.json, which every platform draws from, copied
// into the app's assets as it is rather than kept here as a second copy that
// could drift from the iPhone's. A build without it fails: a card that draws
// nobody would otherwise be found only by looking at it.
abstract class PortraitAssets : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val drawings: RegularFileProperty

    @get:OutputDirectory
    abstract val assets: DirectoryProperty

    @TaskAction
    fun copy() {
        val out = assets.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        drawings.get().asFile.copyTo(out.resolve("portraits.json"), overwrite = true)
    }
}

val portraitAssets = tasks.register<PortraitAssets>("portraitAssets") {
    drawings.set(rootDir.parentFile.resolve("art/portraits.json"))
}

// Refuses a release build over a core that is not one to hand anybody.
// build-core.sh builds a debug core unless told `release`, and a release APK
// around a debug or `local` core built and ran as well as any: the only
// difference was that the core admitted whatever deployment a link named
// (protocol api::admitted keys on debug assertions). A checkout with no
// deployment.local builds for meet.example.com, which reaches nothing, and
// that built cleanly too; and a core with no update key installs whatever
// build the deployment hands it. The core writes what it is into itself
// (protocol api BUILD_MARK: profile, host, update key); this reads it out of
// the library the APK will carry, and holds it to the host this build gives
// the app's links and to the update key this checkout names, where it names
// one (TOMOSHIBI_UPDATE_KEY in the environment, else in deployment.local).
abstract class RequireReleaseCore : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val library: RegularFileProperty

    @get:Input
    abstract val host: Property<String>

    @get:Input
    abstract val updateKey: Property<String>

    @TaskAction
    fun check() {
        val file = library.get().asFile
        val text = String(file.readBytes(), Charsets.ISO_8859_1)
        val marks = Regex("tomoshibi-core-build:[a-z]*:[a-z0-9.-]*:[0-9a-f]*;").findAll(text).map { it.value }.toSet()
        if (marks.size != 1) throw GradleException("$file carries ${marks.size} core build marks, not one; is it older than the mark?")
        val (profile, builtFor, key) = marks.single().removeSuffix(";").split(":").drop(1)
        if (profile != "release") throw GradleException("$file is a $profile core, which admits any deployment; run android/build-core.sh release")
        if (builtFor == "meet.example.com") throw GradleException("$file is built for meet.example.com, which reaches nothing; set TOMOSHIBI_HOST or deployment.local (see deployment.example) and build the core again")
        if (key.length != 64) throw GradleException("$file has no update key and would install any build the deployment hands it; set TOMOSHIBI_UPDATE_KEY (see deployment.example) and build the core again")
        if (builtFor != host.get().lowercase()) throw GradleException("$file is built for $builtFor and this app's links for ${host.get()}; build both for one deployment")
        val wanted = updateKey.get().lowercase()
        if (wanted.isNotEmpty() && wanted != key) throw GradleException("$file holds updates to another key than this checkout's TOMOSHIBI_UPDATE_KEY")
    }
}

val requireReleaseCore = tasks.register<RequireReleaseCore>("requireReleaseCore") {
    library.set(rootDir.resolve("core/src/main/jniLibs/arm64-v8a/libtomoshibi_core.so"))
    host.set(deploymentHost)
    updateKey.set(
        System.getenv("TOMOSHIBI_UPDATE_KEY")?.trim()?.takeIf { it.isNotEmpty() }
            ?: rootDir.parentFile.resolve("deployment.local").takeIf { it.isFile }?.readLines()?.firstNotNullOfOrNull { line ->
                val said = line.substringBefore("//").split("=", limit = 2)
                said.takeIf { it.size == 2 && it[0].trim() == "TOMOSHIBI_UPDATE_KEY" }?.get(1)?.trim()?.takeIf { it.isNotEmpty() }
            }
            ?: ""
    )
}
tasks.matching { it.name == "preReleaseBuild" }.configureEach { dependsOn(requireReleaseCore) }

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(portraitAssets, PortraitAssets::assets)
    }
}

android {
    namespace = "app.tomoshibi"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.tomoshibi.android"
        minSdk = 29
        targetSdk = 36
        // One more for every build of every version: Android installs over
        // the app only a version code above its own.
        versionCode = 9
        versionName = joined
        manifestPlaceholders["tomoshibiHost"] = deploymentHost.lowercase()
        ndk { abiFilters += "arm64-v8a" }
    }

    // The release key lives in the owner's vault; keystore.properties (not
    // committed) says where, and the password is read from its own file
    // there rather than written into any build file. Without it a release
    // build is unsigned, which is enough to see that it builds.
    val signing = rootProject.file("keystore.properties").takeIf { it.exists() }?.let { file ->
        // Read as UTF-8: Properties alone reads ISO-8859-1, and the vault's path
        // is not ASCII.
        Properties().apply { file.reader(Charsets.UTF_8).use { load(it) } }
    }
    signingConfigs {
        if (signing != null) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                val password = file(signing.getProperty("passwordFile")).readText().trim()
                storePassword = password
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = password
                storeType = "PKCS12"
            }
        }
    }

    buildTypes {
        release {
            // R8 shrinks the Compose, Media3 and icon libraries to what is
            // used; the core module's consumer rules keep what JNA, UniFFI and
            // WebRTC's JNI find by name.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (signing != null) signingConfig = signingConfigs.getByName("release")
        }
        // The release build's shrinking on a debuggable build: a release
        // build talks to the production deployment and nothing else, so
        // whether R8 kept everything the core finds by name (JNA's
        // structures, UniFFI's callbacks, WebRTC's Java classes) is checked
        // here, against a local server, before a release goes out.
        create("shrunk") {
            initWith(getByName("debug"))
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            matchingFallbacks += "debug"
            applicationIdSuffix = ".shrunk"
        }
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
                // The appearance the pictures are drawn in: the dark unless
                // `-Pappearance=light` (ui/TestAppearance.kt).
                it.systemProperty("tomoshibi.appearance", providers.gradleProperty("appearance").getOrElse("dark"))
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
