import FieldCore
import Foundation
import SimulationCore
import simd

/// The first game's rules. No rendering, UI, wall clock, or file access.
public struct Expedition: Codable, Equatable, Sendable {
  public enum Phase: String, Codable, Sendable { case searching, carrying, settled }
  public static let creatureID = "frostling-001"
  public static let home = SIMD2<Float>(0, 18)
  public static let den = SIMD2<Float>(-3, -83)
  public static let signs: [SIMD2<Float>] = [SIMD2(0, -2), SIMD2(-10, -32), SIMD2(-2, -61)]
  public static let signNotes = [
    "Frost on warm grass. Two small tracks head north, along the path.",
    "A tuft of pale fur. The tracks continue toward the old stone gate.",
    "Fresh frost beneath the gate. Something is sheltering just beyond it.",
  ]
  public private(set) var phase: Phase = .searching
  public private(set) var discoveredSigns: Set<Int> = []
  public private(set) var relationship: AnimalRelationship
  public var trust: Float { relationship.trust }
  public private(set) var habitat: Float = 0
  public private(set) var garden: HabitatGarden?
  /// Absent in existing saves; missing identity always resolves to the qualified legacy source.
  public var terrainRecipe: SanctuaryTerrainRecipe?
  public var resolvedTerrainRecipe: SanctuaryTerrainRecipe { terrainRecipe ?? .legacy }
  public var wildlife: WildlifePopulation?
  public var construction: PersonalConstruction?
  public var journey: SanctuaryJourney?
  public var boulders: BoulderArrangements?
  public var movedBoulders: BoulderArrangements { boulders ?? BoulderArrangements() }
  /// Optional so expeditions saved before surface response remain readable.
  public var groundContacts: SanctuaryGroundContacts?
  public var recentGroundContacts: SanctuaryGroundContacts {
    groundContacts ?? SanctuaryGroundContacts()
  }
  public var surfaceInfluenceSnapshot: SurfaceInfluenceSnapshot {
    recentGroundContacts.snapshot(at: age)
  }
  public var creatureDecisions: CreatureDecisionJournal?
  public var decisionJournal: CreatureDecisionJournal { creatureDecisions ?? CreatureDecisionJournal() }
  public var tools: SanctuaryToolSettings?
  public var craftTools: SanctuaryToolSettings { tools ?? SanctuaryToolSettings() }
  public var population: WildlifePopulation {
    wildlife ?? .initial()
  }
  public var buildings: PersonalConstruction { construction ?? PersonalConstruction() }
  public var travel: SanctuaryJourney { journey ?? SanctuaryJourney() }
  public mutating func initializeLivingWorld() throws {
    if wildlife == nil { wildlife = try .migrating(relationship: relationship, creature: creatureState) }
    if construction == nil { construction = PersonalConstruction() }
    if journey == nil { journey = SanctuaryJourney() }
  }
  public private(set) var age: Double = 0
  public private(set) var legendaryHint = false
  public private(set) var creature: CreatureSimulation?
  public var creatureState: CreatureSimulation {
    creature ?? CreatureSimulation(home: phase == .settled ? Self.home : Self.den)
  }
  /// Optional for legacy saves; world-space eye elevation preserves the collision-resolved pose.
  public var playerElevation: Float?
  public var player = SIMD2<Float>(0, 24)
  public var yaw: Float = 0
  public var pitch: Float = -0.04

  public init() { relationship = .baked(id: Self.creatureID) }
  public init(seed: UInt32) {
    relationship = .baked(id: Self.creatureID, seed: seed)
    creature = CreatureSimulation(home: Self.den, seed: seed)
  }

  public var creaturePosition: SIMD2<Float> {
    creatureState.position
  }

