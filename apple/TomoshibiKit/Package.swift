// swift-tools-version:5.9
// The Swift that both apps share.
//
// TomoshibiBasics is everything that needs no core: the phrases in four
// languages, where pictures go, the palette. It is apart so that its tests run
// on their own; a test bundle that links the core's debug build (with WebRTC
// inside it) timed out before the first test, on the Mac and in the simulator
// alike. TomoshibiKit is the rest: the bridge from Apple's audio, video and
// screen capture to the core, and the views that draw a call.
import PackageDescription

let package = Package(
    name: "TomoshibiKit",
    defaultLocalization: "en",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "TomoshibiKit", targets: ["TomoshibiKit"])],
    dependencies: [.package(path: "../TomoshibiCore")],
    targets: [
        .target(
            name: "TomoshibiBasics",
            path: "Sources/TomoshibiBasics",
            resources: [.process("Resources")]
        ),
        .target(
            name: "TomoshibiKit",
            dependencies: ["TomoshibiBasics", .product(name: "TomoshibiCore", package: "TomoshibiCore")],
            path: "Sources/TomoshibiKit"
        ),
        .testTarget(name: "TomoshibiBasicsTests", dependencies: ["TomoshibiBasics"], path: "Tests/TomoshibiBasicsTests"),
    ]
)
