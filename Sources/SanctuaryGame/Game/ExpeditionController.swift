import FieldCore
import Foundation
import SanctuaryContent
import simd

/// Owns the playable expedition and its persistence. UI and agents share its actions.
final class ExpeditionController {
  private(set) var state: Expedition
  private(set) var store: ExpeditionStore
  private var secondsSinceSave: Float = 0
  var message: String
  var onMessage: ((String) -> Void)?

  init(root: URL) throws {
    store = ExpeditionStore(url: root.appendingPathComponent("saves/expedition.json"))
    let loaded = try store.load()
    state = loaded.state
    message =
      loaded.recovered
      ? "Recovered your previous expedition save."
      : "A pale trail waits along the northern path. E examines things nearby."
  }

  func updatePlayer(_ camera: V3, yaw: Float, pitch: Float) {
    state.player = SIMD2(camera.x, camera.z)
    state.yaw = yaw
    state.pitch = pitch
  }

  func advance(_ seconds: Float, running: Bool, visible: Bool) {
    state.advance(seconds: seconds, movingQuickly: running, visible: visible)
    secondsSinceSave += seconds
    if secondsSinceSave >= 5 {
      secondsSinceSave = 0
      do { try save() } catch { onMessage?("Save failed: \(error.localizedDescription)") }
    }
  }

  func interact(visible: Bool) throws -> String {
    // Persist the candidate first. A failed save must not silently lose a rescue.
    var candidate = state
    let result = candidate.interact(visible: visible)
    try store.save(candidate)
    state = candidate
    message = result
    onMessage?(result)
    return result
  }

  func save() throws { try store.save(state) }

  /// Named slots let tools test a fresh expedition without resetting the player's save.
  func loadSlot(_ name: String) throws {
    guard !name.isEmpty, name.count <= 48,
      name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") })
    else {
      throw RuntimeError.message("Save slot must contain 1–48 letters, digits or hyphens")
    }
    let candidate = ExpeditionStore(
      url: store.url.deletingLastPathComponent().appendingPathComponent(name + ".json"))
    let loaded = try candidate.load()
    try save()
    store = candidate
    state = loaded.state
    secondsSinceSave = 0
  }

  var snapshot: [String: Any] {
    [
      "phase": state.phase.rawValue, "creatureID": Expedition.creatureID,
      "creaturePosition": [state.creaturePosition.x, state.creaturePosition.y],
      "home": [Expedition.home.x, Expedition.home.y],
      "signs": Expedition.signs.map { [$0.x, $0.y] },
      "discoveredSigns": state.discoveredSigns.sorted(), "trust": state.trust,
      "habitat": state.habitat,
      "age": state.age, "legendaryHint": state.legendaryHint, "objective": state.objective,
      "prompt": state.prompt ?? "", "saveSlot": store.url.deletingPathExtension().lastPathComponent,
      "message": message,
    ]
  }
}

extension SanctuarySession {
  var creatureVisible: Bool {
    guard let expedition else { return false }
    let p = expedition.state.creaturePosition
    let target = V3(p.x, world.terrain.height(p.x, p.y) + 0.8, p.y)
    let delta = target - camera
    guard length_squared(delta) > 0.001, length_squared(delta) < 100,
      dot(normalize(delta), forward) > 0.3
    else { return false }
    for i in 1..<20 {
      let q = camera + delta * (Float(i) / 20)
      if q.y < world.terrain.height(q.x, q.z) { return false }
      for index in world.collisionGrid.candidates(at: q) where world.solids[index].value(q) < 0 {
        return false
      }
    }
    return true
  }
  func syncExpeditionPlayer() { expedition?.updatePlayer(camera, yaw: yaw, pitch: pitch) }
  func interact() throws -> String {
    guard scene == "garden", let expedition else {
      throw RuntimeError.message("Interactions are available in the expedition")
    }
    syncExpeditionPlayer()
    return try expedition.interact(visible: creatureVisible)
  }
  func loadExpeditionSlot(_ name: String) throws {
    guard let expedition else {
      throw RuntimeError.message("Save slots are only available in the game")
    }
    syncExpeditionPlayer()
    try expedition.loadSlot(name)
    let state = expedition.state
    camera = V3(
      state.player.x, world.terrain.height(state.player.x, state.player.y) + 1.72, state.player.y)
    yaw = state.yaw
    pitch = state.pitch
    keys.removeAll()
  }
}
