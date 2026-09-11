import FieldCore
import Foundation
import SimulationCore

public struct CampaignResult: Codable {
  public var owner: String
  public var startSeed: UInt32
  public var seeds: Int
  public var ticksPerSeed: Int
  public var passed = true
  public var elapsedMilliseconds: Double = 0
  public var coverage: [String: Int] = [:]
  public var failure: RunArtifact?
  public var minimized: RunArtifact?
  public var replayDifference: String?
}
public enum DeterministicTesting {
  /// Independent input RNG; game RNG is exclusively owned by the production snapshot.
  public static func campaign(project: TestProject, seeds: Int, ticks: Int, startSeed: UInt32 = 1)
    throws -> CampaignResult
  {
    guard !project.fixtures.isEmpty, (1...10000).contains(seeds), (1...36000).contains(ticks) else {
      throw SimulationFailure.invalid("DST bounds: 1...10000 seeds, 1...36000 ticks")
    }
    var report = CampaignResult(
      owner: project.id, startSeed: startSeed, seeds: seeds, ticksPerSeed: ticks)
    let start = DispatchTime.now().uptimeNanoseconds
    for offset in 0..<seeds {
      let seed = startSeed &+ UInt32(offset)
      let fixture = project.fixtures[offset % project.fixtures.count]
      let world = try project.create(fixture, seed)
      let twin = try project.create(fixture, seed)
      let initial = try world.checkpoint()
      try twin.restore(initial)
      var rng = SeededRandom(seed: UInt64(seed))
      var steps: [TestStep] = []
      var previous = world.observations
      for tick in 0..<ticks {
        var actions: [SimulationAction] = []
        if tick % 11 == 0 {
          actions.append(.move(V3(rng.range(-0.18, 0.18), 0, rng.range(-0.22, 0.22))))
        }
        if tick % 37 == 0 {
          actions.append(
            .look(yaw: world.camera.yaw + rng.range(-0.25, 0.25), pitch: rng.range(-0.25, 0.25)))
        }
        if tick % 29 == 0 { actions.append(.interact) }
        if world.observations["flashlight"] != nil && tick % 47 == 0 {
          actions.append(.flashlight(rng.next() > 0.35))
        }
        actions.append(.tick(running: rng.next() < 0.15))
        do {
          for action in actions {
            steps.append(.action(action))
            try world.apply(action)
            try twin.apply(action)
            let actual = try world.checkpoint()
            let expected = try twin.checkpoint()
            if let diff = TestDigest.firstDifference(expected, actual) {
              throw SimulationFailure.invalid("First divergence at tick \(tick): \(diff)")
            }
            for key in ["phase", "encounter", "behavior"] {
              if let value = world.observations[key] {
                report.coverage[key + ":" + value, default: 0] += 1
                if let old = previous[key], old != value {
                  report.coverage[key + ":" + old + "→" + value, default: 0] += 1
                }
              }
            }
            previous = world.observations
          }
          // Alter the restore schedule without altering simulated time or random inputs.
          if tick % 73 == 0 { try twin.restore(world.checkpoint()) }
          // Malformed saved state must fail atomically, never reset the game.
          if tick % 137 == 0 {
            let before = try world.checkpoint()
            var rejected = false
            do { try world.restore(Data("{\"version\":999}".utf8)) } catch { rejected = true }
            guard rejected, try world.checkpoint() == before else {
              throw SimulationFailure.invalid("Invalid restore changed valid state at tick \(tick)")
            }
          }
        } catch {
          var test = GameTest("dst-\(seed)", fixture: fixture) {}
          test.steps = steps
          var failed = try ScenarioRunner.run(test, project: project, seed: seed)
          if failed.passed {
            failed.passed = false
            failed.failure = error.localizedDescription
            failed.failureStep = steps.count - 1
          }
          report.passed = false
          report.failure = failed
          report.replayDifference = error.localizedDescription
          report.minimized = try minimize(failed, project: project, attempts: 40)
          report.elapsedMilliseconds = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
          return report
        }
      }
    }
    report.elapsedMilliseconds = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
    return report
  }
  /// Bounded delta debugging. Retain the same failure message to avoid replacing it with a setup error.
  public static func minimize(_ failure: RunArtifact, project: TestProject, attempts: Int) throws
    -> RunArtifact
  {
    var best = failure
    var chunk = max(1, failure.test.steps.count / 2)
    var remaining = attempts
    while chunk > 0 && remaining > 0 {
      var index = 0
      var changed = false
      while index < best.test.steps.count && remaining > 0 {
        var candidate = best.test
        candidate.steps.removeSubrange(index..<min(index + chunk, candidate.steps.count))
        remaining -= 1
        let run = try ScenarioRunner.run(candidate, project: project, seed: best.seed)
        if !run.passed, run.failure == failure.failure {
          best = run
          changed = true
        } else {
          index += chunk
        }
      }
      if !changed { chunk /= 2 }
    }
    return best
  }
}
