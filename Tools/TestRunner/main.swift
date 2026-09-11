import CaveTesting
import EngineTesting
import Foundation
import SanctuaryTesting
import SimulationCore
import TestKit

struct Catalog: Codable {
  var game: String
  var fixtures: [String]
  var tests: [GameTest]
}
struct CLIError: LocalizedError {
  let errorDescription: String?
  init(_ text: String) { errorDescription = text }
}
let args = Array(CommandLine.arguments.dropFirst())
func option(_ key: String, _ fallback: String) -> String {
  guard let i = args.firstIndex(of: "--" + key), i + 1 < args.count else { return fallback }
  return args[i + 1]
}
func integer(_ key: String, _ fallback: Int) throws -> Int {
  guard let n = Int(option(key, String(fallback))) else { throw CLIError("Invalid --\(key)") }
  return n
}
func emit<T: Encodable>(_ result: T) throws {
  let encoder = JSONEncoder()
  encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
  let data = try encoder.encode(result)
  let output = option("output", "")
  if !output.isEmpty {
    let url = URL(fileURLWithPath: output)
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try data.write(to: url, options: .atomic)
  }
  print(String(decoding: data, as: UTF8.self))
}
do {
  let workspace = URL(
    fileURLWithPath: option("workspace", FileManager.default.currentDirectoryPath))
  let projects = [
    CaveTesting.project(workspace: workspace), SanctuaryTesting.project(workspace: workspace),
  ]
  let owner = option("game", "cave")
  let project = projects.first { $0.id == owner }
  let command = args.first ?? "list"
  if command == "list" {
    try emit(projects.map { Catalog(game: $0.id, fixtures: $0.fixtures, tests: $0.tests) })
  } else if command == "bench" {
    let workloads: [Workload]
    if owner == "engine" {
      workloads = EngineWorkloads.all()
    } else {
      guard let project else { throw CLIError("Unknown game \(owner)") }
      workloads = try project.workloads.map { config in
        guard (1...256).contains(config.actors), (1...3600).contains(config.ticks) else {
          throw CLIError("Invalid game workload")
        }
        let worlds = try (0..<config.actors).map {
          try project.create(config.fixture, UInt32(17 + $0))
        }
        let initial = try worlds.map { try $0.checkpoint() }
        let definition = String(decoding: try SimulationCoding.encode(config), as: UTF8.self)
        return Workload(config.name, definition: definition) {
          var sum = 0.0
          for (world, state) in zip(worlds, initial) {
            try world.restore(state)
            for _ in 0..<config.ticks { world.advance(1 / 60, running: false) }
            try world.validate()
            sum += Double(world.camera.position.x)
          }
          return sum
        }
      }
    }
    try emit(
      workloads.filter { option("test", "all") == "all" || $0.name == option("test", "") }.map {
        try Performance.measure($0, samples: integer("samples", 20))
      })
  } else {
    guard let project else { throw CLIError("Unknown game \(owner)") }
    let seedValue = try integer("seed", 17)
    guard let seed = UInt32(exactly: seedValue) else { throw CLIError("Seed must be UInt32") }
    switch command {
    case "run":
      let scenarioPath = option("scenario", "")
      let available =
        scenarioPath.isEmpty
        ? project.tests
        : [
          try JSONDecoder().decode(
            GameTest.self, from: Data(contentsOf: URL(fileURLWithPath: scenarioPath)))
        ]
      let selected = available.filter { test in
        (option("test", "all") == "all" || test.name == option("test", ""))
          && (option("tag", "").isEmpty || test.tags.contains(option("tag", "")))
      }
      guard !selected.isEmpty else { throw CLIError("No tests match") }
      var runs: [RunArtifact] = []
      for test in selected {
        var run = try ScenarioRunner.run(test, project: project, seed: seed)
        run.sourceDigest = option("digest", "")
        runs.append(run)
      }
      try emit(runs)
      if runs.contains(where: { !$0.passed }) { exit(1) }
    case "dst":
      let report = try DeterministicTesting.campaign(
        project: project, seeds: integer("seeds", 8), ticks: integer("ticks", 600), startSeed: seed)
      try emit(report)
      if !report.passed { exit(1) }
    case "replay", "minimize":
      let artifact = try JSONDecoder().decode(
        RunArtifact.self, from: Data(contentsOf: URL(fileURLWithPath: option("artifact", ""))))
      guard artifact.version == 1, artifact.owner == owner else {
        throw CLIError("Artifact version or game does not match")
      }
      if command == "minimize" {
        try emit(
          DeterministicTesting.minimize(
            artifact, project: project, attempts: integer("attempts", 40)))
      } else {
        let difference = try ScenarioRunner.replay(artifact, project: project)
        let reproduction = try ScenarioRunner.run(
          artifact.test, project: project, seed: artifact.seed)
        try emit([
          "traceMatches": String(difference == nil), "difference": difference ?? "",
          "scenarioPassed": String(reproduction.passed), "failure": reproduction.failure ?? "",
        ])
        if difference != nil || reproduction.passed != artifact.passed
          || reproduction.failure != artifact.failure
        {
          exit(1)
        }
      }
    default: throw CLIError("Use list, run, dst, replay, minimize or bench")
    }
  }
} catch {
  fputs("WrelaTest: \(error.localizedDescription)\n", stderr)
  exit(2)
}
