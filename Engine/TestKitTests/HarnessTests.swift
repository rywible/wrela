import FieldCore
import SimulationCore
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
final class HarnessTests: XCTestCase {
  var project: TestProject {
    TestProject(
      id: "counter", product: "Counter", fixtures: ["zero"], tests: [],
      create: { _, _ in CounterWorld() })
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
  func testDeterminismCampaign() throws {
    XCTAssertTrue(try DeterministicTesting.campaign(project: project, seeds: 3, ticks: 150).passed)
  }
}
