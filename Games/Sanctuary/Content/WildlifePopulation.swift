import Foundation
import simd

public enum WildlifeSpecies: String, CaseIterable, Codable, Hashable, Sendable {
  case frostling
  case sunhare
  case brookweaver
  case reedwalker
  case moonhart
  case cloudstepper
  case dunefox
  case canopyGlider
  case saltback
  case tidepooler
  case cloudRay
}

public enum WildlifeCapability: String, CaseIterable, Codable, Hashable, Sendable {
  case ride
  case fly
  case habitatHelp
  case moveBoulders
}

public enum WildlifeLifeStage: String, Codable, Equatable, Sendable {
  case young, adult
}

public enum WildlifeEvidence: String, Codable, Equatable, Sendable {
  case sighting, tracks, call, nest, silhouette
}

/// Authored scarcity for a particular persistent individual. Absence from the
/// characterization registry means the journal has no rarity claim to make.
public enum WildlifeRarity: String, Codable, Equatable, Sendable {
  case uncommon, rare
}

/// Optional discovery metadata keyed by a stable actor identity. It is source
/// metadata rather than save state, so existing saves gain the same authored
/// characterization without a migration or a synthetic discovery event.
public struct WildlifeDiscoveryCharacterization: Codable, Equatable, Sendable {
  public let animalID: String
  public let rarity: WildlifeRarity
  public let evidence: WildlifeEvidence

  public init(animalID: String, rarity: WildlifeRarity, evidence: WildlifeEvidence) {
    self.animalID = animalID
    self.rarity = rarity
    self.evidence = evidence
  }
}

public enum WildlifeHabitatActivity: String, Codable, Equatable, Sendable {
  case gatheringFrost
  case turningMeadow
  case rebuildingDam
  case tendingReeds
  case clearingShore
  case openingTrail
  case shadingDen
  case plantingCanopy
  case movingDriftwood
  case sortingPools
  case guidingShoal
}

public struct WildlifeSpeciesProfile: Codable, Equatable, Sendable {
  public let biomes: [SanctuaryBiome]
  public let capabilities: [WildlifeCapability]
  public let preferredPlanting: HabitatGarden.Planting?
  public let avoidedPlanting: HabitatGarden.Planting?
  public let habitatActivity: WildlifeHabitatActivity
  public let hiddenEvidence: WildlifeEvidence

  public init(
    biomes: [SanctuaryBiome], capabilities: [WildlifeCapability],
    preferredPlanting: HabitatGarden.Planting?, avoidedPlanting: HabitatGarden.Planting?,
    habitatActivity: WildlifeHabitatActivity, hiddenEvidence: WildlifeEvidence
  ) {
    self.biomes = biomes
    self.capabilities = capabilities
    self.preferredPlanting = preferredPlanting
    self.avoidedPlanting = avoidedPlanting
    self.habitatActivity = habitatActivity
    self.hiddenEvidence = hiddenEvidence
  }
}

extension WildlifeSpecies {
  public var profile: WildlifeSpeciesProfile {
    switch self {
    case .frostling:
      return .init(
        biomes: [.woodland, .alpine], capabilities: [.habitatHelp],
        preferredPlanting: .flowers, avoidedPlanting: .shallowWater,
        habitatActivity: .gatheringFrost, hiddenEvidence: .tracks)
    case .sunhare:
      return .init(
        biomes: [.woodland, .meadow], capabilities: [], preferredPlanting: .flowers,
        avoidedPlanting: .shallowWater, habitatActivity: .turningMeadow,
        hiddenEvidence: .tracks)
    case .brookweaver:
      return .init(
        biomes: [.creek], capabilities: [.habitatHelp], preferredPlanting: .shallowWater,
        avoidedPlanting: nil, habitatActivity: .rebuildingDam, hiddenEvidence: .call)
    case .reedwalker:
      return .init(
        biomes: [.wetland], capabilities: [.habitatHelp], preferredPlanting: .reeds,
        avoidedPlanting: nil, habitatActivity: .tendingReeds, hiddenEvidence: .nest)
    case .moonhart:
      return .init(
        biomes: [.woodland, .meadow, .lake],
        capabilities: [.ride, .habitatHelp, .moveBoulders], preferredPlanting: .reeds,
        avoidedPlanting: nil, habitatActivity: .clearingShore, hiddenEvidence: .silhouette)
    case .cloudstepper:
      return .init(
        biomes: [.alpine], capabilities: [.ride, .habitatHelp, .moveBoulders],
        preferredPlanting: .grove,
        avoidedPlanting: .shallowWater, habitatActivity: .openingTrail,
        hiddenEvidence: .call)
    case .dunefox:
      return .init(
        biomes: [.desert], capabilities: [], preferredPlanting: .grove,
        avoidedPlanting: .shallowWater, habitatActivity: .shadingDen,
        hiddenEvidence: .tracks)
    case .canopyGlider:
      return .init(
        biomes: [.rainforest], capabilities: [.fly, .habitatHelp], preferredPlanting: .grove,
        avoidedPlanting: nil, habitatActivity: .plantingCanopy, hiddenEvidence: .call)
    case .saltback:
      return .init(
        biomes: [.coast], capabilities: [.ride, .habitatHelp, .moveBoulders],
        preferredPlanting: .reeds,
        avoidedPlanting: nil, habitatActivity: .movingDriftwood, hiddenEvidence: .silhouette)
    case .tidepooler:
      return .init(
        biomes: [.tidepool], capabilities: [.habitatHelp], preferredPlanting: .shallowWater,
        avoidedPlanting: nil, habitatActivity: .sortingPools, hiddenEvidence: .tracks)
    case .cloudRay:
      return .init(
        biomes: [.ocean], capabilities: [.ride, .fly], preferredPlanting: .shallowWater,
        avoidedPlanting: nil, habitatActivity: .guidingShoal, hiddenEvidence: .silhouette)
    }
  }

  public var ecologyStructureKind: EcologyStructureKind {
    switch self {
    case .brookweaver: return .dam
    case .reedwalker, .canopyGlider, .tidepooler, .cloudRay: return .nest
    case .sunhare, .moonhart, .cloudstepper, .saltback: return .restingBed
    case .frostling, .dunefox: return .seedCache
    }
  }
}

public struct WildlifeHabitatWork: Codable, Equatable, Sendable {
  public let activity: WildlifeHabitatActivity
  public private(set) var progress: Float
  public private(set) var completed: Bool

  public init(activity: WildlifeHabitatActivity, progress: Float = 0, completed: Bool = false) {
    self.activity = activity
    self.progress = progress
    self.completed = completed
  }

  fileprivate mutating func advance(seconds: Float, habitatSupport: Float) {
    guard !completed, seconds > 0 else { return }
    let rate = 1 / (90 + 90 * (1 - min(1, max(0, habitatSupport))))
    progress = min(1, progress + seconds * rate)
    completed = progress >= 1
  }

  fileprivate mutating func restart() {
    progress = 0
    completed = false
  }

  fileprivate func validate() throws {
    guard progress.isFinite, (0...1).contains(progress), completed == (progress >= 1) else {
      throw WildlifePopulationError.invalidState
    }
  }
}

public struct WildlifeCompanionFacts: Codable, Equatable, Sendable {
  public let willing: Bool
  public let follows: Bool
  public let available: [WildlifeCapability]
  public let rideEligible: Bool
  public let flyEligible: Bool
  public let helpEligible: Bool

