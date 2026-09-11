import CaveContent
import FieldCore
import Foundation
import SimulationCore
import TestKit
import simd

public enum CaveTesting {
  public static func project(workspace: URL) -> TestProject {
    TestProject(
      id: "cave", product: "Cave", fixtures: ["entrance", "watcher", "beacon", "return"],
      tests: [
        GameTest("beam-visibility", fixture: "watcher", tags: ["smoke", "render"]) {
          TestStep.action(.flashlight(false))
          TestStep.advance(60)
          TestStep.expect("scares", "0")
          TestStep.capture("dark")
          TestStep.action(.flashlight(true))
          TestStep.advance(5)
          TestStep.expect("illuminated", "true")
          TestStep.capture("watcher")
          TestStep.save("seen")
          TestStep.advance(30)
          TestStep.expect("scares", "1")
          TestStep.load("seen")
          TestStep.advance(30)
          TestStep.expect("scares", "1")
          TestStep.advance(120)
          TestStep.expect("encounter", "dormant")
        },
        GameTest("wall-collision", fixture: "entrance", tags: ["smoke", "physics"]) {
          TestStep.walk(x: 0, z: 0)
          TestStep.action(.move(V3(20, 0, 0)))
          TestStep.range("x", -3, 3)
          TestStep.expect("phase", "searching")
        },
        GameTest("beacon-return", fixture: "beacon", tags: ["smoke", "render"]) {
          TestStep.action(.interact)
          TestStep.expect("phase", "returning")
          TestStep.save("beacon")
          TestStep.walk(x: 3, z: -34)
          TestStep.walk(x: -4, z: -24)
          TestStep.walk(x: 0, z: -12)
          TestStep.walk(x: 0, z: 4)
          TestStep.expect("phase", "escaped")
          TestStep.range("scares", 1, 2)
          TestStep.capture("escape")
          TestStep.load("beacon")
          TestStep.expect("phase", "returning")
        },
        GameTest("complete-traversal", fixture: "entrance", tags: ["route", "render"]) {
          TestStep.walk(x: 0, z: -11)
          TestStep.walk(x: -4, z: -22)
          TestStep.walk(x: 3, z: -33)
          TestStep.walk(x: 3, z: -40)
          TestStep.action(.interact)
          TestStep.expect("phase", "returning")
          TestStep.capture("beacon")
          TestStep.walk(x: 3, z: -34)
          TestStep.walk(x: -4, z: -23)
          TestStep.walk(x: 0, z: -11)
          TestStep.walk(x: 0, z: 4)
          TestStep.expect("phase", "escaped")
        },
      ],
      workloads: [
        SimulationWorkload("one-actor-60-ticks", fixture: "watcher"),
        SimulationWorkload("sixteen-actors-60-ticks", fixture: "watcher", actors: 16),
      ],
      create: { fixture, seed in
        let data = try Data(
          contentsOf: workspace.appendingPathComponent("Games/Cave/Authoring/Assets/cave.json"))
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let p = json?["parameters"] as? [String: NSNumber] ?? [:]
        let w = CaveWorld(
          seed: seed, width: p["width"]?.floatValue ?? 1, ceiling: p["ceiling"]?.floatValue ?? 1)
        switch fixture {
        case "entrance": break
        case "watcher":
          w.camera = PlayerCamera(
            position: V3(0, 1.72, -13), yaw: atan2(w.state.monster.x, 5), pitch: 0.02)
        case "beacon": w.camera = PlayerCamera(position: V3(3, 1.72, -40), pitch: -0.2)
        case "return":
          w.camera = PlayerCamera(position: V3(3, 1.72, -40))
          _ = try w.interact()
          w.camera = PlayerCamera(position: V3(-2.6, 1.72, -26), yaw: -2.467)
        default: throw SimulationFailure.invalid("Unknown Cave fixture \(fixture)")
        }
        try w.validate()
        return w
      })
  }
}
