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
    try loaded.state.initializeLivingWorld()
    state = loaded.state
    message =
      loaded.recovered
      ? "Recovered your previous expedition save."
      : "Wander, watch, and make yourself at home. T reaches out to nearby wildlife."
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
    var candidate = state
    candidate.advance(
      seconds: seconds, movingQuickly: running, visible: visible, motion: motion,
      perceived: perceived, canTraverse: canTraverse)
    state = candidate
    secondsSinceSave += seconds
    if secondsSinceSave >= 5 {
      secondsSinceSave = 0
      do { try save() } catch { onMessage?("Save failed: \(error.localizedDescription)") }
    }
  }

  /// Called after the living population tick, so the autosave is one coherent tick.
  public func advanceLivingClock(_ seconds: Float) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    state.advanceClock(seconds: seconds)
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

  /// Parses the authored fallback vocabulary, then persists the validated
  /// semantic action before committing it to the live expedition.
  public func addressCreature(_ text: String, visible: Bool) throws -> String {
    guard text.count <= 160 else {
      throw SimulationFailure.invalid("Creature requests are limited to 160 characters")
    }
    guard let request = AnimalRequestParser.parse(text) else { return "No request understood." }
    var candidate = state
    _ = try candidate.addressCreature(request, visible: visible)
    try store?.save(candidate)
    state = candidate
    let result = ""
    message = result
    onMessage?(result)
    return result
  }

  @discardableResult public func applyNature(
    _ command: HabitatGarden.Command, expectedRevision: UInt64? = nil
  ) throws -> HabitatGarden.PatchID {
    var candidate = state
    let id = try candidate.applyNature(command, expectedRevision: expectedRevision)
    try store?.save(candidate)
    state = candidate
    return id
  }

  /// Persist before publishing all user-authored living-world changes.
  @discardableResult public func editLiving<T>(_ edit: (inout Expedition) throws -> T) throws -> T {
    var candidate = state
    try candidate.initializeLivingWorld()
    let result = try edit(&candidate)
    try candidate.validate()
    try store?.save(candidate)
    state = candidate
    return result
  }

  /// Fixed-step simulation changes are autosaved by advance; no external decision occurs here.
  public func simulateLiving(_ edit: (inout Expedition) -> Void) {
    var candidate = state
    edit(&candidate)
    state = candidate
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
    var loaded = try candidate.load()
    try loaded.state.initializeLivingWorld()
    try save()
    self.store = candidate
    state = loaded.state
    secondsSinceSave = 0
  }

  public var snapshot: [String: Any] {
    let memory = state.relationship.encounters.last
    return [
      "terrainRecipe": state.resolvedTerrainRecipe.identity,
      "phase": state.phase.rawValue, "behavior": state.creatureState.state,
      "behaviorReason": state.creatureState.reason, "fear": state.creatureState.fear,
      "creatureID": Expedition.creatureID,
      "creaturePosition": [state.creaturePosition.x, state.creaturePosition.y],
      "home": [Expedition.home.x, Expedition.home.y],
      "signs": Expedition.signs.map { [$0.x, $0.y] },
      "discoveredSigns": state.discoveredSigns.sorted(), "trust": state.trust,
      "temperament": state.relationship.preferences.temperament.rawValue,
      "favoriteRequest": state.relationship.preferences.favoriteRequest.rawValue,
      "companionWilling": state.relationship.preferences.companionWilling,
      "relationshipMemoryCount": state.relationship.encounters.count,
      "lastCreatureRequest": memory?.request.rawValue ?? "",
      "lastCreatureResponse": memory?.outcome.rawValue ?? "",
      "habitat": state.habitat,
      "gardenRevision": state.garden?.revision ?? 0,
      "gardenPatchCount": state.garden?.patches.count ?? 0,
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
    var s = try JSONDecoder().decode(Memory.self, from: data)
    try s.state.initializeLivingWorld()
    try s.state.validate()
    guard s.secondsSinceSave.isFinite, (0...5.1).contains(s.secondsSinceSave) else {
      throw ExpeditionError.invalidSave
    }
    state = s.state
    secondsSinceSave = s.secondsSinceSave
    message = s.message
  }

}
