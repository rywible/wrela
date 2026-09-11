// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Sanctuary",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "Sanctuary", targets: ["SanctuaryGame"])],
    targets: [
        .target(name: "FieldCore"),
        .target(name: "SanctuaryContent", dependencies: ["FieldCore"]),
        .target(name: "CGeometry", exclude: ["LICENSE.md", "PROVENANCE.md"], publicHeadersPath: "include"),
        .target(name: "FieldCompiler", dependencies: ["FieldCore", "CGeometry"]),
        .executableTarget(name: "SanctuaryGame", dependencies: ["FieldCore", "FieldCompiler", "SanctuaryContent"],
                          resources: [.copy("Resources/Garden.metal"), .copy("Resources/Atmosphere.metal")]),
        .testTarget(name: "SanctuaryContentTests", dependencies: ["SanctuaryContent", "FieldCore"]),
        .testTarget(name: "FieldCoreTests", dependencies: ["FieldCore", "FieldCompiler", "SanctuaryContent"])
    ],
    swiftLanguageModes: [.v5]
)