  public init(relationship: AnimalRelationship, capabilities: [WildlifeCapability]) {
    willing = relationship.preferences.companionWilling
    follows = relationship.isWilling(to: .follow)
    available = capabilities
    rideEligible = follows && capabilities.contains(.ride)
    flyEligible = follows && capabilities.contains(.fly)
    helpEligible = willing && relationship.trust >= 0.65 && capabilities.contains(.habitatHelp)
  }
}

public enum WildlifeSignal: String, Codable, Equatable, Sendable {
  case resting, watching, greeting, approaching, following, waiting, playing, refusing, fleeing
  case seekingHabitat, workingHabitat
}

public struct WildlifeObservation: Codable, Equatable, Sendable {
  public let id: String
  public let species: WildlifeSpecies
  public let biome: SanctuaryBiome
  public let position: SIMD2<Float>
  public let yaw: Float
  public let animationTime: Float
  public let motionPhase: Float?
  public let distance: Float
  public let visible: Bool
  public let posture: String
  public let gaze: Float
  public let fear: Float
  public let signal: WildlifeSignal
  public let evidence: WildlifeEvidence?
  public let trust: Float
  public let temperament: AnimalTemperament
  public let capabilities: [WildlifeCapability]
  public let companion: WildlifeCompanionFacts
  public let activeActivity: AnimalSharedActivityProgress?
  public let sharedExperiences: [AnimalSharedExperience]
  public let lastReturnEvent: AnimalReturnEvent?
  public let assistanceMemories: [WildlifeAssistanceMemory]
  public let habitatWork: WildlifeHabitatWork
  public let lifeStage: WildlifeLifeStage
  public let morphologyScale: Float
  public let familyID: String?
  public let parentID: String?
}

public struct WildlifeAddressResult: Codable, Equatable, Sendable {
  public let animalID: String
  public let request: AnimalRequest
  public let accepted: Bool
  public let signal: WildlifeSignal
  public let revision: UInt64
}

/// Current, game-owned state for one observed rare individual's habitat story.
/// It deliberately makes no claim about the population of the species.
public enum RareHabitatState: String, Codable, Equatable, Sendable {
  case observed
  case approachingPreferredHabitat
  case settledAtPreferredHabitat
  case returningHome
}

public struct RareHabitatFact: Codable, Equatable, Sendable {
  public let characterization: WildlifeDiscoveryCharacterization
  public let species: WildlifeSpecies
  public let landmarkID: String
  public let preferredPlanting: HabitatGarden.Planting?
  public let state: RareHabitatState
  public let supportingPatchID: HabitatGarden.PatchID?
  public let habitatWork: WildlifeHabitatWork
  /// The exact persistent object owned by this actor, including its actual
  /// building, active, displaced or rebuilding state and stable revision.
  public let structure: EcologyStructure?
}

/// Persisted facts from the already validated mounted-movement path. Rendering
/// may map distance to a species gait, but gameplay owns whether travel occurred.
public struct WildlifeMountedTravelSignal: Codable, Equatable, Sendable {
  public static let cycleLength: Float = 64
  public private(set) var cycleDistance: Float = 0
  public private(set) var latestSegmentDistance: Float = 0
  public private(set) var heading: Float = 0
  public private(set) var populationTick: Int = 0
  public private(set) var active = false

  public var isMoving: Bool { active && latestSegmentDistance > 0.001 }

  fileprivate mutating func accept(
    from start: SIMD2<Float>, to end: SIMD2<Float>, tick: Int, contributesTravel: Bool
  ) {
    let delta = end - start
    let segment = distance(start, end)
    active = true
    populationTick = tick
    if segment > 0.001 { heading = atan2(delta.x, -delta.y) }
    guard contributesTravel, segment > 0.001 else {
      latestSegmentDistance = 0
      return
    }
    latestSegmentDistance = segment
    cycleDistance = (cycleDistance + segment).truncatingRemainder(
      dividingBy: Self.cycleLength)
  }

  fileprivate mutating func expireMovement(at tick: Int) {
    if tick - populationTick > 1 { latestSegmentDistance = 0 }
  }

  fileprivate mutating func end(at tick: Int) {
    active = false
    latestSegmentDistance = 0
    populationTick = tick
  }

  fileprivate func validate() throws {
    guard cycleDistance.isFinite, (0..<Self.cycleLength).contains(cycleDistance),
      latestSegmentDistance.isFinite, (0...12).contains(latestSegmentDistance),
      heading.isFinite, abs(heading) <= Float.pi,
      (0...1_000_000_000).contains(populationTick), active || latestSegmentDistance == 0
    else { throw WildlifePopulationError.invalidState }
  }
}

public struct WildlifeActor: Codable, Equatable, Sendable {
  public let id: String
  public let species: WildlifeSpecies
  public let biome: SanctuaryBiome
  public let landmarkID: String
  public private(set) var nativeHome: SIMD2<Float>
  public let familyID: String?
  public let parentID: String?
  public let maturityTicks: Int
  public private(set) var growthTicks: Int
  public private(set) var relationship: AnimalRelationship
  public private(set) var simulation: CreatureSimulation
  public private(set) var relocationTarget: SIMD2<Float>?
  public private(set) var attractedPatchID: HabitatGarden.PatchID?
  public private(set) var habitatOrigin: SIMD2<Float>?
  public private(set) var habitatDestination: SIMD2<Float>?
  public private(set) var habitatWork: WildlifeHabitatWork
  public private(set) var mountedTravel: WildlifeMountedTravelSignal

  public var position: SIMD2<Float> { simulation.position }
  public var yaw: Float { simulation.yaw }
  public var time: Float { simulation.time }
  public var motionPhase: Float? { simulation.phase }
  public var posture: String { simulation.state }
  public var gaze: Float { simulation.gaze }
  public var refusal: Bool { simulation.state == "declining" }
  public var playing: Bool { simulation.state == "playing" }
  public var isHabitatMigrating: Bool { habitatDestination != nil }
  public var lifeStage: WildlifeLifeStage {
    maturityTicks > 0 && growthTicks < maturityTicks ? .young : .adult
  }
  /// Uniform root scale preserves the authored face proportions and uses the
  /// same motion evaluator throughout gentle growth.
  public var morphologyScale: Float {
    guard maturityTicks > 0 else { return 1 }
    let t = min(1, max(0, Float(growthTicks) / Float(maturityTicks)))
    return 0.62 + 0.38 * CreatureMotion.smooth(t)
  }
  public var capabilities: [WildlifeCapability] { species.profile.capabilities }
  public var companion: WildlifeCompanionFacts {
    WildlifeCompanionFacts(relationship: relationship, capabilities: capabilities)
  }

  fileprivate init(
    id: String, species: WildlifeSpecies, biome: SanctuaryBiome, landmarkID: String,
    home: SIMD2<Float>, seed: UInt32, companionWilling: Bool,
    familyID: String? = nil, parentID: String? = nil, young: Bool = false
  ) {
    self.id = id
    self.species = species
    self.biome = biome
    self.landmarkID = landmarkID
    nativeHome = home
    self.familyID = familyID
    self.parentID = parentID
    maturityTicks = young ? 21_600 : 0
    growthTicks = 0
    relationship = .baked(id: id, seed: seed, companionWilling: companionWilling)
    simulation = CreatureSimulation(home: home, seed: seed)
    relocationTarget = nil
    attractedPatchID = nil
    habitatOrigin = nil
    habitatDestination = nil
    habitatWork = WildlifeHabitatWork(activity: species.profile.habitatActivity)
    mountedTravel = WildlifeMountedTravelSignal()
  }

