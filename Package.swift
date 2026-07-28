// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "rasitop_app",
  platforms: [.macOS(.v13)],
  products: [
    .library(name: "rasitop_app", targets: ["rasitop_app"])
  ],
  targets: [
    .target(
      name: "rasitop_app",
      path: "app-macos/Sources/rasitop_app",
      swiftSettings: [
        .unsafeFlags([
          "-import-objc-header",
          "app-macos/include/rasitop.h",
        ])
      ]
    )
  ],
  swiftLanguageModes: [.v5]
)
