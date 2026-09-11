import FieldCore
import Foundation
import SimulationCore
import simd

/// Owns the playable expedition and its persistence. UI and agents share its actions.
public final class ExpeditionController {
  public private(set) var state: Expedition
  public private(set) var store: ExpeditionStore?
  private var secondsSinceSave: Float = 0
  public var message: String
  public var onMessage: ((String) -> Void)?

  public init(root: URL? = nil, seed: UInt32 = 17) throws {
    store = root.map { ExpeditionStore(url: $0.appendingPathComponent("saves/expedition.json")) }
    var loaded = try store?.load() ?? (state: Expedition(), recovered: false)
    if root == nil { loaded.state = Expedition(seed: seed) }
    state = loaded.state
    message =
      loaded.recovered
      ? "Recovered your previous expedition save."
      : "A pale trail waits along the northern path. E examines things nearby."
  }

  public func updatePlayer(_ camera: V3, yaw: Float, pitch: Float) {
    state.player = SIMD2(camera.x, camera.z)
    state.playerElevation = camera.y
    state.yaw = yaw
    state.pitch = pitch
  }

  public func advance(
    _ seconds: Float, running: Bool, visible: Bool, motion: CreatureMotion = CreatureMotion(),
    perceived: Bool? = nil, canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil
  ) {
    state.advance(
      seconds: seconds, movingQuickly: running, visible: visible, motion: motion,
      perceived: perceived, canTraverse: canTraverse)
    secondsSinceSave += seconds
    if secondsSinceSave >= 5 {
      secondsSinceSave = 0
      do { try save() } catch { onMessage?("Save failed: \(error.localizedDescription)") }
    }
  }

  public func interact(visible: Bool) throws -> String {
    // Persist the candidate first. A failed save must not silently lose a rescue.
    var candidate = state
    let result = candidate.interact(visible: visible)
    try store?.save(candidate)
    state = candidate
    message = result
    onMessage?(result)
    return result
  }

  public func save() throws { try store?.save(state) }

  /// Named slots let tools test a fresh expedition without resetting the player's save.
  public func loadSlot(_ name: String) throws {
    guard !name.isEmpty, name.count <= 48,
      name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") })
    else {
      throw SimulationFailure.invalid("Save slot must contain 1–48 letters, digits or hyphens")
    }
    guard let store else {
      throw SimulationFailure.invalid("Named disk slots require a persistent simulation")
    }
    let candidate = ExpeditionStore(
      url: store.url.deletingLastPathComponent().appendingPathComponent(name + ".json"))
    let loaded = try candidate.load()
    try save()
    self.store = candidate
    state = loaded.state
    secondsSinceSave = 0
  }

  public var snapshot: [String: Any] {
    [
      "phase": state.phase.rawValue, "behavior": state.creatureState.state,
      "behaviorReason": state.creatureState.reason, "fear": state.creatureState.fear,
      "creatureID": Expedition.creatureID,
      "creaturePosition": [state.creaturePosition.x, state.creaturePosition.y],
      "home": [Expedition.home.x, Expedition.home.y],
      "signs": Expedition.signs.map { [$0.x, $0.y] },
      "discoveredSigns": state.discoveredSigns.sorted(), "trust": state.trust,
      "habitat": state.habitat,
      "age": state.age, "legendaryHint": state.legendaryHint, "objective": state.objective,
      "prompt": state.prompt ?? "",
      "saveSlot": store?.url.deletingPathExtension().lastPathComponent ?? "memory",
      "message": message,
    ]
  }
  public struct Memory: Codable {
    var state: Expedition
    var secondsSinceSave: Float
    var message: String
  }
  public func checkpoint() throws -> Data {
    try SimulationCoding.encode(
      Memory(state: state, secondsSinceSave: secondsSinceSave, message: message))
  }
  public func restore(_ data: Data) throws {
    let s = try JSONDecoder().decode(Memory.self, from: data)
    try s.state.validate()
    guard s.secondsSinceSave.isFinite, (0...5.1).contains(s.secondsSinceSave) else {
      throw ExpeditionError.invalidSave
    }
    state = s.state
    secondsSinceSave = s.secondsSinceSave
    message = s.message
  }

}
