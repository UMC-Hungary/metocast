// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-native-nav",
  platforms: [
    .iOS(.v13)
  ],
  products: [
    .library(
      name: "tauri-plugin-native-nav",
      type: .static,
      targets: ["tauri-plugin-native-nav"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-native-nav",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