  public mutating func advanceClock(seconds: Float) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    age += Double(seconds)
  }

  public mutating func advance(
    seconds: Float, movingQuickly: Bool, visible: Bool = true,
    motion: CreatureMotion = CreatureMotion(), perceived: Bool? = nil,
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil
  ) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    age += Double(seconds)
    if phase != .carrying {
      var actor = creatureState
      var input = CreatureStimulus()
      input.player = player
      input.visible = perceived ?? visible
      input.running = movingQuickly
      input.food = nil
      for _ in 0..<Int((seconds * 60).rounded()) {
        actor.step(input, motion: motion, canTraverse: canTraverse)
      }
      creature = actor
    }
    if phase == .searching {
      let near = distance(player, creaturePosition) < 5
      if near && visible && !movingQuickly { relationship.observeCalmPresence(seconds: seconds) }
    }
    if phase == .settled { habitat = min(1, habitat + seconds / 18) }
  }

  public var objective: String {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 9 {
        return "A Frostling! Approach gently and let it get used to you."
      }
      return discoveredSigns.isEmpty
        ? "Find signs of wildlife along the northern path."
        : "Follow the frost traces toward and beyond the old gate."
    case .carrying:
      return "Bring your Frostling home. Prepare its garden inside the sanctuary stones."
    case .settled:
      return habitat < 1
        ? "Watch your Frostling awaken its frost garden."
        : "Your first resident is home. Visit it to learn what might live farther north."
    }
  }

  public var prompt: String? {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 4 {
        return trust >= 0.99 ? "Rescue Frostling" : "Let it know you are a friend"
      }
      if nearestUnreadSign != nil { return "Examine frost traces" }
    case .carrying:
      if distance(player, Self.home) < 7 { return "Prepare frost garden & release" }
    case .settled:
      if distance(player, creaturePosition) < 5 { return "Visit Frostling" }
    }
    return nil
  }

  private var nearestUnreadSign: Int? {
    Self.signs.indices.filter {
      !discoveredSigns.contains($0) && distance(player, Self.signs[$0]) < 4
    }
    .min { distance(player, Self.signs[$0]) < distance(player, Self.signs[$1]) }
  }

  /// Applies one typed request after spatial validation. Refusal is a real brain
  /// response and remembered outcome; malformed or impossible requests throw
  /// before changing state.
  @discardableResult public mutating func addressCreature(
    _ request: AnimalRequest, visible: Bool
  ) throws -> Bool {
    guard phase != .carrying else {
      throw SimulationFailure.invalid("The Frostling cannot move independently while in your care")
    }
    let range: Float = request == .greeting ? 6 : 8
    guard distance(player, creaturePosition) <= range else {
      throw SimulationFailure.invalid("The Frostling is too far away to notice that request")
    }
    guard visible else {
      throw SimulationFailure.invalid("The Frostling needs a clear view of your gesture")
    }
    let accepted = relationship.isWilling(to: request)
    var actor = creatureState
    actor.address(request, accepted: accepted, player: player)
    relationship.record(request, accepted: accepted, tick: actor.tick)
    creature = actor
    return accepted
  }

  @discardableResult public mutating func applyNature(
    _ command: HabitatGarden.Command, expectedRevision: UInt64? = nil
  ) throws -> HabitatGarden.PatchID {
    var candidate = garden ?? HabitatGarden()
    let id = try candidate.apply(
      command, expectedRevision: expectedRevision ?? candidate.revision, atPlaySeconds: age)
    garden = candidate
    return id
  }

  /// The native button, keyboard and agent protocol all call this same action.
  @discardableResult public mutating func interact(visible: Bool = true) -> String {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 4 {
        guard visible else { return "Move into a clear view of the Frostling." }
        guard trust >= 0.99 else {
          return "Stay close and move gently. Give it a few seconds to trust you."
        }
        phase = .carrying
        return "The Frostling nestles into your care. Take it back to the sanctuary."
      }
      if let sign = nearestUnreadSign {
        discoveredSigns.insert(sign)
        return Self.signNotes[sign]
      }
    case .carrying:
      if distance(player, Self.home) < 7 {
        phase = .settled
        var actor = creatureState
        actor.relocateHome(Self.home)
        creature = actor
        habitat = 0
        return "A home for your Frostling. Watch the ground bloom with frost."
      }
    case .settled:
      if distance(player, creaturePosition) < 5 {
        guard habitat >= 1 else {
          return "Your Frostling is settling in. Tiny ice flowers are opening around it."
        }
        legendaryHint = true
        return
          "It watches the northern ridge. Old stories speak of a great white creature that carries winter in its antlers…"
      }
    }
    return "Explore the path and look for pale frost traces."
  }

  enum CodingKeys: String, CodingKey {
    case phase, discoveredSigns, trust, relationship, habitat, garden, age, legendaryHint, creature,
      player, playerElevation, yaw, pitch, wildlife, construction, journey, tools, creatureDecisions,
      boulders, groundContacts, terrainRecipe
  }
  public func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(phase, forKey: .phase)
    try c.encode(discoveredSigns.sorted(), forKey: .discoveredSigns)
    try c.encode(trust, forKey: .trust)
    try c.encode(relationship, forKey: .relationship)
    try c.encode(habitat, forKey: .habitat)
    try c.encodeIfPresent(garden, forKey: .garden)
    try c.encodeIfPresent(terrainRecipe, forKey: .terrainRecipe)
    try c.encodeIfPresent(wildlife, forKey: .wildlife)
    try c.encodeIfPresent(construction, forKey: .construction)
    try c.encodeIfPresent(journey, forKey: .journey)
    try c.encodeIfPresent(boulders, forKey: .boulders)
    try c.encodeIfPresent(groundContacts, forKey: .groundContacts)
    try c.encodeIfPresent(creatureDecisions, forKey: .creatureDecisions)
    try c.encodeIfPresent(tools, forKey: .tools)
    try c.encode(age, forKey: .age)
    try c.encode(legendaryHint, forKey: .legendaryHint)
    try c.encodeIfPresent(creature, forKey: .creature)
    try c.encode(player, forKey: .player)
    try c.encodeIfPresent(playerElevation, forKey: .playerElevation)
    try c.encode(yaw, forKey: .yaw)
    try c.encode(pitch, forKey: .pitch)
  }
  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    terrainRecipe = try c.decodeIfPresent(SanctuaryTerrainRecipe.self, forKey: .terrainRecipe)
    try (terrainRecipe ?? .legacy).validate()
    phase = try c.decode(Phase.self, forKey: .phase)
    discoveredSigns = try c.decode(Set<Int>.self, forKey: .discoveredSigns)
    habitat = try c.decode(Float.self, forKey: .habitat)
    garden = try c.decodeIfPresent(HabitatGarden.self, forKey: .garden)
    wildlife = try c.decodeIfPresent(WildlifePopulation.self, forKey: .wildlife)
    construction = try c.decodeIfPresent(PersonalConstruction.self, forKey: .construction)
    journey = try c.decodeIfPresent(SanctuaryJourney.self, forKey: .journey)
    boulders = try c.decodeIfPresent(BoulderArrangements.self, forKey: .boulders)
    groundContacts = try c.decodeIfPresent(SanctuaryGroundContacts.self, forKey: .groundContacts)
    creatureDecisions = try c.decodeIfPresent(CreatureDecisionJournal.self, forKey: .creatureDecisions)
    tools = try c.decodeIfPresent(SanctuaryToolSettings.self, forKey: .tools)
    age = try c.decode(Double.self, forKey: .age)
    legendaryHint = try c.decode(Bool.self, forKey: .legendaryHint)
    creature = try c.decodeIfPresent(CreatureSimulation.self, forKey: .creature)
    player = try c.decode(SIMD2<Float>.self, forKey: .player)
    playerElevation = try c.decodeIfPresent(Float.self, forKey: .playerElevation)
    yaw = try c.decode(Float.self, forKey: .yaw)
    pitch = try c.decode(Float.self, forKey: .pitch)
    if let saved = try c.decodeIfPresent(AnimalRelationship.self, forKey: .relationship) {
      relationship = saved
    } else {
      relationship = .baked(
        id: Self.creatureID, trust: try c.decodeIfPresent(Float.self, forKey: .trust) ?? 0)
    }
    try validate()
  }
  public func validate() throws {
    try resolvedTerrainRecipe.validate()
    try creature?.validate()
    try relationship.validate()
    if let garden { try garden.validate(createdAtOrBefore: age) }
    try wildlife?.validate()
    try construction?.validate()
    try journey?.validate()
    try tools?.validate()
    try creatureDecisions?.validate()
    try boulders?.validate()
    try groundContacts?.validate(at: age, validAnimalIDs: Set(wildlife?.actors.map(\.id) ?? []))
    if let journey, let id = journey.companionID {
      guard let actor = wildlife?.actor(id: id),
        journey.mode == .riding ? actor.companion.rideEligible : actor.companion.flyEligible
      else { throw ExpeditionError.invalidSave }
    }
    if let creature, creature.home != (phase == .settled ? Self.home : Self.den) {
      throw ExpeditionError.invalidSave
    }
    guard playerElevation.map({ $0.isFinite && abs($0) <= 1000 }) ?? true, player.x.isFinite,
      player.y.isFinite, SanctuaryGeography.bounds.contains(player),
      yaw.isFinite, abs(yaw) < 100000, pitch.isFinite, (-1.35...1.35).contains(pitch),
      relationship.id == Self.creatureID, trust.isFinite, (0...1).contains(trust),
      habitat.isFinite, (0...1).contains(habitat),
      age.isFinite, (0...1e9).contains(age),
      discoveredSigns.allSatisfy(Self.signs.indices.contains),
      phase == .settled || (habitat == 0 && !legendaryHint)
    else {
      throw ExpeditionError.invalidSave
    }
  }
}

public enum ExpeditionError: LocalizedError {
  case invalidSave, unsupportedVersion, unreadableSave
  public var errorDescription: String? {
    switch self {
    case .invalidSave: return "The expedition save contains invalid state."
    case .unsupportedVersion: return "This expedition was saved by an unsupported version."
    case .unreadableSave:
      return "The expedition save could not be read. The original files have been preserved."
    }
  }
}
