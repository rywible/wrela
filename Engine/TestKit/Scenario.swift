import CryptoKit
import FieldCore
import Foundation
import SimulationCore
import simd

public enum TestStep: Codable, Equatable {
  case action(SimulationAction)
  case advance(Int, running: Bool = false)
  case walk(x: Float, z: Float, maxTicks: Int = 2400)
  case expect(String, String)
  case range(String, Double, Double)
  case save(String)
  case load(String)
  case capture(String)
}
@resultBuilder public enum ScenarioBuilder {
  public static func buildExpression(_ expression: TestStep) -> TestStep { expression }
  public static func buildBlock(_ parts: TestStep...) -> [TestStep] { parts }
}
public struct GameTest: Codable {
  public var name: String
  public var fixture: String
  public var tags: [String]
  public var steps: [TestStep]
  public init(
    _ name: String, fixture: String, tags: [String] = [], @ScenarioBuilder steps: () -> [TestStep]
  ) {
    self.name = name
    self.fixture = fixture
    self.tags = tags
    self.steps = steps()
  }
}
public struct SimulationWorkload: Codable {
  public var name: String
  public var fixture: String
  public var actors: Int
  public var ticks: Int
  public init(_ name: String, fixture: String, actors: Int = 1, ticks: Int = 60) {
    self.name = name
    self.fixture = fixture
    self.actors = actors
    self.ticks = ticks
  }
}
public struct TestProject {
  public var id: String
  public var product: String
  public var fixtures: [String]
  public var tests: [GameTest]
  public var workloads: [SimulationWorkload]
  public var create: (String, UInt32) throws -> any GameSimulation
  public init(
    id: String, product: String, fixtures: [String], tests: [GameTest],
    workloads: [SimulationWorkload]? = nil,
    create: @escaping (String, UInt32) throws -> any GameSimulation
  ) {
    self.id = id
    self.product = product
    self.fixtures = fixtures
    self.tests = tests
    self.workloads =
      workloads ?? fixtures.map { SimulationWorkload("simulation-\($0)-60-ticks", fixture: $0) }
    self.create = create
  }
}
public struct TraceFrame: Codable {
  public var index: Int
  public var label: String
  public var actions: [SimulationAction]
  public var restore: Data?
  public var state: Data
  public var observations: [String: String]
  public var digest: String { TestDigest.data(state) }
  public var capture: String?
  public var actionError: String?
}
public struct RunArtifact: Codable {
  public var version = 1
  public var owner: String
  public var product: String
  public var test: GameTest
  public var seed: UInt32
  public var initial: Data
  public var frames: [TraceFrame] = []
  public var passed = true
  public var failure: String?
  public var failureStep: Int?
  public var elapsedMilliseconds: Double = 0
  public var coverage: [String: Int] = [:]
  public var replayCommand: String = ""
  public var sourceDigest: String = ""
}
public enum TestDigest {
  public static func data(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }
  public static func firstDifference(_ expected: Data, _ actual: Data) -> String? {
    guard expected != actual else { return nil }
    func diff(_ a: Any, _ b: Any, _ path: String) -> String? {
      if let x = a as? [String: Any], let y = b as? [String: Any] {
        for k in Set(x.keys).union(y.keys).sorted() {
          guard let left = x[k], let right = y[k] else { return path + "." + k + " missing" }
          if let result = diff(left, right, path + "." + k) { return result }
        }
        return nil
      }
      if let x = a as? [Any], let y = b as? [Any] {
        guard x.count == y.count else { return path + " length \(x.count) != \(y.count)" }
        for i in x.indices { if let r = diff(x[i], y[i], path + "[\(i)]") { return r } }
        return nil
      }
      if let x = a as? String, let y = b as? String, x != y, let l = Data(base64Encoded: x),
        let r = Data(base64Encoded: y), let lj = try? JSONSerialization.jsonObject(with: l),
        let rj = try? JSONSerialization.jsonObject(with: r)
      {
        return diff(lj, rj, path)
      }
      return String(describing: a) == String(describing: b) ? nil : "\(path): \(a) != \(b)"
    }
    guard let a = try? JSONSerialization.jsonObject(with: expected),
      let b = try? JSONSerialization.jsonObject(with: actual)
    else { return "Snapshot bytes differ" }
    return diff(a, b, "state") ?? "Snapshot encoding differs"
  }
}
public enum ScenarioRunner {
  public static func run(
    _ test: GameTest, project: TestProject, seed: UInt32 = 17, trace: Bool = true
  ) throws -> RunArtifact {
    guard !test.name.isEmpty, test.name.count <= 80,
      test.name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }
      ), test.steps.count <= 10000
    else { throw SimulationFailure.invalid("Invalid scenario name or more than 10000 steps") }
    let world = try project.create(test.fixture, seed)
    let initial = try world.checkpoint()
    var result = RunArtifact(
      owner: project.id, product: project.product, test: test, seed: seed, initial: initial)
    var bookmarks: [String: Data] = [:]
    let started = DispatchTime.now().uptimeNanoseconds
    for (index, step) in test.steps.enumerated() {
      var actions: [SimulationAction] = []
      var restored: Data?
      var capture: String?
      var actionError: String?
      func perform(_ action: SimulationAction) throws {
        actions.append(action)
        do { try world.apply(action) } catch {
          actionError = error.localizedDescription
          throw error
        }
        for key in ["phase", "encounter", "behavior"] {
          if let value = world.observations[key] {
            result.coverage[key + ":" + value, default: 0] += 1
          }
        }
      }
      do {
        switch step {
        case .action(let a): try perform(a)
        case .advance(let n, let running):
          guard (0...36000).contains(n) else {
            throw SimulationFailure.invalid("Tick count outside 0...36000")
          }
          for _ in 0..<n { try perform(.tick(running: running)) }
        case .walk(let x, let z, let maxTicks):
          guard x.isFinite, z.isFinite, (1...36000).contains(maxTicks) else {
            throw SimulationFailure.invalid("Invalid walk target")
          }
          var arrived = false
          for _ in 0..<maxTicks {
            let d = SIMD2(x - world.camera.position.x, z - world.camera.position.z)
            if length(d) < 0.25 {
              arrived = true
              break
            }
            try perform(.look(yaw: atan2(d.x, -d.y), pitch: world.camera.pitch))
            try perform(.move(V3(0, 0, min(5.5 / 60, length(d)))))
            try perform(.tick(running: false))
          }
          guard arrived else {
            throw SimulationFailure.invalid(
              "Walk did not reach (\(x),\(z)); actual \(world.camera.position)")
          }
        case .expect(let key, let expected):
          guard world.observations[key] == expected else {
            throw SimulationFailure.invalid(
              "\(key): expected \(expected), got \(world.observations[key] ?? "missing")")
          }
        case .range(let key, let lo, let hi):
          guard lo.isFinite, hi.isFinite, lo <= hi, let text = world.observations[key],
            let value = Double(text), value.isFinite, (lo...hi).contains(value)
          else {
            throw SimulationFailure.invalid(
              "\(key): expected \(lo)...\(hi), got \(world.observations[key] ?? "missing")")
          }
        case .save(let key): bookmarks[key] = try world.checkpoint()
        case .load(let key):
          guard let data = bookmarks[key] else {
            throw SimulationFailure.invalid("Unknown bookmark \(key)")
          }
          try world.restore(data)
          restored = data
        case .capture(let label): capture = label
        }
      } catch {
        result.passed = false
        result.failure = error.localizedDescription
        result.failureStep = index
      }
      if trace || !result.passed {
        result.frames.append(
          TraceFrame(
            index: index, label: String(describing: step), actions: actions, restore: restored,
            state: try world.checkpoint(), observations: world.observations, capture: capture,
            actionError: actionError))
      }
      if !result.passed { break }
    }
    result.elapsedMilliseconds = Double(DispatchTime.now().uptimeNanoseconds - started) / 1e6
    return result
  }
  /// Replays recorded production actions, comparing state after each action group.
  public static func replay(_ artifact: RunArtifact, project: TestProject) throws -> String? {
    let world = try project.create(artifact.test.fixture, artifact.seed)
    try world.restore(artifact.initial)
    for frame in artifact.frames {
      if let state = frame.restore { try world.restore(state) }
      var caught: String?
      do { for action in frame.actions { try world.apply(action) } } catch {
        caught = error.localizedDescription
      }
      if caught != frame.actionError {
        return
          "Step \(frame.index): expected action error \(frame.actionError ?? "none"), got \(caught ?? "none")"
      }
      if let difference = TestDigest.firstDifference(frame.state, try world.checkpoint()) {
        return "Step \(frame.index): " + difference
      }
    }
    return nil
  }
}