  fileprivate mutating func restoreLegacy(
    relationship: AnimalRelationship, simulation: CreatureSimulation
  ) {
    self.relationship = relationship
    self.simulation = simulation
  }

  fileprivate mutating func observePresence(seconds: Float) {
    relationship.observeCalmPresence(seconds: seconds)
  }

  fileprivate mutating func advanceGrowth() {
    guard maturityTicks > 0, growthTicks < maturityTicks else { return }
    growthTicks += 1
  }

  fileprivate mutating func restartHabitatWork() {
    habitatWork.restart()
  }

  /// Moves one superseded authored home through the ordinary habitat-waypoint
  /// path. Identity, relationship, growth and any player-selected habitat stay
  /// intact. When an animal is already settled at an attracted patch, only its
  /// eventual return point changes until that patch is removed.
  fileprivate mutating func correctAuthoredHome(
    from legacyHome: SIMD2<Float>, to correctedHome: SIMD2<Float>
  ) -> Bool {
    guard distance(nativeHome, legacyHome) <= 0.01 else { return false }
    nativeHome = correctedHome
    if habitatOrigin.map({ distance($0, legacyHome) <= 0.01 }) == true {
      habitatOrigin = correctedHome
    }
    if habitatDestination.map({ distance($0, legacyHome) <= 0.01 }) == true {
      habitatDestination = correctedHome
      updateHabitatWaypoint()
    } else if habitatOrigin == nil {
      habitatOrigin = simulation.home
      habitatDestination = correctedHome
      attractedPatchID = nil
      updateHabitatWaypoint()
    }
    habitatWork.restart()
    return true
  }

  fileprivate mutating func beginHabitatAttraction(
    patchID: HabitatGarden.PatchID, destination: SIMD2<Float>
  ) {
    guard attractedPatchID == nil, habitatOrigin == nil,
      SanctuaryGeography.bounds.contains(destination)
    else { return }
    habitatOrigin = simulation.home
    habitatDestination = destination
    attractedPatchID = patchID
    updateHabitatWaypoint()
  }

  fileprivate mutating func beginHabitatRecovery() {
    guard let origin = habitatOrigin else {
      attractedPatchID = nil
      habitatDestination = nil
      return
    }
    attractedPatchID = nil
    habitatDestination = origin
    updateHabitatWaypoint()
  }

  private mutating func updateHabitatWaypoint() {
    guard let destination = habitatDestination else { return }
    let from = simulation.home
    let delta = destination - from
    if length(delta) <= 8 {
      relocationTarget = destination
    } else {
      relocationTarget = boundedWorldPoint(from + normalize(delta) * 8)
    }
  }

  private mutating func finishHabitatWaypointIfReached() {
    guard let waypoint = relocationTarget, let destination = habitatDestination,
      distance(simulation.position, waypoint) <= 0.35
    else { return }
    simulation.settleHome(waypoint)
    if distance(waypoint, destination) <= 0.35 {
      relocationTarget = nil
      habitatDestination = nil
      if attractedPatchID == nil { habitatOrigin = nil }
    } else {
      updateHabitatWaypoint()
    }
  }

  fileprivate mutating func address(_ request: AnimalRequest, player: SIMD2<Float>, tick: Int)
    -> Bool
  {
    let accepted = relationship.isWilling(to: request)
    simulation.address(request, accepted: accepted, player: player)
    relationship.recordInvitation(
      request, accepted: accepted, tick: tick, position: simulation.position)
    return accepted
  }

  fileprivate mutating func observePlayerReturn(
    tick: Int, player: SIMD2<Float>, visible: Bool, canRespond: Bool
  ) {
    let response = relationship.observePlayerReturn(
      tick: tick, distance: distance(simulation.position, player), visible: visible,
      canRespond: canRespond && simulation.fear < 0.35)
    guard let response else { return }
    simulation.address(
      response == .approached ? .come : .greeting, accepted: true, player: player)
  }

  fileprivate mutating func planHabitat(in garden: HabitatGarden) {
    guard habitatOrigin == nil else { return }
    let profile = species.profile
    let home = simulation.home
    let terrainDisturbance = garden.terrainPatches.compactMap {
      patch -> (HabitatGarden.TerrainPatch, Float)? in
      let point = SIMD2<Float>(patch.center.x, patch.center.z)
      let d = distance(home, point)
      return d <= patch.radius + 1 ? (patch, d) : nil
    }.min { left, right in
      left.1 == right.1 ? left.0.id < right.0.id : left.1 < right.1
    }
    if let disturbance = terrainDisturbance {
      let point = SIMD2<Float>(disturbance.0.center.x, disturbance.0.center.z)
      let delta = home - point
      let direction = length(delta) > 0.01 ? normalize(delta) : stableDirection(for: id)
      relocationTarget = boundedWorldPoint(home + direction * min(6, disturbance.0.radius + 1))
      return
    }
    let nearby = garden.patches.compactMap { patch -> (HabitatGarden.Patch, Float)? in
      let point = SIMD2<Float>(patch.center.x, patch.center.z)
      let d = distance(home, point)
      return d <= 9.5 ? (patch, d) : nil
    }
    if let avoided = profile.avoidedPlanting,
      let threat = nearby.filter({ $0.0.planting == avoided }).min(by: habitatPatchOrder)
    {
      let point = SIMD2<Float>(threat.0.center.x, threat.0.center.z)
      let delta = home - point
      let direction = length(delta) > 0.01 ? normalize(delta) : stableDirection(for: id)
      relocationTarget = boundedWorldPoint(home + direction * 6)
      return
    }
    if let preferred = profile.preferredPlanting,
      let refuge = nearby.filter({ $0.0.planting == preferred }).min(by: habitatPatchOrder)
    {
      relocationTarget = SIMD2<Float>(refuge.0.center.x, refuge.0.center.z)
      return
    }
    relocationTarget = distance(home, nativeHome) <= 9.5 && distance(home, nativeHome) > 0.35
      ? nativeHome : nil
  }

