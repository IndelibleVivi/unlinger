// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "UnlingerApp",
    defaultLocalization: "en",
    platforms: [.macOS(.v14)],
    targets: [
        .target(
            name: "UnlingerKit",
            path: "Sources/UnlingerKit",
            resources: [
                .process("Copy/en.lproj"),
                .process("Copy/zh-Hans.lproj"),
                .process("Assets")
            ]
        ),
        .executableTarget(
            name: "UnlingerApp",
            dependencies: ["UnlingerKit"],
            path: "Sources/UnlingerApp"
        ),
        .testTarget(
            name: "UnlingerAppTests",
            dependencies: ["UnlingerKit"],
            path: "Tests/UnlingerAppTests"
        )
    ]
)
