import CaveContent
import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

private struct CaveSave: Codable {
  var version = 1
  var state: CaveSimulation
  var camera: PlayerCamera
  var flashlight: Bool
}
final class CaveExperience: GameExperience {
  let graphics: MetalRenderer
  var world: CaveWorld
  var simulation: any GameSimulation { world }
  var rock: Shape {
    get { world.rock }
    set { world.rock = newValue }
  }
  var scenery: [GPUBatch]
  var watcher: [GPUBatch]
  var source: AssetSource
  var state: CaveSimulation {
    get { world.state }
    set { world.state = newValue }
  }
  var camera: PlayerCamera {
    get { world.camera }
    set { world.camera = newValue }
  }
  private var lightingStorage = GameLighting()
  var lighting: GameLighting {
    get {
      var value = lightingStorage
      value.lightEnabled = world.lightEnabled
      return value
    }
    set {
      lightingStorage = newValue
      world.lightEnabled = newValue.lightEnabled
    }
  }
  var slot = "expedition"
  var sinceSave: Float = 0
  var queuedAudio: [AudioCue] = []
  init(graphics: MetalRenderer) throws {
    self.graphics = graphics
    let cave = try AssetSource.load("cave")
    world = CaveWorld(
      width: cave.parameters["width"] ?? 1, ceiling: cave.parameters["ceiling"] ?? 1)
    scenery = try cave.compile().map(graphics.upload)
    source = try AssetSource.load("watcher")
    watcher = try source.compile().map(graphics.upload)
    func mesh(_ name: String, _ field: Shape, _ color: V3, _ metallic: Float = 0) throws -> GPUBatch
    {
      graphics.upload(
        SceneBatch(
          name: name, mesh: try Mesher.compile(field, resolution: 24),
          instances: [Instance(tint: color, kind: 7)], roughness: 0.45, metallic: metallic))
    }
    scenery.append(
      try mesh(
        "Survey beacon", .box(V3(0.22, 0.35, 0.16)).moved(CaveLayout.relic), V3(0.28, 0.37, 0.36),
        0.65))
    let chalk = try Mesher.compile(
      .capsule(V3(-0.25, 0.03, 0), V3(0.25, 0.03, 0), 0.025), resolution: 16)
    let marks: [V3] = [
      V3(0, 0, 4), V3(0, 0, -2), V3(0, 0, -8), V3(-1, 0, -14), V3(-4, 0, -20), V3(-4, 0, -25),
      V3(0, 0, -31), V3(3, 0, -36),
    ]
    scenery.append(
      graphics.upload(
        SceneBatch(
          name: "Chalk trail", mesh: chalk,
          instances: marks.map { Instance(position: $0, tint: V3(0.7, 0.72, 0.66), kind: 7) })))
    let glow = try mesh(
      "Beacon indicator", .sphere(0.055).moved(CaveLayout.relic + V3(0, 0.18, -0.17)),
      V3(0.1, 0.7, 0.72))
    scenery.append(glow)
    lighting.flashlight = true
    lighting.intensity = 75
    lighting.preset = "flashlight"
    lighting.wetness = 0.55
    try load(slot, saveCurrent: false)
  }
  var saveURL: URL { ProjectContext.dataRoot.appendingPathComponent("saves/\(slot).json") }
  func move(_ local: V3) { world.move(local) }
  var illuminated: Bool { world.illuminated }
  func step(_ seconds: Float, running: Bool) {
    let count = state.scares
    let old = state.encounter
    world.advance(seconds, running: running)
    if state.scares != count {
      queuedAudio.append(
        AudioCue(kind: "impact", position: state.monster + V3(0, 1, 0), gain: 0.35))
    }
    if old != "watching" && state.encounter == "watching" {
      queuedAudio.append(AudioCue(kind: "scrape", position: state.monster, gain: 0.12))
    }
    if Int(state.time * 60) % 480 == 1 {
      queuedAudio.append(
        AudioCue(kind: "drip", position: camera.position + V3(3, 2, -5), gain: 0.07))
    }
    sinceSave += seconds
    if sinceSave > 5 {
      sinceSave = 0
      try? save()
    }
  }
  func audioCues() -> [AudioCue] {
    defer { queuedAudio = [] }
    return queuedAudio
  }
  func items() -> [RenderItem] {
    var result = scenery.map { b -> RenderItem in
      var m = b.material
      if b.name == "Beacon indicator" { m.z = 1 }
      return RenderItem(batch: b, material: m)
    }
    if state.monsterVisible {
      let poses =
        source.animation?.poses(
          "behavior", state.time, CaveProject.snapshot(state), source.motion ?? MotionParameters())
        ?? [:]
      let matrices = PartRig.matrices(source.joints, poses: poses)
      let d = camera.position - state.monster
      let yaw = atan2(-d.x, -d.z)
      for b in watcher {
        var i = b.sourceInstances[0]
        i.model =
          transform(state.monster, V3(repeating: 1), yaw)
          * (matrices[b.name] ?? matrix_identity_float4x4) * i.model
        result.append(RenderItem(batch: b, instance: i))
      }
    }
    return result
  }
  var hud: GameHUD {
    GameHUD(
      title: state.phase == "escaped" ? "The Last Survey · escaped" : "The Last Survey",
      objective: state.phase == "searching"
        ? "Find the missing survey beacon. Follow the chalk marks into the cave."
        : state.phase == "returning"
          ? "Bring the beacon back to the entrance."
          : "The recording contains a second set of footsteps.", detail: state.reason,
      prompt: state.phase == "escaped"
        ? "Begin another survey"
        : state.phase == "searching" && distance(camera.position, CaveLayout.relic) < 3
          ? "Recover survey beacon" : nil)
  }
  var snapshot: [String: Any] {
    [
      "scene": "cave",
      "cave": [
        "phase": state.phase, "encounter": state.encounter, "reason": state.reason,
        "scares": state.scares, "time": state.time,
        "monster": [state.monster.x, state.monster.y, state.monster.z], "illuminated": illuminated,
        "flashlight": lighting.lightEnabled, "saveSlot": slot, "events": state.events,
      ], "groundHeight": 0, "seed": state.seed,
    ]
  }
  func interact() throws -> String {
    let before = try world.checkpoint()
    do {
      let message = try world.interact()
      try save()
      lighting.lightEnabled = world.lightEnabled
      return message
    } catch {
      try world.restore(before)
      throw error
    }
  }
  func write(_ state: CaveSimulation, _ camera: PlayerCamera, _ slot: String) throws {
    try state.validate()
    let url = ProjectContext.dataRoot.appendingPathComponent("saves/\(slot).json")
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try JSONEncoder().encode(
      CaveSave(state: state, camera: camera, flashlight: world.lightEnabled)
    ).write(to: url, options: .atomic)
  }
  func query(_ p: V3) throws -> [String: Any] {
    [
      "solidValue": rock.value(at: p),
      "visible": FieldQueries.visible(from: camera.position, to: p, solid: rock),
    ]
  }
  func save() throws { try write(state, camera, slot) }
  func load(_ name: String, saveCurrent: Bool = true) throws {
    guard !name.isEmpty, name.count <= 48,
      name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") })
    else { throw RuntimeError.message("Invalid save slot") }
    let url = ProjectContext.dataRoot.appendingPathComponent("saves/\(name).json")
    let saved: CaveSave
    if FileManager.default.fileExists(atPath: url.path) {
      saved = try JSONDecoder().decode(CaveSave.self, from: Data(contentsOf: url))
      try saved.state.validate()
      guard saved.version == 1,
        [
          saved.camera.position.x, saved.camera.position.y, saved.camera.position.z,
          saved.camera.yaw, saved.camera.pitch,
        ].allSatisfy(\.isFinite), abs(saved.camera.pitch) <= 1.35,
        rock.value(at: saved.camera.position) > 0.1
      else { throw RuntimeError.message("Invalid cave save camera") }
    } else {
      saved = CaveSave(
        state: CaveSimulation(), camera: PlayerCamera(position: CaveLayout.entrance),
        flashlight: true)
    }
    if saveCurrent { try save() }
    slot = name
    state = saved.state
    camera = saved.camera
    world.lightEnabled = saved.flashlight
    lighting.lightEnabled = saved.flashlight
  }
  func command(_ action: String, _ c: [String: Any]) throws {
    switch action {
    case "flashlight":
      world.lightEnabled = (c["value"] as? Bool) ?? !world.lightEnabled
      lighting.lightEnabled = world.lightEnabled
    case "saveExpedition": try save()
    case "expeditionSlot":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Slot name required")
      }
      try load(name)
    case "camera":
      guard rock.value(at: camera.position) > 0.28 else {
        throw RuntimeError.message("Camera must be inside a passage")
      }
    case "parameters": break
    case "reloadAssets":
      let candidate = try AssetSource.load("watcher")
      let environment = try AssetSource.load("cave")
      let solid = CaveLayout.rock(
        width: environment.parameters["width"] ?? 1, ceiling: environment.parameters["ceiling"] ?? 1
      )
      guard solid.value(at: camera.position) > 0.28 else {
        throw RuntimeError.message(
          "Edited cave blocks the player; move to the entrance before reloading")
      }
      let compiled = try candidate.compile().map(graphics.upload)
      let caveBatches = try environment.compile().map(graphics.upload)
      scenery =
        caveBatches + scenery.filter { $0.name != "Cave rock" && !$0.name.hasPrefix("Outcrop ") }
      rock = solid
      source = candidate
      watcher = compiled
    default: throw RuntimeError.message("Unknown cave action: \(action)")
    }
  }
}