  fileprivate mutating func step(
    stimulus: CreatureStimulus, seconds: Float, populationTick: Int, garden: HabitatGarden,
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)?
  ) {
    let requestBeforeStep = simulation.activeRequest
    let requestWasAccepted = simulation.requestAccepted
    simulation.step(stimulus, canTraverse: canTraverse)
    if let activity = relationship.activeActivity?.kind {
      let playerDistance = distance(simulation.position, stimulus.player)
      let calm = simulation.fear < 0.35
      let qualifying: Bool
      switch activity {
      case .company:
        qualifying = calm && !stimulus.running && playerDistance <= WildlifePopulation.requestRange
      case .play:
        qualifying = calm && requestWasAccepted && requestBeforeStep == .play
          && playerDistance <= WildlifePopulation.requestRange
      case .exploration:
        qualifying = calm && requestWasAccepted && requestBeforeStep == .follow
          && playerDistance <= 10
      }
      relationship.advanceSharedActivity(
        tick: populationTick, position: simulation.position, qualifying: qualifying)
    }
    if habitatDestination != nil {
      finishHabitatWaypointIfReached()
    } else if let target = relocationTarget, distance(simulation.position, target) <= 0.35 {
      simulation.settleHome(target)
      relocationTarget = nil
    }
    // Local habitat repair includes gathering and preparation while the animal
    // crosses the few metres to its new work site. Regional attraction remains
    // paused until arrival so a distant animal cannot build along its route.
    guard lifeStage == .adult, habitatDestination == nil,
      simulation.activeRequest == nil, simulation.fear < 0.35
    else { return }
    let conditions = garden.conditions(
      at: .init(x: simulation.position.x, z: simulation.position.y))
    let support: Float
    switch species.profile.preferredPlanting {
    case .some(.flowers): support = conditions.flowering
    case .some(.grove): support = conditions.shelter
    case .some(.reeds): support = conditions.groundcover
    case .some(.shallowWater): support = conditions.shallowWaterDepth * 5
    case .none: support = 0
    }
    habitatWork.advance(seconds: seconds, habitatSupport: support)
  }

  fileprivate mutating func moveWithCompanion(to point: SIMD2<Float>, tick: Int) {
    let contributesTravel = relationship.mountedExplorationActive
      && (relationship.activeActivity == nil || relationship.activeActivity!.activeTicks > 0)
    mountedTravel.accept(
      from: simulation.position, to: point, tick: tick, contributesTravel: contributesTravel)
    relationship.recordMountedExplorationMovement(
      tick: tick, to: point)
    simulation.relocateHome(point)
    relocationTarget = nil
  }

  fileprivate mutating func advanceMountedExploration(tick: Int) {
    mountedTravel.expireMovement(at: tick)
    relationship.advanceMountedExploration(tick: tick, position: position)
  }

  fileprivate mutating func endMountedExploration(tick: Int) {
    if mountedTravel.active { mountedTravel.end(at: tick) }
    relationship.endMountedExploration()
  }

  fileprivate mutating func recordAssistance(
    outcomeID: String, kind: WildlifeAssistanceKind, tick: Int
  ) {
    relationship.recordAssistance(outcomeID: outcomeID, kind: kind, tick: tick)
  }

  fileprivate func validate() throws {
    guard !id.isEmpty, id.count <= 64, relationship.id == id,
      species.profile.biomes.contains(biome),
      let landmark = SanctuaryGeography.landmarks.first(where: { $0.id == landmarkID }),
      landmark.biome == biome,
      nativeHome.x.isFinite, nativeHome.y.isFinite,
      SanctuaryGeography.bounds.contains(nativeHome),
      [simulation.position, simulation.home, simulation.start, simulation.target]
        .allSatisfy({ SanctuaryGeography.bounds.contains($0) }),
      // Common groups may live along a route whose stable endpoint landmark
      // owns their region. The farthest authored midpoint is under 5 km.
      distance(nativeHome, landmark.coordinate) <= 5_000,
      relocationTarget.map({
        $0.x.isFinite && $0.y.isFinite && distance($0, simulation.home) <= 10.1
      }) ?? true,
      habitatOrigin.map({ $0.x.isFinite && $0.y.isFinite
        && SanctuaryGeography.bounds.contains($0) }) ?? true,
      habitatDestination.map({ $0.x.isFinite && $0.y.isFinite
        && SanctuaryGeography.bounds.contains($0) }) ?? true,
      attractedPatchID.map({ $0 > 0 }) ?? true,
      habitatOrigin == nil
        ? (attractedPatchID == nil && habitatDestination == nil)
        : (attractedPatchID != nil || habitatDestination != nil),
      maturityTicks >= 0, maturityTicks <= 1_000_000_000,
      growthTicks >= 0, growthTicks <= maturityTicks,
      familyID.map({ !$0.isEmpty && $0.count <= 64 }) ?? true,
      parentID.map({ !$0.isEmpty && $0.count <= 64 && $0 != id }) ?? true,
      morphologyScale.isFinite, (0.62...1).contains(morphologyScale)
    else { throw WildlifePopulationError.invalidState }
    try relationship.validate()
    try simulation.validate()
    try habitatWork.validate()
    try mountedTravel.validate()
  }


  private enum CodingKeys: String, CodingKey {
    case id, species, biome, landmarkID, nativeHome, familyID, parentID, maturityTicks,
      growthTicks, relationship, simulation, relocationTarget, attractedPatchID, habitatOrigin,
      habitatDestination, habitatWork, mountedTravel
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    id = try container.decode(String.self, forKey: .id)
    species = try container.decode(WildlifeSpecies.self, forKey: .species)
    biome = try container.decode(SanctuaryBiome.self, forKey: .biome)
    landmarkID = try container.decode(String.self, forKey: .landmarkID)
    nativeHome = try container.decode(SIMD2<Float>.self, forKey: .nativeHome)
    familyID = try container.decodeIfPresent(String.self, forKey: .familyID)
    parentID = try container.decodeIfPresent(String.self, forKey: .parentID)
    maturityTicks = try container.decodeIfPresent(Int.self, forKey: .maturityTicks) ?? 0
    growthTicks = try container.decodeIfPresent(Int.self, forKey: .growthTicks) ?? maturityTicks
    relationship = try container.decode(AnimalRelationship.self, forKey: .relationship)
    simulation = try container.decode(CreatureSimulation.self, forKey: .simulation)
    relocationTarget = try container.decodeIfPresent(SIMD2<Float>.self, forKey: .relocationTarget)
    attractedPatchID = try container.decodeIfPresent(
      HabitatGarden.PatchID.self, forKey: .attractedPatchID)
    habitatOrigin = try container.decodeIfPresent(SIMD2<Float>.self, forKey: .habitatOrigin)
    habitatDestination = try container.decodeIfPresent(
      SIMD2<Float>.self, forKey: .habitatDestination)
    habitatWork = try container.decode(WildlifeHabitatWork.self, forKey: .habitatWork)
    mountedTravel = try container.decodeIfPresent(
      WildlifeMountedTravelSignal.self, forKey: .mountedTravel) ?? WildlifeMountedTravelSignal()
    try validate()
  }
}

public enum WildlifePopulationError: LocalizedError, Equatable, Sendable {
  case staleRevision
  case unknownAnimal
  case unrecognizedRequest
  case tooFar
  case notVisible
  case unavailableCapability
  case invalidMovement
  case invalidAssistanceOutcome
  case duplicateAssistanceOutcome
  case invalidState

  public var errorDescription: String? {
    switch self {
    case .staleRevision: return "The wildlife population changed. Observe it again before acting."
    case .unknownAnimal: return "That animal is not present in this population."
    case .unrecognizedRequest:
      return "Try hello, play with me, come here, follow me, or wait here."
    case .tooFar: return "The animal is too far away to notice the request."
    case .notVisible: return "The animal needs a clear view of the player."
    case .unavailableCapability: return "This animal is not ready or able to provide that help."
    case .invalidMovement: return "That companion movement is outside the reachable world."
    case .invalidAssistanceOutcome:
      return "That completed assistance outcome is not valid for this animal."
    case .duplicateAssistanceOutcome:
      return "That assistance outcome is already part of the relationship history."
    case .invalidState: return "The wildlife population contains invalid state."
    }
  }
}

/// A finite, saveable population. It owns animal facts and decisions but no
/// renderer, wall clock, terrain implementation, or language model.
public struct WildlifePopulation: Codable, Equatable, Sendable {
  public static let maximumActors = 128
  /// Maximum player-to-animal distance for every authored request and for the
  /// native interaction prompt that offers those requests.
  public static let requestRange: Float = 8
  public static let fullSimulationRadius: Float = 80
  public static let coarseInterval = 30
  public static let habitatAttractionRadius: Float = 480
  public static let maximumAttractedPerPatch = 3
  /// Sorted, explicit actor identities keep rarity authored per individual.
  /// Other members of the same species do not inherit this claim.
  public static let discoveryCharacterizations: [WildlifeDiscoveryCharacterization] = [
    .init(animalID: "brookweaver-001", rarity: .rare, evidence: .call)
  ]

