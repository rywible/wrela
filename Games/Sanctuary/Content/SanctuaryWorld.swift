import FieldCore
import Foundation
import SimulationCore
import simd

public final class SanctuaryWorld: GameSimulation {

  public var world: SanctuaryLayout
  public let controller: ExpeditionController
  var expedition: ExpeditionController? { controller }
  public var motion = CreatureMotion()
  public var podScale: Float = 1
  public var camera: PlayerCamera
  var position: V3 {
    get { camera.position }
    set { camera.position = newValue }
  }
  var yaw: Float {
    get { camera.yaw }
    set { camera.yaw = newValue }
  }
  var pitch: Float {
    get { camera.pitch }
    set { camera.pitch = newValue }
  }
  var forward: V3 { camera.forward }
  public init(root: URL? = nil, seed: UInt32 = 17, parameters: [String: Float] = [:]) throws {
    world = SanctuaryLayout(parameters: parameters)
    controller = try ExpeditionController(root: root, seed: seed)
    let s = controller.state
    camera = PlayerCamera(
      position: V3(
        s.player.x, s.playerElevation ?? (world.terrain.height(s.player.x, s.player.y) + 1.72),
        s.player.y),
      yaw: s.yaw, pitch: s.pitch)
  }
  var podPosition: V3 {
    V3(
      5,
      world.terrain.height(5, 12) - SanctuaryLayout.podShape.bounds.min.y * podScale
        * SanctuaryLayout.podUnitScale, 12)
  }
  private struct Snapshot: Codable {
    var version = 1
    var expedition: Data
    var camera: PlayerCamera
    var podScale: Float
    var motion: CreatureMotion
  }
  public func checkpoint() throws -> Data {
    syncExpeditionPlayer()
    return try SimulationCoding.encode(
      Snapshot(
        expedition: controller.checkpoint(), camera: camera, podScale: podScale, motion: motion))
  }
  public func restore(_ data: Data) throws {
    let s = try JSONDecoder().decode(Snapshot.self, from: data)
    try SimulationCoding.validateCamera(s.camera)
    try s.motion.validate()
    guard s.version == 1, s.podScale.isFinite, (0.25...4).contains(s.podScale),
      abs(
        s.camera.position.y - world.terrain.height(s.camera.position.x, s.camera.position.z) - 1.72)
        < 0.4
    else { throw ExpeditionError.invalidSave }
    try controller.restore(s.expedition)
    camera = s.camera
    podScale = s.podScale
    motion = s.motion
  }
  public func validate() throws {
    syncExpeditionPlayer()
    try controller.state.validate()
    try SimulationCoding.validateCamera(camera)
  }
  public var observations: [String: String] {
    let s = controller.state
    return [
      "phase": s.phase.rawValue, "trust": String(s.trust), "habitat": String(s.habitat),
      "fear": String(s.creatureState.fear), "behavior": s.creatureState.state,
      "visible": String(creatureVisible), "creatureID": Expedition.creatureID,
      "signs": String(s.discoveredSigns.count), "x": String(position.x), "y": String(position.y),
      "z": String(position.z),
    ]
  }
  public func move(_ local: V3) {

    let right = V3(cos(yaw), 0, sin(yaw))
    let forward = V3(sin(yaw), 0, -cos(yaw))
    let delta = right * local.x + forward * local.z
    let count = max(1, Int(ceil(length(delta) / 0.15)))
    for _ in 0..<count {
      var p = position + delta / Float(count)
      p.x = clamp(p.x, -130, 130)
      p.z = clamp(p.z, -135, 120)
      var ground = world.terrain.height(p.x, p.z)
      for off in [V3(0.25, 0, 0), V3(-0.25, 0, 0), V3(0, 0, 0.25), V3(0, 0, -0.25)] {
        ground = max(ground, world.terrain.height(p.x + off.x, p.z + off.z))
      }
      p.y = ground + 1.72
      var nearby = world.collisionGrid.candidates(at: p).map { world.solids[$0] }
      nearby.append(
        Solid(
          shape: SanctuaryLayout.podShape, position: podPosition,
          scale: podScale * SanctuaryLayout.podUnitScale, name: "Seed pod"))
      for solid in nearby {
        for height: Float in [0.35, 1.0, 1.55] {
          let sample = V3(p.x, ground + height, p.z)
          let d = solid.value(sample)
          if d < 0.28 {
            let gradient = solid.shape.sample(at: (sample - solid.position) / solid.scale).gradient
            var n = V3(gradient.x, 0, gradient.z)
            if length_squared(n) > 0.000001 {
              n = normalize(n)
              p += n * (0.28 - d)
            }
          }
        }
      }
      p.y = max(world.terrain.height(p.x, p.z), ground) + 1.72
      position = p
    }
  }

  public func advance(_ seconds: Float, running: Bool) {
    syncExpeditionPlayer()
    expedition?.advance(
      seconds, running: running, visible: creatureVisible,
      motion: motion,
      perceived: creatureLineOfSight, canTraverse: creatureCanTraverse)
  }

  public func creatureCanTraverse(_ a: SIMD2<Float>, _ b: SIMD2<Float>) -> Bool {
    let start = world.terrain.height(a.x, a.y)
    for i in 0...6 {
      let p = a + (b - a) * Float(i) / 6
      let ground = world.terrain.height(p.x, p.y)
      if abs(ground - start) > 0.28 { return false }
      for height: Float in [0.3, 0.65] {
        let q = V3(p.x, ground + height, p.y)
        for index in world.collisionGrid.candidates(at: q) where world.solids[index].value(q) < 0.35
        { return false }
      }
    }
    return true
  }
  public var creatureVisible: Bool {
    guard let expedition else { return false }
    let p = expedition.state.creaturePosition
    let delta = V3(p.x, world.terrain.height(p.x, p.y) + 0.8, p.y) - position
    return length_squared(delta) > 0.001 && dot(normalize(delta), forward) > 0.3
      && creatureLineOfSight
  }
  public var creatureLineOfSight: Bool {
    guard let expedition else { return false }
    let p = expedition.state.creaturePosition
    let target = V3(p.x, world.terrain.height(p.x, p.y) + 0.8, p.y)
    let delta = target - position
    guard length_squared(delta) > 0.001, length_squared(delta) < 100
    else { return false }
    for i in 1..<20 {
      let q = position + delta * (Float(i) / 20)
      if q.y < world.terrain.height(q.x, q.z) { return false }
      for index in world.collisionGrid.candidates(at: q) where world.solids[index].value(q) < 0 {
        return false
      }
    }
    return true
  }
  public func syncExpeditionPlayer() { expedition?.updatePlayer(position, yaw: yaw, pitch: pitch) }
  public func interact() throws -> String {
    guard let expedition else {
      throw SimulationFailure.invalid("Interactions are available in the expedition")
    }
    syncExpeditionPlayer()
    return try expedition.interact(visible: creatureVisible)
  }
  public func loadExpeditionSlot(_ name: String) throws {
    guard let expedition else {
      throw SimulationFailure.invalid("Save slots are only available in the game")
    }
    syncExpeditionPlayer()
    try expedition.loadSlot(name)
    let state = expedition.state
    position = V3(
      state.player.x,
      state.playerElevation ?? (world.terrain.height(state.player.x, state.player.y) + 1.72),
      state.player.y)
    yaw = state.yaw
    pitch = state.pitch

  }
}
