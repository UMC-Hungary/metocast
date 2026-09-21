// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Metocast",
    platforms: [
        .iOS(.v16),
        .macOS(.v13),
    ],
    products: [
        .library(name: "Metocast", targets: ["Metocast"]),
    ],
    targets: [
        .target(
            name: "Metocast",
            dependencies: ["MetocastFFI"],
            path: "Sources/Metocast"
        ),
        .binaryTarget(
            name: "MetocastFFI",
            path: "MetocastFFI.xcframework"
        ),
        .testTarget(
            name: "MetocastTests",
            dependencies: ["Metocast"],
            path: "Tests/MetocastTests"
        ),
    ]
)