  /// Exact previous authored homes and nearby dry-bank replacements. This is
  /// derived migration metadata rather than another required save field.
  private static let authoredHomeCorrections: [
    String: (legacy: SIMD2<Float>, candidates: [SIMD2<Float>])
  ] = {
    let meadow = SanctuaryGeography.landmarks.first { $0.id == "golden-meadow" }?.coordinate
      ?? SIMD2<Float>(2_450, -630)
    return [
      "sunhare-002": (
        meadow + SIMD2<Float>(4, -3),
        [SIMD2<Float>(26, 8), SIMD2<Float>(32, 12), SIMD2<Float>(26, -10), SIMD2<Float>(38, 8)]
          .map { meadow + $0 }),
      "sunhare-young-001": (
        meadow + SIMD2<Float>(5.5, -3.5),
        [
          SIMD2<Float>(27.5, 7.5), SIMD2<Float>(33.5, 11.5), SIMD2<Float>(27.5, -10.5),
          SIMD2<Float>(39.5, 7.5),
        ].map { meadow + $0 }),
    ]
  }()

  public private(set) var revision: UInt64
  public private(set) var tick: Int
  public private(set) var actors: [WildlifeActor]
  public private(set) var ecology: EcologyStructures
  private var fractionalTicks: Double
  private var observedGardenRevision: UInt64

  private init(
    revision: UInt64, tick: Int, actors: [WildlifeActor], fractionalTicks: Double,
    observedGardenRevision: UInt64, ecology: EcologyStructures
  ) {
    self.revision = revision
    self.tick = tick
    self.actors = actors
    self.fractionalTicks = fractionalTicks
    self.observedGardenRevision = observedGardenRevision
    self.ecology = ecology
  }

  public static func initial(seed: UInt32 = 17) -> WildlifePopulation {
    func landmark(_ id: String) -> SIMD2<Float> {
      SanctuaryGeography.landmarks.first(where: { $0.id == id })?.coordinate ?? .zero
    }
    func actor(
      _ id: String, _ species: WildlifeSpecies, _ biome: SanctuaryBiome, _ landmarkID: String,
      offset: SIMD2<Float> = .zero, companion: Bool = false, family: String? = nil,
      parent: String? = nil, young: Bool = false
    ) -> WildlifeActor {
      WildlifeActor(
        id: id, species: species, biome: biome, landmarkID: landmarkID,
        home: landmark(landmarkID) + offset, seed: stableSeed(for: id, base: seed),
        companionWilling: companion, familyID: family, parentID: parent, young: young)
    }
    let roster = [
      actor(
        "frostling-001", .frostling, .woodland, "cabin-glade", offset: SIMD2(-3, -83),
        companion: true),
      actor(
        "sunhare-001", .sunhare, .woodland, "cabin-glade", offset: SIMD2(2, 18),
        companion: true),
      actor(
        "sunhare-002", .sunhare, .meadow, "golden-meadow", offset: SIMD2(26, 8),
        family: "golden-sunhare-family"),
      actor(
        "sunhare-young-001", .sunhare, .meadow, "golden-meadow", offset: SIMD2(27.5, 7.5),
        family: "golden-sunhare-family", parent: "sunhare-002", young: true),
      actor(
        "sunhare-003", .sunhare, .meadow, "golden-meadow", offset: SIMD2(-1_590, 410)),
      actor(
        "brookweaver-001", .brookweaver, .creek, "willow-creek", offset: SIMD2(-2, 3),
        companion: true),
      actor(
        "brookweaver-002", .brookweaver, .creek, "willow-creek",
        offset: SIMD2(385, -1_400)),
      actor(
        "reedwalker-001", .reedwalker, .wetland, "reed-mirror", offset: SIMD2(5, 1),
        companion: true),
      actor(
        "reedwalker-002", .reedwalker, .wetland, "reed-mirror", offset: SIMD2(-4, -2),
        family: "reed-mirror-family"),
      actor(
        "reedwalker-young-001", .reedwalker, .wetland, "reed-mirror",
        offset: SIMD2(-2.5, -1.5), family: "reed-mirror-family",
        parent: "reedwalker-002", young: true),
      actor(
        "reedwalker-003", .reedwalker, .wetland, "reed-mirror",
        offset: SIMD2(-1_400, -875)),
      actor(
        "reedwalker-004", .reedwalker, .wetland, "reed-mirror",
        offset: SIMD2(770, 1_190)),
      actor(
        "moonhart-001", .moonhart, .woodland, "cabin-glade", offset: SIMD2(120, -20),
        companion: true),
      actor(
        "moonhart-002", .moonhart, .lake, "moonlake", offset: SIMD2(1_065, 0),
        companion: true),
      actor(
        "cloudstepper-001", .cloudstepper, .alpine, "cloudstep", offset: SIMD2(-3, 2),
        companion: true),
      actor(
        "cloudstepper-002", .cloudstepper, .alpine, "cloudstep",
        offset: SIMD2(3_220, -2_188)),
      actor("dunefox-001", .dunefox, .desert, "sunglass-dunes", offset: SIMD2(3, 3)),
      actor(
        "dunefox-002", .dunefox, .desert, "sunglass-dunes", offset: SIMD2(4_078, 2_450)),
      actor(
        "canopy-glider-001", .canopyGlider, .rainforest, "verdant-canopy",
        offset: SIMD2(-2, 4), companion: true),
      actor(
        "canopy-glider-002", .canopyGlider, .rainforest, "verdant-canopy",
        offset: SIMD2(-3_063, 1_173)),
      actor(
        "saltback-001", .saltback, .coast, "saltwind-bluffs", offset: SIMD2(4, 2),
        companion: true),
      actor(
        "saltback-002", .saltback, .coast, "saltwind-bluffs",
        offset: SIMD2(-1_575, 2_100)),
      actor(
        "tidepooler-001", .tidepooler, .tidepool, "lantern-tidepools",
        offset: SIMD2(-3, -1), companion: true),
      actor(
        "tidepooler-002", .tidepooler, .tidepool, "lantern-tidepools",
        offset: SIMD2(-613, 1_173)),
      actor(
        "cloud-ray-001", .cloudRay, .ocean, "open-blue", offset: SIMD2(2, -4),
        companion: true),
    ].sorted { $0.id < $1.id }
    return WildlifePopulation(
      revision: 0, tick: 0, actors: roster, fractionalTicks: 0, observedGardenRevision: 0,
      ecology: EcologyStructures())
  }

  /// Places legacy Frostling state into its stable roster slot. The population
  /// still contains exactly one `frostling-001`.
  public static func migrating(
    seed: UInt32 = 17, relationship: AnimalRelationship, creature: CreatureSimulation
  ) throws -> WildlifePopulation {
    var population = initial(seed: seed)
    guard relationship.id == "frostling-001",
      let index = population.actors.firstIndex(where: { $0.id == relationship.id })
    else { throw WildlifePopulationError.invalidState }
    population.actors[index].restoreLegacy(relationship: relationship, simulation: creature)
    try population.validate()
    return population
  }

  public func actor(id: String) -> WildlifeActor? {
    actors.first { $0.id == id }
  }

