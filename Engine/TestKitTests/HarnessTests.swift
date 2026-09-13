import FieldCore
import SimulationCore
import simd
import XCTest

@testable import TestKit

final class CounterWorld: GameSimulation {
  var camera = PlayerCamera(position: .zero)
  var count = 0
  var observations: [String: String] { ["count": String(count)] }
  func move(_ delta: V3) { camera.position += delta }
  func advance(_ seconds: Float, running: Bool) { count += 1 }
  func interact() throws -> String {
    count += 10
    return "ok"
  }
  func checkpoint() throws -> Data {
    try SimulationCoding.encode(State(camera: camera, count: count))
  }
  struct State: Codable {
    var camera: PlayerCamera
    var count: Int
  }
  func restore(_ data: Data) throws {
    let value = try JSONDecoder().decode(State.self, from: data)
    guard value.count >= 0 else { throw SimulationFailure.invalid("Invalid count") }
    try SimulationCoding.validateCamera(value.camera)
    camera = value.camera
    count = value.count
  }
  func validate() throws { try SimulationCoding.validateCamera(camera) }
}
final class GainWorld: GameSimulation {
  var camera = PlayerCamera(position: .zero)
  let gain: Float
  let blocked: Bool
  init(gain: Float, blocked: Bool = false) { self.gain = gain; self.blocked = blocked }
  var observations: [String: String] { [:] }
  func move(_ local: V3) {
    guard !blocked else { return }
    let right = V3(cos(camera.yaw), 0, sin(camera.yaw))
    let forward = V3(sin(camera.yaw), 0, -cos(camera.yaw))
    camera.position += (right * local.x + forward * local.z) * gain
  }
  func advance(_ seconds: Float, running: Bool) {}
  func interact() throws -> String { "" }
  func checkpoint() throws -> Data { try SimulationCoding.encode(camera) }
  func restore(_ data: Data) throws { camera = try JSONDecoder().decode(PlayerCamera.self, from: data) }
  func validate() throws { try SimulationCoding.validateCamera(camera) }
}
final class HarnessTests: XCTestCase {
  var project: TestProject {
    TestProject(
      id: "counter", product: "Counter", fixtures: ["zero"], tests: [],
      create: { _, _ in CounterWorld() })
  }
  var gainProject: TestProject {
    TestProject(
      id: "gain", product: "Gain", fixtures: ["flight", "blocked"], tests: [],
      create: { fixture, _ in GainWorld(gain: 12, blocked: fixture == "blocked") })
  }
  func testBookmarksReplayAndAssertionFailure() throws {
    let test = GameTest("bookmark", fixture: "zero") {
      TestStep.advance(3)
      TestStep.save("three")
      TestStep.advance(5)
      TestStep.load("three")
      TestStep.expect("count", "3")
      TestStep.expect("count", "99")
    }
    let run = try ScenarioRunner.run(test, project: project)
    XCTAssertFalse(run.passed)
    XCTAssertEqual(run.failureStep, 5)
    XCTAssertNil(try ScenarioRunner.replay(run, project: project))
    let reduced = try DeterministicTesting.minimize(run, project: project, attempts: 30)
    XCTAssertLessThan(reduced.test.steps.count, test.steps.count)
    XCTAssertEqual(reduced.failure, run.failure)
  }
  func testReopenUsesFreshProjectInstanceAndReplays() throws {
    var creations = 0
    let restartProject = TestProject(
      id: "restart", product: "Restart", fixtures: ["zero"], tests: [],
      create: { _, _ in
        creations += 1
        return CounterWorld()
      })
    let test = GameTest("fresh-instance-reopen", fixture: "zero") {
      TestStep.advance(3)
      TestStep.save("destination")
      TestStep.advance(2)
      TestStep.reopen("destination")
      TestStep.expect("count", "3")
    }

    let run = try ScenarioRunner.run(test, project: restartProject)
    XCTAssertTrue(run.passed)
    XCTAssertEqual(creations, 2)
    XCTAssertEqual(run.frames[3].processReopen, true)
    XCTAssertNotNil(run.frames[3].restore)
    XCTAssertNil(try ScenarioRunner.replay(run, project: restartProject))
    XCTAssertEqual(creations, 4)
  }
  func testTraceFrameWithoutProcessReopenMarkerStillDecodes() throws {
    let frame = TraceFrame(
      index: 0, label: "legacy", actions: [], restore: nil, state: Data("state".utf8),
      observations: [:], capture: nil, actionError: nil)
    let encoded = try JSONEncoder().encode(frame)
    var object = try XCTUnwrap(
      JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    object.removeValue(forKey: "processReopen")
    let legacy = try JSONSerialization.data(withJSONObject: object)
    XCTAssertNil(try JSONDecoder().decode(TraceFrame.self, from: legacy).processReopen)
  }
  func testDivergenceNamesTheField() throws {
    var run = try ScenarioRunner.run(
      GameTest("one", fixture: "zero") { TestStep.advance(1) }, project: project)
    run.frames[0].state = try SimulationCoding.encode(
      CounterWorld.State(camera: PlayerCamera(position: .zero), count: 2))
    XCTAssertTrue(try ScenarioRunner.replay(run, project: project)!.contains("count"))
  }
  func testMalformedRestoreIsAtomic() throws {
    let world = CounterWorld()
    world.count = 5
    let before = try world.checkpoint()
    XCTAssertThrowsError(try world.restore(Data("{\"count\":-1}".utf8)))
    XCTAssertEqual(try world.checkpoint(), before)
  }
  func testInvalidScenarioBoundsFailWithoutTrapping() throws {
    for step: TestStep in [
      TestStep.range("count", 10, 0), TestStep.advance(-1), TestStep.walk(x: 0, z: 0, maxTicks: 0),
    ] {
      let run = try ScenarioRunner.run(
        GameTest("invalid", fixture: "zero") { step }, project: project)
      XCTAssertFalse(run.passed)
    }
  }
  func testWalkLearnsRecordedGainForExactFlightArrivalAndKeepsBlockedPathsFailing() throws {
    let endpoint = GameTest("gain-endpoint", fixture: "flight") {
      // The initial 5.5/60 local input moves 1.1 m. A target at 0.7 m used to oscillate
      // between 0 and 1.1 because the generic runner did not observe that production gain.
      TestStep.walk(x: 0, z: -0.7, maxTicks: 4)
    }
    let arrived = try ScenarioRunner.run(endpoint, project: gainProject)
    XCTAssertTrue(arrived.passed)
    let moves = arrived.frames[0].actions.compactMap { action -> Float? in
      guard case let .move(delta) = action else { return nil }
      return delta.z
    }
    XCTAssertEqual(moves.count, 2)
    XCTAssertEqual(moves[0], 5.5 / 60, accuracy: 0.000_001)
    XCTAssertEqual(moves[1], 0.4 / 12, accuracy: 0.000_001)
    XCTAssertNil(try ScenarioRunner.replay(arrived, project: gainProject))

    let blocked = try ScenarioRunner.run(
      GameTest("blocked-endpoint", fixture: "blocked") {
        TestStep.walk(x: 0, z: -0.7, maxTicks: 4)
      }, project: gainProject)
    XCTAssertFalse(blocked.passed)
    XCTAssertTrue(blocked.failure?.contains("Walk did not reach") == true)
  }
  func testUnsupportedSemanticActionsRejectWithoutChangingState() throws {
    let world = CounterWorld()
    let before = try world.checkpoint()
    XCTAssertThrowsError(try world.apply(.request("hello")))
    XCTAssertEqual(try world.checkpoint(), before)
    XCTAssertThrowsError(try world.apply(.externalDecision(Data("{}".utf8))))
    XCTAssertEqual(try world.checkpoint(), before)
    XCTAssertThrowsError(try world.apply(.externalDecision(Data(repeating: 0, count: 16_385))))
    XCTAssertEqual(try world.checkpoint(), before)
    XCTAssertThrowsError(try world.apply(.control("flowers")))
    XCTAssertEqual(try world.checkpoint(), before)
  }
  func testExistingSimulationActionEncodingRemainsDecodable() throws {
    let encoded = try JSONEncoder().encode(SimulationAction.interact)
    XCTAssertEqual(try JSONDecoder().decode(SimulationAction.self, from: encoded), .interact)
  }
  func testCameraSafetyEnvelopeAllowsWideGeographyButKeepsVerticalLimit() throws {
    try SimulationCoding.validateCamera(PlayerCamera(position: V3(100_000, 1000, -100_000)))
    XCTAssertThrowsError(
      try SimulationCoding.validateCamera(PlayerCamera(position: V3(100_001, 0, 0))))
    XCTAssertThrowsError(
      try SimulationCoding.validateCamera(PlayerCamera(position: V3(0, 1001, 0))))
  }
  func testDeterminismCampaign() throws {
    XCTAssertTrue(try DeterministicTesting.campaign(project: project, seeds: 3, ticks: 150).passed)
  }
}
