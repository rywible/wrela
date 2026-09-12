// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "Wrela", platforms: [.macOS(.v14)],
  products: [
    .executable(name: "Sanctuary", targets: ["SanctuaryApp"]),
    .executable(name: "Cave", targets: ["CaveApp"]),
    .executable(name: "Soundstage", targets: ["SoundstageApp"]),
    .executable(name: "WrelaTest", targets: ["WrelaTest"]),
    .executable(name: "ImageCompare", targets: ["ImageCompare"]),
  ],
  targets: [
    .target(name: "FieldCore", path: "Engine/FieldCore"),
    .target(name: "SimulationCore", dependencies: ["FieldCore"], path: "Engine/SimulationCore"),
    .target(
      name: "CGeometry", path: "Engine/CGeometry", exclude: ["LICENSE.md", "PROVENANCE.md"],
      publicHeadersPath: "include"),
    .target(
      name: "FieldCompiler", dependencies: ["FieldCore", "CGeometry"], path: "Engine/FieldCompiler"),
    .target(
      name: "FieldEngine", dependencies: ["FieldCore", "FieldCompiler", "SimulationCore"],
      path: "Engine/FieldEngine",
      resources: [.copy("Resources/Surface.metal"), .copy("Resources/Atmosphere.metal")]),
    .target(name: "GameHost", dependencies: ["FieldEngine", "FieldCore"], path: "Engine/GameHost"),
    .target(
      name: "SanctuaryContent", dependencies: ["FieldCore", "SimulationCore"],
      path: "Games/Sanctuary/Content"),
    .target(
      name: "SanctuaryProject",
      dependencies: ["FieldEngine", "FieldCore", "FieldCompiler", "SanctuaryContent"],
      path: "Games/Sanctuary/Project"),
    .target(
      name: "CaveContent", dependencies: ["FieldCore", "SimulationCore"], path: "Games/Cave/Content"
    ),
    .target(
      name: "CaveProject",
      dependencies: ["FieldEngine", "FieldCore", "FieldCompiler", "CaveContent"],
      path: "Games/Cave/Project"),
    .target(
      name: "SoundstageKit", dependencies: ["FieldEngine", "FieldCore", "FieldCompiler"],
      path: "Tools/SoundstageKit"),
    .executableTarget(
      name: "SanctuaryApp", dependencies: ["GameHost", "SanctuaryProject"],
      path: "Games/Sanctuary/App"),
    .executableTarget(
      name: "CaveApp", dependencies: ["GameHost", "CaveProject"], path: "Games/Cave/App"),
    .executableTarget(
      name: "SoundstageApp", dependencies: ["SoundstageKit", "SanctuaryProject", "CaveProject"],
      path: "Tools/Soundstage"),
    .executableTarget(name: "ImageCompare", path: "Tools/ImageCompare"),
    .target(
      name: "TestKit", dependencies: ["SimulationCore", "FieldCore"], path: "Engine/TestKit"),
    .target(
      name: "EngineTesting", dependencies: ["TestKit", "FieldCore", "FieldCompiler"],
      path: "Engine/Testing", exclude: ["RenderWorkloads.json"]),
    .target(
      name: "SanctuaryTesting",
      dependencies: ["TestKit", "SanctuaryContent", "SimulationCore", "FieldCore"],
      path: "Games/Sanctuary/Testing", exclude: ["RenderWorkloads.json", "NativeChecks.json"]),
    .target(
      name: "CaveTesting", dependencies: ["TestKit", "CaveContent", "SimulationCore", "FieldCore"],
      path: "Games/Cave/Testing", exclude: ["RenderWorkloads.json", "NativeChecks.json"]),
    .executableTarget(
      name: "WrelaTest",
      dependencies: [
        "SimulationCore", "TestKit", "EngineTesting", "SanctuaryTesting", "CaveTesting",
      ], path: "Tools/TestRunner"),
    .testTarget(
      name: "GameContractTests",
      dependencies: ["TestKit", "SimulationCore", "CaveTesting", "SanctuaryTesting"],
      path: "Tools/TestingContracts"),
    .testTarget(
      name: "TestKitTests", dependencies: ["TestKit", "SimulationCore", "FieldCore"],
      path: "Engine/TestKitTests"),
    .testTarget(
      name: "SanctuaryContentTests",
      dependencies: ["SanctuaryContent", "FieldCore", "FieldCompiler"],
      path: "Games/Sanctuary/Tests"),
    .testTarget(
      name: "CaveContentTests", dependencies: ["CaveContent", "FieldCore"], path: "Games/Cave/Tests"
    ),
    .testTarget(
      name: "FieldCoreTests", dependencies: ["FieldCore", "FieldCompiler"], path: "Engine/Tests"),
    .testTarget(
      name: "FieldEngineTests", dependencies: ["FieldEngine", "FieldCore"], path: "Engine/RuntimeTests"),
    .testTarget(
      name: "SoundstageKitTests", dependencies: ["SoundstageKit", "FieldEngine", "FieldCore"],
      path: "Tools/SoundstageTests"),
  ], swiftLanguageModes: [.v5])