  public func discoveryCharacterization(
    for animalID: String
  ) -> WildlifeDiscoveryCharacterization? {
    Self.discoveryCharacterizations.first { $0.animalID == animalID }
  }

  /// Projects journal-ready facts from the current save without recording a
  /// milestone. Only actually observed, explicitly rare individuals appear.
  public func rareHabitatFacts(
    observedAnimalIDs: [String], garden: HabitatGarden
  ) -> [RareHabitatFact] {
    let observed = Set(observedAnimalIDs)
    return Self.discoveryCharacterizations.compactMap { characterization in
      guard characterization.rarity == .rare,
        observed.contains(characterization.animalID),
        let actor = actor(id: characterization.animalID)
      else { return nil }
      let supportingPatch = actor.attractedPatchID.flatMap { patchID in
        garden.patches.first {
          $0.id == patchID && $0.planting == actor.species.profile.preferredPlanting
        }
      }
      let state: RareHabitatState
      if actor.attractedPatchID == nil, actor.habitatOrigin != nil,
        actor.habitatDestination != nil
      {
        state = .returningHome
      } else if supportingPatch != nil, actor.habitatDestination != nil {
        state = .approachingPreferredHabitat
      } else if let patch = supportingPatch {
        let center = SIMD2<Float>(patch.center.x, patch.center.z)
        state = distance(actor.simulation.home, center) <= patch.radius + 0.5
          ? .settledAtPreferredHabitat : .observed
      } else {
        state = .observed
      }
      return RareHabitatFact(
        characterization: characterization, species: actor.species,
        landmarkID: actor.landmarkID, preferredPlanting: actor.species.profile.preferredPlanting,
        state: state, supportingPatchID: supportingPatch?.id,
        habitatWork: actor.habitatWork, structure: ecology.structure(ownerID: actor.id))
    }
  }

  public func nearest(
    to point: SIMD2<Float>, within range: Float = .greatestFiniteMagnitude,
    matching predicate: (WildlifeActor) -> Bool = { _ in true }
  ) -> WildlifeActor? {
    guard point.x.isFinite, point.y.isFinite, range.isFinite, range >= 0 else { return nil }
    return actors.filter { predicate($0) && distance($0.position, point) <= range }
      .min {
        let left = distance_squared($0.position, point)
        let right = distance_squared($1.position, point)
        return left == right ? $0.id < $1.id : left < right
      }
  }

  public func observation(for id: String, player: SIMD2<Float>, visible: Bool)
    -> WildlifeObservation?
  {
    guard let actor = actor(id: id), player.x.isFinite, player.y.isFinite else { return nil }
    let d = distance(actor.position, player)
    let signal = signal(for: actor)
    let evidence: WildlifeEvidence?
    if visible {
      evidence = .sighting
    } else if d <= 12 {
      evidence = actor.species.profile.hiddenEvidence
    } else if d <= 35 && actor.species.profile.hiddenEvidence == .call {
      evidence = .call
    } else {
      evidence = nil
    }
    return WildlifeObservation(
      id: actor.id, species: actor.species, biome: actor.biome, position: actor.position,
      yaw: actor.yaw, animationTime: actor.time, motionPhase: actor.motionPhase, distance: d,
      visible: visible, posture: actor.posture, gaze: actor.gaze, fear: actor.simulation.fear,
      signal: signal, evidence: evidence, trust: actor.relationship.trust,
      temperament: actor.relationship.preferences.temperament, capabilities: actor.capabilities,
      companion: actor.companion, activeActivity: actor.relationship.activeActivity,
      sharedExperiences: actor.relationship.sharedExperiences,
      lastReturnEvent: actor.relationship.returnEvents.last,
      assistanceMemories: actor.relationship.assistanceMemories, habitatWork: actor.habitatWork,
      lifeStage: actor.lifeStage, morphologyScale: actor.morphologyScale,
      familyID: actor.familyID, parentID: actor.parentID)
  }

  public func observations(
    player: SIMD2<Float>, isVisible: (WildlifeActor) -> Bool
  ) -> [WildlifeObservation] {
    actors.compactMap { observation(for: $0.id, player: player, visible: isVisible($0)) }
  }

  private mutating func refreshHabitatAttractions(
    in garden: HabitatGarden, mountedID: String?
  ) {
    let patchesByID = Dictionary(uniqueKeysWithValues: garden.patches.map { ($0.id, $0) })
    for index in actors.indices {
      if let patchID = actors[index].attractedPatchID {
        let stillAppealing = patchesByID[patchID].map {
          $0.planting == actors[index].species.profile.preferredPlanting
        } ?? false
        if !stillAppealing { actors[index].beginHabitatRecovery() }
      }
      if actors[index].id != mountedID, actors[index].simulation.activeRequest == nil {
        actors[index].planHabitat(in: garden)
      }
    }

    for patch in garden.patches.sorted(by: { $0.id < $1.id }) {
      let destination = SIMD2<Float>(patch.center.x, patch.center.z)
      var assigned = actors.filter { $0.attractedPatchID == patch.id }.count
      guard assigned < Self.maximumAttractedPerPatch else { continue }
      let candidates = actors.indices.filter { index in
        let actor = actors[index]
        return actor.id != mountedID && actor.lifeStage == .adult && actor.habitatOrigin == nil
          && actor.simulation.activeRequest == nil
          && actor.species.profile.preferredPlanting == patch.planting
          && distance(actor.simulation.home, destination) <= Self.habitatAttractionRadius
      }.sorted { left, right in
        let leftDistance = distance_squared(actors[left].simulation.home, destination)
        let rightDistance = distance_squared(actors[right].simulation.home, destination)
        return leftDistance == rightDistance ? actors[left].id < actors[right].id
          : leftDistance < rightDistance
      }
      for index in candidates where assigned < Self.maximumAttractedPerPatch {
        if !actors[index].habitatWork.completed { actors[index].restartHabitatWork() }
        let parentID = actors[index].id
        actors[index].beginHabitatAttraction(patchID: patch.id, destination: destination)
        assigned += 1
        let children = actors.indices.filter {
          actors[$0].parentID == parentID && actors[$0].habitatOrigin == nil
            && actors[$0].id != mountedID && actors[$0].simulation.activeRequest == nil
            && distance(actors[$0].simulation.home, actors[index].simulation.home) <= 40
        }.sorted { actors[$0].id < actors[$1].id }
        for child in children where assigned < Self.maximumAttractedPerPatch {
          actors[child].beginHabitatAttraction(patchID: patch.id, destination: destination)
          assigned += 1
        }
      }
    }
  }

  /// Corrects only exact legacy authored homes. A player-authored shallow-water
  /// patch reserves its footprint, so migration waits for or selects another
  /// authored bank site without rewriting the garden.
  private mutating func correctSupersededAuthoredHomes(
    in garden: HabitatGarden, mountedID: String?
  ) throws {
    for index in actors.indices {
      let actor = actors[index]
      guard actor.id != mountedID, actor.simulation.activeRequest == nil,
        actor.relationship.activeActivity == nil,
        let correction = Self.authoredHomeCorrections[actor.id],
        distance(actor.nativeHome, correction.legacy) <= 0.01,
        let destination = correction.candidates.first(where: { point in
          !garden.patches.contains { patch in
            patch.planting == .shallowWater
              && distance(point, SIMD2<Float>(patch.center.x, patch.center.z))
                <= patch.radius + 1.25
          }
        })
      else { continue }
      if actors[index].correctAuthoredHome(from: correction.legacy, to: destination) {
        try ecology.beginAuthoredHomeCorrection(
          ownerID: actor.id, kind: .restingBed, legacyPosition: correction.legacy)
      }
    }
  }

