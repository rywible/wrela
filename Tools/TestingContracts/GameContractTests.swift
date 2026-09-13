import CaveTesting
import Foundation
import SanctuaryTesting
import SimulationCore
import TestKit
import XCTest

final class GameContractTests: XCTestCase {
  let workspace = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
  func testProductionScenariosAndCrossInstanceCheckpoints() throws {
    for project in [
      CaveTesting.project(workspace: workspace), SanctuaryTesting.project(workspace: workspace),
    ] {
      // Long biome walks belong to the optimized scenario/native replay tier.
      // They still run with `scripts/test run/render --tag exploration`; repeating
      // all per-tick snapshots in debug would make every quick edit loop minutes long.
      for scenario in project.tests where !scenario.tags.contains("exploration") {
        let result = try ScenarioRunner.run(scenario, project: project)
        XCTAssertTrue(result.passed, "\(project.id)/\(scenario.name): \(result.failure ?? "")")
        XCTAssertNil(try ScenarioRunner.replay(result, project: project))
      }
    }
  }
  func testRejectedRestorePreservesEveryProductionField() throws {
    for project in [
      CaveTesting.project(workspace: workspace), SanctuaryTesting.project(workspace: workspace),
    ] {
      let world = try project.create(project.fixtures[0], 17)
      for _ in 0..<80 { try world.apply(.tick(running: false)) }
      let before = try world.checkpoint()
      var snapshot = try JSONSerialization.jsonObject(with: before) as! [String: Any]
      snapshot["version"] = 999
      XCTAssertThrowsError(try world.restore(JSONSerialization.data(withJSONObject: snapshot)))
      XCTAssertEqual(try world.checkpoint(), before)
    }
  }
}
