import FieldCore
import Foundation
import SanctuaryContent
import SimulationCore
import TestKit
import simd

public enum SanctuaryTesting {
  public static func project(workspace: URL) -> TestProject {
    TestProject(
      id: "sanctuary", product: "Sanctuary", fixtures: ["arrival", "frostling", "home"],
      tests: [
        GameTest("rescue-trust", fixture: "frostling", tags: ["smoke", "render"]) {
          TestStep.action(.interact)
          TestStep.expect("phase", "searching")
          TestStep.advance(300)
          TestStep.range("trust", 0.99, 1)
          TestStep.capture("wild")
          TestStep.action(.interact)
          TestStep.expect("phase", "carrying")
          TestStep.save("rescued")
          TestStep.advance(60)
          TestStep.load("rescued")
          TestStep.expect("creatureID", "frostling-001")
          TestStep.expect("phase", "carrying")
        },
        GameTest("running-frightens", fixture: "frostling", tags: ["smoke", "behavior"]) {
          TestStep.advance(120, running: true)
          TestStep.range("fear", 0.9, 1)
          TestStep.range("trust", 0, 0.1)
          TestStep.action(.interact)
          TestStep.expect("phase", "searching")
        },
        GameTest("habitat-release", fixture: "home", tags: ["smoke", "render"]) {
          TestStep.action(.interact)
          TestStep.expect("phase", "settled")
          TestStep.capture("new-home")
          TestStep.advance(1200)
          TestStep.range("habitat", 0.99, 1)
          TestStep.capture("established-home")
        },
        GameTest("return-traversal", fixture: "frostling", tags: ["route", "render"]) {
          TestStep.advance(300)
          TestStep.action(.interact)
          TestStep.expect("phase", "carrying")
          TestStep.walk(x: -2, z: -68)
          TestStep.walk(x: 0, z: -55)
          TestStep.walk(x: -10, z: -32)
          TestStep.walk(x: 0, z: -2)
          TestStep.walk(x: 0, z: 21)
          TestStep.action(.interact)
          TestStep.expect("phase", "settled")
        },
      ],
      workloads: [
        SimulationWorkload("one-actor-60-ticks", fixture: "frostling"),
        SimulationWorkload("sixteen-actors-60-ticks", fixture: "frostling", actors: 16),
      ],
      create: { fixture, seed in
        let dir = workspace.appendingPathComponent("Games/Sanctuary/Authoring/Assets")
        let tree =
          try JSONSerialization.jsonObject(
            with: Data(contentsOf: dir.appendingPathComponent("tree.json"))) as? [String: Any]
        let parameters = (tree?["parameters"] as? [String: NSNumber] ?? [:]).mapValues(\.floatValue)
        let w = try SanctuaryWorld(seed: seed, parameters: parameters)
        let creature =
          try JSONSerialization.jsonObject(
            with: Data(contentsOf: dir.appendingPathComponent("frostling.json"))) as? [String: Any]
        if let motion = creature?["motion"] {
          w.motion = try JSONDecoder().decode(
            CreatureMotion.self, from: JSONSerialization.data(withJSONObject: motion))
        }
        if fixture != "arrival" {
          w.camera = PlayerCamera(
            position: V3(-3, w.world.terrain.height(-3, -80) + 1.72, -80), pitch: -0.2)
        }
        switch fixture {
        case "arrival", "frostling": break
        case "home":
          for _ in 0..<300 { try w.apply(.tick(running: false)) }
          _ = try w.interact()
          w.camera = PlayerCamera(
            position: V3(0, w.world.terrain.height(0, 21) + 1.72, 21), pitch: -0.3)
          w.syncExpeditionPlayer()
        default: throw SimulationFailure.invalid("Unknown Sanctuary fixture \(fixture)")
        }
        try w.validate()
        return w
      })
  }
}