  public mutating func advance(
    seconds: Float, player: SIMD2<Float>, running: Bool, garden: HabitatGarden,
    mountedID: String? = nil,
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil,
    actorCanTraverse: ((WildlifeActor, SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil,
    isVisible: (WildlifeActor) -> Bool
  ) throws {
    guard seconds.isFinite, seconds > 0, seconds <= 1, player.x.isFinite, player.y.isFinite,
      SanctuaryGeography.bounds.contains(player), revision < .max
    else { throw WildlifePopulationError.invalidState }
    if let mountedID {
      guard let mounted = actor(id: mountedID) else { throw WildlifePopulationError.unknownAnimal }
      guard mounted.companion.rideEligible || mounted.companion.flyEligible else {
        throw WildlifePopulationError.unavailableCapability
      }
    }
    var candidate = self
    if candidate.observedGardenRevision != garden.revision {
      let resetOwners = try candidate.ecology.reconcile(with: garden)
      for index in candidate.actors.indices {
        if resetOwners.contains(candidate.actors[index].id) {
          candidate.actors[index].restartHabitatWork()
        }
      }
      candidate.refreshHabitatAttractions(in: garden, mountedID: mountedID)
      candidate.observedGardenRevision = garden.revision
    }
    // Run after ordinary garden reconciliation so a just-displaced legacy
    // structure cannot be cleared back to rebuilding at its obsolete wet site.
    try candidate.correctSupersededAuthoredHomes(in: garden, mountedID: mountedID)
    candidate.fractionalTicks += Double(seconds) * 60
    let frames = Int(candidate.fractionalTicks.rounded(.down))
    candidate.fractionalTicks -= Double(frames)
    for _ in 0..<frames {
      candidate.tick += 1
      guard candidate.tick <= 1_000_000_000 else { throw WildlifePopulationError.invalidState }
      for index in candidate.actors.indices {
        var individual = candidate.actors[index]
        individual.advanceGrowth()
        candidate.actors[index] = individual
        if individual.id == mountedID {
          individual.advanceMountedExploration(tick: candidate.tick)
          candidate.actors[index] = individual
          continue
        }
        individual.endMountedExploration(tick: candidate.tick)
        // Publish the dismount even when this actor is outside its coarse-update
        // bucket; otherwise stale mounted activity survives until a later tick.
        candidate.actors[index] = individual
        let d = distance(individual.position, player)
        let urgent = d <= Self.fullSimulationRadius || individual.simulation.activeRequest != nil
          || individual.relationship.activeActivity != nil
          || individual.relocationTarget != nil
        let bucket = Int(stableSeed(for: individual.id, base: 0) % UInt32(Self.coarseInterval))
        let sparse = (candidate.tick + bucket) % Self.coarseInterval == 0
        guard urgent || sparse else { continue }
        let visible = isVisible(individual)
        individual.observePlayerReturn(
          tick: candidate.tick, player: player, visible: visible,
          canRespond: !running && individual.id != mountedID
            && individual.simulation.activeRequest == nil
            && individual.relationship.activeActivity == nil
            && individual.relocationTarget == nil && !individual.isHabitatMigrating)
        if visible && !running && d <= 5 {
          individual.observePresence(seconds: 1 / 60)
        }
        var stimulus = CreatureStimulus()
        stimulus.player = player
        stimulus.visible = visible
        stimulus.running = running
        stimulus.food = nil
        let parentIsOccupied = individual.parentID.flatMap { parentID in
          candidate.actors.first(where: { $0.id == parentID })
        }.map { $0.id == mountedID || $0.simulation.activeRequest != nil } ?? false
        stimulus.habitatTarget = parentIsOccupied ? nil : individual.relocationTarget
        // Host-committed collision is a local presentation fact: distant chunks
        // are deliberately absent from the native 3x3 residency window. A
        // sparse coarse update stays inside its bounded authored home and must
        // not change persistent state merely because that chunk is not drawn.
        // Nearby and otherwise urgent actors still use the production
        // terrain/solid traversal check on every full-simulation tick.
        let detailedTraversal: ((SIMD2<Float>, SIMD2<Float>) -> Bool)?
        if urgent, let actorCanTraverse {
          // The production world can qualify support by species while the shared
          // creature brain keeps its small two-point route predicate.
          let traversalActor = individual
          detailedTraversal = { start, end in
            actorCanTraverse(traversalActor, start, end)
          }
        } else {
          detailedTraversal = urgent ? canTraverse : nil
        }
        let wasCorrectingAuthoredHome = individual.habitatDestination.map {
          distance($0, individual.nativeHome) <= 0.01 && individual.attractedPatchID == nil
        } ?? false
        individual.step(
          stimulus: stimulus,
          seconds: urgent ? 1 / 60 : Float(Self.coarseInterval) / 60,
          populationTick: candidate.tick, garden: garden,
          canTraverse: detailedTraversal)
        candidate.actors[index] = individual
        if wasCorrectingAuthoredHome, !individual.isHabitatMigrating {
          try candidate.ecology.finishAuthoredHomeCorrection(
            ownerID: individual.id, kind: .restingBed, position: individual.simulation.home,
            yaw: individual.yaw, garden: garden)
        }
        if individual.lifeStage == .adult, !individual.isHabitatMigrating,
          individual.relocationTarget == nil
        {
          try candidate.ecology.synchronize(
            ownerID: individual.id, kind: individual.species.ecologyStructureKind,
            position: individual.simulation.home, yaw: individual.yaw,
            work: individual.habitatWork, garden: garden)
        }
      }
    }
    candidate.revision += 1
    try candidate.validate()
    self = candidate
  }

  @discardableResult public mutating func address(
    _ text: String, targetID: String? = nil, player: SIMD2<Float>, expectedRevision: UInt64,
    isVisible: (WildlifeActor) -> Bool
  ) throws -> WildlifeAddressResult {
    guard let request = AnimalRequestParser.parse(text) else {
      throw WildlifePopulationError.unrecognizedRequest
    }
    return try address(
      request, targetID: targetID, player: player, expectedRevision: expectedRevision,
      isVisible: isVisible)
  }

  @discardableResult public mutating func address(
    _ request: AnimalRequest, targetID: String? = nil, player: SIMD2<Float>,
    expectedRevision: UInt64, isVisible: (WildlifeActor) -> Bool
  ) throws -> WildlifeAddressResult {
    guard expectedRevision == revision else { throw WildlifePopulationError.staleRevision }
    guard player.x.isFinite, player.y.isFinite, SanctuaryGeography.bounds.contains(player),
      revision < .max
    else {
      throw WildlifePopulationError.invalidState
    }
    let index: Int
    if let targetID {
      guard let found = actors.firstIndex(where: { $0.id == targetID }) else {
        throw WildlifePopulationError.unknownAnimal
      }
      index = found
    } else if let found = actors.indices.filter({ isVisible(actors[$0]) }).min(by: {
      let left = distance_squared(actors[$0].position, player)
      let right = distance_squared(actors[$1].position, player)
      return left == right ? actors[$0].id < actors[$1].id : left < right
    }) {
      index = found
    } else {
      throw WildlifePopulationError.notVisible
    }
    let selected = actors[index]
    guard distance(selected.position, player) <= Self.requestRange else {
      throw WildlifePopulationError.tooFar
    }
    guard isVisible(selected) else { throw WildlifePopulationError.notVisible }
    var candidate = self
    let accepted = candidate.actors[index].address(request, player: player, tick: tick)
    candidate.revision += 1
    try candidate.validate()
    self = candidate
    return WildlifeAddressResult(
      animalID: selected.id, request: request, accepted: accepted,
      signal: accepted ? signal(for: candidate.actors[index]) : .refusing,
      revision: candidate.revision)
  }

  /// Keeps an already eligible ridden or flying companion with the player. Each
  /// update is identity-, capability-, revision-, range-, world- and route-checked.
  public mutating func updateCompanionPosition(
    id: String, position: SIMD2<Float>, using capability: WildlifeCapability,
    expectedRevision: UInt64,
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil
  ) throws {
    guard expectedRevision == revision else { throw WildlifePopulationError.staleRevision }
    guard let index = actors.firstIndex(where: { $0.id == id }) else {
      throw WildlifePopulationError.unknownAnimal
    }
    let individual = actors[index]
    let eligible = capability == .ride ? individual.companion.rideEligible
      : capability == .fly ? individual.companion.flyEligible : false
    guard eligible else { throw WildlifePopulationError.unavailableCapability }
    guard position.x.isFinite, position.y.isFinite, SanctuaryGeography.bounds.contains(position),
      distance(individual.position, position) <= 12,
      canTraverse?(individual.position, position) ?? true, revision < .max
    else { throw WildlifePopulationError.invalidMovement }
    var candidate = self
    candidate.actors[index].moveWithCompanion(to: position, tick: candidate.tick)
    candidate.revision += 1
    try candidate.validate()
    self = candidate
  }

  /// Records an outcome only after its owning game action has validated and
  /// applied the concrete mutation in the same transaction.
  public mutating func recordAssistance(
    animalID: String, outcomeID: String, kind: WildlifeAssistanceKind,
    expectedRevision: UInt64
  ) throws {
    guard expectedRevision == revision else { throw WildlifePopulationError.staleRevision }
    guard revision < .max else { throw WildlifePopulationError.invalidState }
    guard let index = actors.firstIndex(where: { $0.id == animalID }) else {
      throw WildlifePopulationError.unknownAnimal
    }
    let trimmed = outcomeID.trimmingCharacters(in: .whitespacesAndNewlines)
    let actor = actors[index]
    guard !trimmed.isEmpty, trimmed.count <= 128, trimmed == outcomeID,
      actor.companion.helpEligible, actor.capabilities.contains(.habitatHelp),
      kind != .boulderMovement || actor.capabilities.contains(.moveBoulders)
    else { throw WildlifePopulationError.invalidAssistanceOutcome }
    guard !actors.contains(where: { individual in
      individual.relationship.assistanceMemories.contains {
        $0.kind == kind && $0.outcomeID == outcomeID
      }
    }) else { throw WildlifePopulationError.duplicateAssistanceOutcome }
    var candidate = self
    candidate.actors[index].recordAssistance(
      outcomeID: outcomeID, kind: kind, tick: tick)
    candidate.revision += 1
    try candidate.validate()
    self = candidate
  }

  public func validate() throws {
    guard actors.count <= Self.maximumActors, revision < .max,
      (0...1_000_000_000).contains(tick), fractionalTicks.isFinite,
      (0..<1).contains(fractionalTicks),
      actors.map(\.id) == actors.map(\.id).sorted(), Set(actors.map(\.id)).count == actors.count,
      Set(actors.map(\.biome)) == Set(SanctuaryBiome.allCases)
    else { throw WildlifePopulationError.invalidState }
    for actor in actors {
      try actor.validate()
      guard actor.relationship.activeActivity.map({
        $0.startedTick <= tick && $0.activeTicks <= tick - $0.startedTick
      }) ?? true,
        actor.relationship.sharedExperiences.allSatisfy({ $0.completedTick <= tick }),
        actor.relationship.awaySinceTick.map({ $0 <= tick }) ?? true,
        actor.relationship.returnEvents.allSatisfy({ $0.returnedTick <= tick }),
        actor.relationship.assistanceMemories.allSatisfy({ $0.completedTick <= tick }),
        actor.mountedTravel.populationTick <= tick
      else { throw WildlifePopulationError.invalidState }
    }
    let byID = Dictionary(uniqueKeysWithValues: actors.map { ($0.id, $0) })
    let assistanceKeys = actors.flatMap { actor in
      actor.relationship.assistanceMemories.map { "\($0.kind.rawValue)|\($0.outcomeID)" }
    }
    guard Set(assistanceKeys).count == assistanceKeys.count else {
      throw WildlifePopulationError.invalidState
    }
    for actor in actors where actor.parentID != nil {
      guard let parent = byID[actor.parentID!], parent.species == actor.species,
        parent.familyID == actor.familyID, actor.familyID != nil
      else { throw WildlifePopulationError.invalidState }
    }
    try ecology.validate(ownerIDs: Set(actors.map(\.id)))
  }

  private enum CodingKeys: String, CodingKey {
    case revision, tick, actors, fractionalTicks, observedGardenRevision, ecology
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    revision = try container.decode(UInt64.self, forKey: .revision)
    tick = try container.decode(Int.self, forKey: .tick)
    actors = try container.decode([WildlifeActor].self, forKey: .actors)
    fractionalTicks = try container.decode(Double.self, forKey: .fractionalTicks)
    observedGardenRevision =
      try container.decodeIfPresent(UInt64.self, forKey: .observedGardenRevision) ?? 0
    ecology = try container.decodeIfPresent(EcologyStructures.self, forKey: .ecology)
      ?? EcologyStructures()
    try validate()
  }
}

private func habitatPatchOrder(
  _ left: (HabitatGarden.Patch, Float), _ right: (HabitatGarden.Patch, Float)
) -> Bool {
  left.1 == right.1 ? left.0.id < right.0.id : left.1 < right.1
}

private func stableSeed(for id: String, base: UInt32) -> UInt32 {
  var value: UInt32 = 2_166_136_261
  for byte in id.utf8 { value = (value ^ UInt32(byte)) &* 16_777_619 }
  return (value ^ base) &* 16_777_619
}

private func stableDirection(for id: String) -> SIMD2<Float> {
  let unit = Float(stableSeed(for: id, base: 91) >> 8) / 16_777_216
  let angle = unit * 2 * Float.pi
  return SIMD2(cos(angle), sin(angle))
}

private func boundedWorldPoint(_ point: SIMD2<Float>) -> SIMD2<Float> {
  SIMD2(
    min(SanctuaryGeography.bounds.maximum.x, max(SanctuaryGeography.bounds.minimum.x, point.x)),
    min(SanctuaryGeography.bounds.maximum.y, max(SanctuaryGeography.bounds.minimum.y, point.y)))
}

private func signal(for actor: WildlifeActor) -> WildlifeSignal {
  switch actor.simulation.state {
  case "watching": return .watching
  case "greeting": return .greeting
  case "come", "coming": return .approaching
  case "follow", "following": return .following
  case "wait", "waiting": return .waiting
  case "play", "playing": return .playing
  case "declining": return .refusing
  case "fleeing": return .fleeing
  case "relocating": return .seekingHabitat
  case "habitat-work": return .workingHabitat
  default: return .resting
  }
}
