// swift-tools-version:5.9
// The Rust core as a Swift package: the XCFramework that apple/build-core.sh
// packs, and the Swift that UniFFI generated beside it.
import PackageDescription

let mac = BuildSettingCondition.when(platforms: [.macOS])
let ios = BuildSettingCondition.when(platforms: [.iOS])

let package = Package(
    name: "TomoshibiCore",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "TomoshibiCore", targets: ["TomoshibiCore"])],
    targets: [
        .binaryTarget(name: "tomoshibi_coreFFI", path: "TomoshibiCoreFFI.xcframework"),
        .target(
            name: "TomoshibiCore",
            dependencies: ["tomoshibi_coreFFI"],
            path: "Sources/TomoshibiCore",
            linkerSettings: [
                // The frameworks webrtc-sys's build script names for each
                // platform. A static library cannot carry them, so the app
                // that links it has to.
                .linkedLibrary("c++"),
                .linkedFramework("Foundation"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("Security"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("VideoToolbox"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("Metal"),
                .linkedFramework("MetalKit"),
                .linkedFramework("QuartzCore"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("AppKit", mac),
                .linkedFramework("OpenGL", mac),
                .linkedFramework("IOKit", mac),
                .linkedFramework("IOSurface", mac),
                .linkedFramework("ScreenCaptureKit", mac),
                .linkedFramework("UIKit", ios),
                .linkedFramework("CoreFoundation", ios),
                .linkedFramework("OpenGLES", ios),
                .linkedFramework("GLKit", ios),
                .linkedFramework("Network", ios),
                // The prebuilt WebRTC's Objective-C categories are only kept
                // when the final link is told to; see core/.cargo/config.toml.
                .unsafeFlags(["-Xlinker", "-ObjC"]),
            ]
        ),
    ]
)
