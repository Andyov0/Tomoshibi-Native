// swift-tools-version:5.9
// The Swift that both apps share: the bridge from Apple's audio, video and
// screen capture to the core, the views that draw a call, the phrases in four
// languages. The apps themselves are thin shells over this package.
import PackageDescription

let package = Package(
    name: "TomoshibiKit",
    defaultLocalization: "en",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "TomoshibiKit", targets: ["TomoshibiKit"])],
    dependencies: [.package(path: "../TomoshibiCore")],
    targets: [
        .target(
            name: "TomoshibiKit",
            dependencies: [.product(name: "TomoshibiCore", package: "TomoshibiCore")],
            path: "Sources/TomoshibiKit",
            resources: [.process("Resources")]
        ),
        .testTarget(name: "TomoshibiKitTests", dependencies: ["TomoshibiKit"], path: "Tests/TomoshibiKitTests"),
    ]
)
