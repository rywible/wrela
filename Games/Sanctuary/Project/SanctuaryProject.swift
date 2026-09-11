import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

package enum SanctuaryProject {
  package static func make() -> GameProject {
    let controls: [ScalarControl] = [
      .init("height", "Growth height · m", 5.6, 3...12),
      .init("width", "Crown width · m", 6, 2...10),
      .init("trunk", "Trunk radius · m", 0.24, 0.1...0.6),
      .init("spread", "Branch spread", 1, 0.4...2), .init("leaves", "Leaf count", 1800, 200...4000),
    ]
    var generators = [AssetGenerator]()
    for (id, name) in [
      ("tree", "Sanctuary tree"), ("branch", "Branch structure"), ("seed", "Seed pod"),
      ("stone", "Mossy stone"), ("calibration", "Material spheres"),
    ] {
      generators.append(
        AssetGenerator(
          id: id, name: name, controls: ["tree", "branch"].contains(id) ? controls : [],
          compile: { try $0.compileSanctuary() }))
    }
    generators.append(
      AssetGenerator(
        id: "frostling", name: "Frostling",
        controls: FrostlingRecipe.controls.map {
          ScalarControl($0.key, $0.title, $0.initial, $0.range)
        }, animation: animation, parts: FrostlingRecipe.parts))
    return GameProject(
      id: "sanctuary", name: "Sanctuary", defaultSubject: "tree",
      directory: ProjectContext.workspace.appendingPathComponent("Games/Sanctuary"),
      generators: generators,
      inspectSimulation: { data in
        let world = try SanctuaryWorld(parameters: AssetSource.load("tree").parameters)
        try world.restore(data)
        return SimulationInspection(
          subject: "frostling", actor: snapshot(world.controller.state.creatureState))
      }, create: { try SanctuaryExperience(graphics: $0) })
  }
  static func motion(_ p: MotionParameters) -> CreatureMotion {
    (try? p.decoded(CreatureMotion.self)) ?? CreatureMotion()
  }
  static func snapshot(_ actor: CreatureSimulation) -> BehaviorSnapshot {
    var s = BehaviorSnapshot()
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    s.data = (try? encoder.encode(actor)) ?? Data()
    s.position = actor.position
    s.target = actor.target
    s.yaw = actor.yaw
    s.state = actor.state
    s.reason = actor.reason
    s.fear = actor.fear
    s.tick = actor.tick
    s.phase = actor.phase
    s.events = actor.events.map { PreviewEvent(tick: $0.tick, state: $0.state, reason: $0.reason) }
    return s
  }
  static func actor(_ s: BehaviorSnapshot) -> CreatureSimulation {
    (try? JSONDecoder().decode(CreatureSimulation.self, from: s.data)) ?? CreatureSimulation()
  }
  static var animation: AnimationDefinition {
    let names = [
      "duration": "Hop duration · s", "stride": "Stride · m", "height": "Hop height · m",
      "crouch": "Crouch · m", "earFollow": "Ear follow-through", "breath": "Breathing · m",
      "blinkRate": "Blink frequency", "attention": "Idle attention",
    ]
    return AnimationDefinition(
      controls: CreatureMotion.ranges.keys.sorted().map {
        ScalarControl($0, names[$0] ?? $0, CreatureMotion().values[$0]!, CreatureMotion.ranges[$0]!)
      }, clips: ["idle", "hop"], scenarios: CreatureScenario.allCases.map(\.rawValue),
      signalName: "Fear",
      duration: { motion($0).duration },
      poses: { clip, time, state, params in
        let m = motion(params)
        let a = actor(state)
        let phase: Float? =
          clip == "hop"
          ? time.truncatingRemainder(dividingBy: m.duration) / m.duration
          : (clip == "behavior" ? a.phase : nil)
        return m.poses(
          time: time, phase: phase, alert: clip == "behavior" ? a.fear : 0,
          gaze: clip == "behavior" ? a.gaze : 0)
      },
      root: { clip, time, state, params in
        let m = motion(params)
        let a = actor(state)
        let phase: Float? =
          clip == "hop"
          ? time.truncatingRemainder(dividingBy: m.duration) / m.duration
          : (clip == "behavior" ? a.phase : nil)
        let p = clip == "behavior" ? a.position : .zero
        return transform(
          V3(p.x, phase.map { CreatureMotion.flight($0) * m.height } ?? 0, p.y), V3(repeating: 1),
          clip == "behavior" ? a.yaw : 0)
      }, blink: { motion($1).blink(at: $0) },
      initialize: { snapshot(CreatureSimulation(seed: $0)) },
      step: { state, input, params in
        var a = actor(state)
        var s = CreatureStimulus()
        s.player = input.player
        s.visible = input.visible
        s.running = input.running
        s.food = input.food
        s.obstacle = input.obstacle
        s.obstacleRadius = input.obstacleRadius
        a.step(s, motion: motion(params))
        return snapshot(a)
      },
      input: { scenario, time in
        let v = (CreatureScenario(rawValue: scenario) ?? .calm).stimulus(at: time)
        var s = PreviewInput()
        s.player = v.player
        s.visible = v.visible
        s.running = v.running
        s.food = v.food
        s.obstacle = v.obstacle
        s.obstacleRadius = v.obstacleRadius
        return s
      }, validate: { try motion($0).validate() })
  }
}
