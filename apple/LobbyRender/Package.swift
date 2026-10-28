// swift-tools-version:5.9
// Pictures of the Mac lobby drawn without a window on the screen, for when
// there is no Mac to run the app on that a person can look at (see
// Sources/main.swift).
import PackageDescription

let package = Package(
    name: "LobbyRender",
    platforms: [.macOS(.v14)],
    dependencies: [.package(path: "../TomoshibiKit")],
    targets: [
        .executableTarget(
            name: "lobby-render",
            dependencies: [.product(name: "TomoshibiKit", package: "TomoshibiKit")],
            path: "Sources"
        ),
    ]
)
