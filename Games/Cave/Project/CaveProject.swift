import CaveContent
import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

package enum CaveProject {
  package static func make() -> GameProject {
    let cave = AssetGenerator(
      id: "cave", name: "Limestone passages",
      controls: [
        .init("width", "Passage width", 1, 0.85...1.2),
        .init("ceiling", "Chamber height", 1, 0.8...1.25),
      ],
      views: [
        InspectionCamera("passage", CaveLayout.entrance, V3(0, 1.7, -10)),
        InspectionCamera("junction", V3(0, 1.7, -12), V3(-5, 1.7, -22)),
        InspectionCamera("chamber", V3(3, 1.7, -36), V3(3, 1.1, -43)),
      ],
      compile: { source in
        let field = CaveLayout.shell(
          width: source.parameters["width"] ?? 1, ceiling: source.parameters["ceiling"] ?? 1)
        let shell = SceneBatch(
          name: "Cave rock", mesh: try Mesher.compile(field, resolution: 128),
          instances: [Instance(tint: V3(0.38, 0.40, 0.39), kind: 5)], roughness: 0.84,
          doubleSided: true)
        // Compile small forms in their own bounds; a world-sized lattice cannot resolve their contacts.
        return [shell]
          + (try CaveLayout.outcrops.enumerated().map { index, shape in
            SceneBatch(
              name: "Outcrop \(index)", mesh: try Mesher.compile(shape, resolution: 32),
              instances: [Instance(tint: V3(0.38, 0.40, 0.39), kind: 5)], roughness: 0.84)
          })
      })
    let stone = AssetGenerator(
      id: "limestone", name: "Fractured limestone",
      controls: [.init("height", "Rock height", 1, 0.5...2)],
      parts: { p in
        [
          AssetPart(
            name: "rock",
            field: .ellipsoid(V3(0, 0.5, 0), V3(0.8, p["height"] ?? 1, 0.7)).blended(
              with: .ellipsoid(V3(0.45, 0.3, 0), V3(0.5, 0.7, 0.55)), radius: 0.12),
            color: [0.4, 0.43, 0.42], material: 5, roughness: 0.8)
        ]
      })
    let calibration = AssetGenerator(
      id: "calibration", name: "Material spheres",
      compile: { _ in
        try (0..<5).map { i in
          SceneBatch(
            name: "Sphere \(i)",
            mesh: try Mesher.compile(
              .sphere(0.45).moved(V3(Float(i - 2) * 1.1, 0.45, 0)), resolution: 24),
            instances: [Instance(tint: V3(repeating: 0.55), kind: 7)],
            roughness: [0.08, 0.25, 0.5, 0.9, 0.04][i], metallic: i == 4 ? 1 : 0)
        }
      })
    return GameProject(
      id: "cave", name: "The Last Survey", defaultSubject: "watcher",
      directory: ProjectContext.workspace.appendingPathComponent("Games/Cave"),
      generators: [
        AssetGenerator(
          id: "watcher", name: "The Watcher", controls: WatcherRecipe.controls,
          animation: animation, parts: WatcherRecipe.parts), cave, stone, calibration,
      ], usesAtmosphere: false,
      inspectSimulation: { data in
        let source = try AssetSource.load("cave")
        let world = CaveWorld(
          width: source.parameters["width"] ?? 1, ceiling: source.parameters["ceiling"] ?? 1)
        try world.restore(data)
        return SimulationInspection(subject: "watcher", actor: snapshot(world.state))
      }, create: { try CaveExperience(graphics: $0) })
  }
  static let animationControls: [ScalarControl] = [
    .init("duration", "Lunge duration · s", 0.9, 0.5...1.5), .init("sway", "Body sway", 1, 0...2),
    .init("reach", "Reaching motion", 1, 0...1.5),
  ]
  static func snapshot(_ actor: CaveSimulation) -> BehaviorSnapshot {
    var s = BehaviorSnapshot()
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    s.data = (try? encoder.encode(actor)) ?? Data()
    s.position = SIMD2(actor.monster.x, actor.monster.z)
    s.target = s.position
    s.state = actor.encounter
    s.reason = actor.reason
    s.tick = Int(actor.time * 60)
    s.fear = actor.seen
    s.events = actor.events.enumerated().map {
      PreviewEvent(tick: nil, state: "Event", reason: $0.element)
    }
    return s
  }
  static var animation: AnimationDefinition {
    AnimationDefinition(
      controls: animationControls, clips: ["idle", "lunge"],
      scenarios: ["beam", "dark", "occluded", "return"], signalName: "Beam exposure",
      duration: { $0.values["duration"] ?? 0.9 },
      poses: { clip, time, state, p in
        let sway = p.values["sway"] ?? 1
        let reach = p.values["reach"] ?? 1
        let phase =
          time.truncatingRemainder(dividingBy: p.values["duration"] ?? 0.9)
          / (p.values["duration"] ?? 0.9)
        let attack: Float = clip == "lunge" ? sin(phase * .pi) : state.state == "rush" ? 1 : 0
        return [
          "torso": JointPose(rotation: V3(-attack * 18, 0, sin(time * 0.7) * 2 * sway)),
          "mask": JointPose(
            rotation: V3(sin(time * 0.9) * 4 * sway, -sin(time * 0.4) * 9 * sway, 7 * sway)),
          "arm-left": JointPose(rotation: V3(attack * 95 * reach, 0, sin(time) * 3)),
          "arm-right": JointPose(rotation: V3(attack * 100 * reach, 0, -sin(time + 1) * 3)),
        ]
      },
      root: { clip, _, state, _ in
        clip == "behavior"
          ? transform(V3(state.position.x, 0, state.position.y), V3(repeating: 1), 0)
          : matrix_identity_float4x4
      }, initialize: { snapshot(CaveSimulation(seed: $0)) },
      step: { state, input, _ in
        var a =
          (try? JSONDecoder().decode(CaveSimulation.self, from: state.data)) ?? CaveSimulation()
        if input.values["return"] == 1 && a.phase == "searching" {
          a.phase = "returning"
          a.encounter = "watching"
          a.monster = V3(-2.4, 0, -18)
        }
        let player = V3(input.player.x, 1.72, input.player.y)
        let target = a.monster + V3(0, 1.8, 0)
        let visible =
          input.visible && (input.values["beam"] ?? 1) > 0
          && FieldQueries.visible(from: player, to: target, solid: CaveLayout.rock())
        a.step(player: player, forward: normalize(target - player), illuminated: visible)
        return snapshot(a)
      },
      input: { scenario, time in
        var s = PreviewInput()
        s.player = scenario == "return" ? SIMD2(-1, -15) : SIMD2(0, -13)
        s.visible = scenario != "occluded"
        s.values = [
          "beam": scenario == "dark" || time < 1 ? 0 : 1, "return": scenario == "return" ? 1 : 0,
        ]
        return s
      }, bounds: { _ in Bounds(V3(-6, 0, -22), V3(2, 3, -10)) },
      validate: { p in
        for (k, v) in p.values {
          guard let c = animationControls.first(where: { $0.key == k }), v.isFinite,
            c.range.contains(v)
          else { throw RuntimeError.message("Invalid Watcher motion") }
        }
      })
  }
}
