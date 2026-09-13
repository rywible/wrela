import FieldCore
import Foundation
import SimulationCore
import simd

/// CPU simulations publish collision immediately. The native host commits the matching
/// visual source, collision and ground support together after GPU upload completes.
public enum RegionalCollisionPublicationPolicy: String, Sendable {
  case immediate, hostCommitted
}

public final class SanctuaryWorld: GameSimulation {

  public var legacyInteractions = false
  public private(set) var regionalCollisionPublicationPolicy: RegionalCollisionPublicationPolicy = .immediate
  private var regionalPublicationGeneration: UInt64 = 0
  private var regionalPendingKeys: Set<SanctuaryTerrainChunkKey> = []
  private var publishedSupportGarden: HabitatGarden?
  private var publishedSupportBoulders: BoulderArrangements?

  public var presentationGarden: HabitatGarden? {
    regionalCollisionPublicationPolicy == .hostCommitted ? publishedSupportGarden : controller.state.garden
  }

  public func enableHostCommittedRegionalCollision(
    initialLayout: SanctuaryLayout, garden: HabitatGarden?, boulders: BoulderArrangements?
  ) {
    world = initialLayout
    regionalCollisionPublicationPolicy = .hostCommitted
    publishedSupportGarden = garden
    publishedSupportBoulders = boulders
    regionalPublicationGeneration = 0
    regionalPendingKeys.removeAll()
  }

  public func markRegionalPublicationPending(
    generation: UInt64, affectedKeys: Set<SanctuaryTerrainChunkKey>
  ) {
    guard regionalCollisionPublicationPolicy == .hostCommitted,
      generation >= regionalPublicationGeneration else { return }
    if generation == regionalPublicationGeneration { regionalPendingKeys.formUnion(affectedKeys) }
    else { regionalPendingKeys = affectedKeys }
    regionalPublicationGeneration = generation
  }

  @discardableResult
  public func publishRegionalState(
    generation: UInt64, layout: SanctuaryLayout, garden: HabitatGarden?, boulders: BoulderArrangements?
  ) -> Bool {
    guard regionalCollisionPublicationPolicy == .hostCommitted,
      generation == regionalPublicationGeneration,
      layout.terrain.recipe == world.terrain.recipe else { return false }
    // Residency and decoration publication do not change the supporting surface. In
    // particular, a restored valid center-supported camera must not be silently moved
    // to the five-probe standing height merely because a near tier finished uploading.
    let previousSupport = publishedEyeHeight()
    world = layout
    publishedSupportGarden = garden
    publishedSupportBoulders = boulders
    regionalPendingKeys.removeAll()
    let nextSupport = publishedEyeHeight()
    // Compare actual local support, not saved revision numbers: undo and restored
    // same-revision sculpt/water branches can still change the ground under the player.
    if nextSupport != previousSupport { position.y = nextSupport }
    return true
  }

  public func regionalMovementIsPublished(at point: SIMD2<Float>) -> Bool {
    guard point.x.isFinite, point.y.isFinite else { return false }
    guard regionalCollisionPublicationPolicy == .hostCommitted else { return true }
    return [SIMD2<Float>.zero, SIMD2(0.3, 0), SIMD2(-0.3, 0),
      SIMD2(0, 0.3), SIMD2(0, -0.3)].allSatisfy { offset in
        let key = SanctuaryTerrainChunkKey(containing: point + offset)
        return world.streamedCollisionKeys.contains(key) && !regionalPendingKeys.contains(key)
      }
  }

  func publishedStandingHeight(at point: SIMD2<Float>) -> Float {
    [SIMD2<Float>.zero, SIMD2(0.25, 0), SIMD2(-0.25, 0),
      SIMD2(0, 0.25), SIMD2(0, -0.25)].map { offset in
        let p = point + offset
        return groundHeight(p.x, p.y)
      }.max()!
  }

  func publishedEyeHeight() -> Float {
    let travel = controller.state.travel
    let point = SIMD2(position.x, position.z)
    if travel.mode == .flying { return flightSurfaceHeight(point.x, point.y) + travel.flightHeight }
    return constructionGround(at: position, terrain: publishedStandingHeight(at: point))
      + (travel.mode == .riding ? 2.5 : 1.72)
  }
  private var collisionGardenRevision: UInt64?
  private var collisionBoulderRevision: UInt64?
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
    let loadedController = try ExpeditionController(root: root, seed: seed)
    let s = loadedController.state
    let terrain = try Terrain(recipe: s.resolvedTerrainRecipe)
    controller = loadedController
    world = SanctuaryLayout(parameters: parameters, terrain: terrain)
    camera = PlayerCamera(
      position: V3(
        s.player.x, s.playerElevation ?? ((s.garden ?? HabitatGarden()).surfaceHeight(baseHeight: world.terrain.height(s.player.x, s.player.y), at: .init(x: s.player.x, z: s.player.y)) + 1.72),
        s.player.y),
      yaw: s.yaw, pitch: s.pitch)
    reconcileLoadedPlayerElevation()
  }
  var podPosition: V3 {
    V3(
      5,
      groundHeight(5, 12) - SanctuaryLayout.podShape.bounds.min.y * podScale
        * SanctuaryLayout.podUnitScale, 12)
  }
  private struct Snapshot: Codable {
    var version = 2
    var legacyInteractions: Bool?
    var expedition: Data
    var camera: PlayerCamera
    var podScale: Float
    var motion: CreatureMotion
  }
  public func checkpoint() throws -> Data {
    syncExpeditionPlayer()
    return try SimulationCoding.encode(
      Snapshot(
        legacyInteractions: legacyInteractions, expedition: controller.checkpoint(), camera: camera, podScale: podScale, motion: motion))
  }
  public func restore(_ data: Data) throws {
    let s = try JSONDecoder().decode(Snapshot.self, from: data)
    try SimulationCoding.validateCamera(s.camera)
    try s.motion.validate()
    guard (1...2).contains(s.version), s.podScale.isFinite, (0.25...4).contains(s.podScale)
    else { throw ExpeditionError.invalidSave }
    let memory = try JSONDecoder().decode(ExpeditionController.Memory.self, from: s.expedition)
    let saved = memory.state
    // Resolve before support queries or mutation. Cross-recipe restores require a future
    // transactional world rebuild; this version only supports the same legacy source.
    let restoredTerrain = try Terrain(recipe: saved.resolvedTerrainRecipe)
    guard restoredTerrain.recipe == world.terrain.recipe else {
      throw ExpeditionError.unsupportedVersion
    }
    let point = SIMD2(s.camera.position.x, s.camera.position.z)
    let garden = saved.garden ?? HabitatGarden()
    let centerGround = garden.surfaceHeight(baseHeight: world.terrain.height(point.x, point.y),
      at: .init(x: point.x, z: point.y))
    let travel = saved.travel
    var ground = travel.mode == .flying ? centerGround : standingTerrainHeight(at: point, state: saved)
    if travel.mode == .flying { ground = max(ground, localWaterHeight(point.x, point.y, state: saved) ?? ground) }
    let eye: Float = travel.mode == .flying ? travel.flightHeight : travel.mode == .riding ? 2.5 : 1.72
    for fact in saved.buildings.collisionFacts + SanctuaryHome.construction.collisionFacts where fact.kind == .walkable {
      if fact.contains(.init(x: point.x, y: fact.top, z: point.y)),
        abs(s.camera.position.y - fact.top - eye) < 0.5 { ground = max(ground, fact.top) }
    }
    // Older edits and fixture snapshots used the terrain's center sample. Keep those saves
    // readable, while accepting the same footprint-supported eye used by ordinary movement.
    let legacyCenterPose = travel.mode != .flying
      && abs(s.camera.position.y - centerGround - eye) < 0.55
    guard distance(point, saved.player) < 0.001,
      abs(s.camera.position.y - ground - eye) < 0.55 || legacyCenterPose,
      travel.companionID.map({ saved.population.actor(id: $0) != nil }) ?? true
    else { throw ExpeditionError.invalidSave }
    try controller.restore(s.expedition)
    legacyInteractions = s.legacyInteractions ?? true
    camera = s.camera
    podScale = s.podScale
    motion = s.motion
    if !legacyInteractions { updateCollisionStreaming(at: point) }
  }
  public func validate() throws {
    syncExpeditionPlayer()
    try controller.state.validate()
    try SimulationCoding.validateCamera(camera)
  }
  public var observations: [String: String] {
    let s = controller.state
    return [
      "terrainRecipe": world.terrain.recipe.identity,
      "phase": s.phase.rawValue, "trust": String(s.trust), "habitat": String(s.habitat),
      "fear": String(s.creatureState.fear), "behavior": s.creatureState.state,
      "visible": String(creatureVisible), "creatureID": Expedition.creatureID,
      "signs": String(s.discoveredSigns.count), "x": String(position.x), "y": String(position.y),
      "z": String(position.z),
      "regionalCollisionPublicationPolicy": regionalCollisionPublicationPolicy.rawValue,
      "regionalPublicationGeneration": String(regionalPublicationGeneration),
      "regionalPendingChunkCount": String(regionalPendingKeys.count),
      "streamedCollisionChunkIDs": world.streamedCollisionKeys.sorted {
        $0.z == $1.z ? $0.x < $1.x : $0.z < $1.z
      }.map(\.id).joined(separator: ","),
    ].merging(livingObservations) { _, value in value }
  }
  public func groundHeight(_ x: Float, _ z: Float) -> Float {
    let base = world.terrain.height(x, z)
    return (presentationGarden ?? HabitatGarden()).surfaceHeight(
      baseHeight: base, at: .init(x: x, z: z))
  }
  /// Ground support for the player's finite foot area, shared by movement, edits and saves.
  func standingTerrainHeight(at point: SIMD2<Float>, state: Expedition) -> Float {
    let garden = state.garden ?? HabitatGarden()
    return [SIMD2<Float>.zero, SIMD2(0.25, 0), SIMD2(-0.25, 0),
      SIMD2(0, 0.25), SIMD2(0, -0.25)].map { offset in
      let p = point + offset
      return garden.surfaceHeight(baseHeight: world.terrain.height(p.x, p.y),
        at: .init(x: p.x, z: p.y))
    }.max()!
  }
  func updateCollisionStreaming(at point: SIMD2<Float>) {
    guard regionalCollisionPublicationPolicy == .immediate else { return }
    do {
      try world.updateImmediateCollision(around: point, garden: controller.state.garden,
        boulders: controller.state.boulders, construction: controller.state.buildings)
      collisionGardenRevision = controller.state.garden?.revision
      collisionBoulderRevision = controller.state.boulders?.revision
    } catch {
      // Source assembly is atomic: preserve the prior complete layout on invalid input.
      controller.message = "World collision: \(error.localizedDescription)"
    }
  }
  public func move(_ local: V3) {
    guard [local.x, local.y, local.z].allSatisfy({ $0.isFinite && abs($0) <= 20 }) else { return }
    if !legacyInteractions { updateCollisionStreaming(at: SIMD2(position.x, position.z)) }

    let right = V3(cos(yaw), 0, sin(yaw))
    let forward = V3(sin(yaw), 0, -cos(yaw))
    let mode = controller.state.travel.mode
    let requestedGroundMotion = mode != .flying
      && length_squared(SIMD2<Float>(local.x, local.z)) > 0.00000001
    let supportOffset: Float = mode == .riding ? 2.5 : 1.72
    var groundSweeps: [(SIMD3<Float>, SIMD3<Float>)] = []
    let speed: Float = mode == .riding ? 3 : mode == .flying ? 12 : 1
    let delta = (right * local.x + forward * local.z) * speed
    let count = max(1, Int(ceil(length(delta) / 0.15)))
    for _ in 0..<count {
      let resolvedStart = position
      var p = position + delta / Float(count)
      p.x = clamp(p.x, SanctuaryGeography.bounds.minimum.x, SanctuaryGeography.bounds.maximum.x)
      p.z = clamp(p.z, SanctuaryGeography.bounds.minimum.y, SanctuaryGeography.bounds.maximum.y)
      guard regionalMovementIsPublished(at: SIMD2(p.x, p.z)) else { continue }
      if mode == .flying && !legacyInteractions {
        let altitude = controller.state.travel.flightHeight
        p.y = flightSurfaceHeight(p.x, p.z) + altitude
        position = p
        continue
      }
      var ground = publishedStandingHeight(at: SIMD2(p.x, p.z))
      if !legacyInteractions {
        ground = constructionGround(at: p, terrain: ground)
        if constructionBlocks(p, ground: ground)
          || ecologyBlocks(from: SIMD2(position.x, position.z), to: SIMD2(p.x, p.z)) { continue }
        if let water = localWaterHeight(p.x, p.z), water > ground + 0.5 { continue }
      }
      p.y = ground + (mode == .riding ? 2.5 : 1.72)
      let beforeCollision = SIMD2(p.x, p.z)
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
      if beforeCollision != SIMD2(p.x, p.z) {
        ground = publishedStandingHeight(at: SIMD2(p.x, p.z))
        if !legacyInteractions { ground = constructionGround(at: p, terrain: ground) }
      }
      p.y = ground + (mode == .riding ? 2.5 : 1.72)
      guard regionalMovementIsPublished(at: SIMD2(p.x, p.z)) else { continue }
      position = p
      if requestedGroundMotion && SIMD2(resolvedStart.x, resolvedStart.z) != SIMD2(p.x, p.z) {
        groundSweeps.append((
          SIMD3(resolvedStart.x, resolvedStart.y - supportOffset, resolvedStart.z),
          SIMD3(p.x, p.y - supportOffset, p.z)))
      }
    }
    if !legacyInteractions { syncMountedCompanion() }
    recordPlayerGroundSweeps(groundSweeps, mode: mode)
  }

  public func advance(_ seconds: Float, running: Bool) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    if !legacyInteractions { updateCollisionStreaming(at: SIMD2(position.x, position.z)) }
    syncExpeditionPlayer()
    if legacyInteractions {
      controller.advance(seconds, running: running, visible: creatureVisible, motion: motion,
        perceived: creatureLineOfSight, canTraverse: creatureCanTraverse)
    } else {
      let populationBefore = controller.state.population
      advanceLiving(seconds, running: running)
      recordWildlifeGroundMovement(
        from: populationBefore, to: controller.state.population, elapsedSeconds: seconds)
      controller.advanceLivingClock(seconds)
    }
  }

  public func creatureCanTraverse(_ a: SIMD2<Float>, _ b: SIMD2<Float>) -> Bool {
    let start = groundHeight(a.x, a.y)
    if !legacyInteractions && ecologyBlocks(from: a, to: b) { return false }
    for i in 0...6 {
      let p = a + (b - a) * Float(i) / 6
      guard regionalMovementIsPublished(at: p) else { return false }
      let ground = groundHeight(p.x, p.y)
      if abs(ground - start) > 0.28 { return false }
      for height: Float in [0.3, 0.65] {
        let q = V3(p.x, ground + height, p.y)
        for index in world.collisionGrid.candidates(at: q) where world.solids[index].value(q) < 0.35
        { return false }
      }
    }
    return true
  }

  /// Species-aware route validation for the living population. Ground species
  /// that author shallow water as avoided require a saved bridge or deck over a
  /// wet sample. Wet-habitat and flying species retain their authored access.
  public func creatureCanTraverse(
    _ actor: WildlifeActor, _ a: SIMD2<Float>, _ b: SIMD2<Float>
  ) -> Bool {
    let startGround = groundHeight(a.x, a.y)
    let startWalkable = creatureWalkableSupport(at: a)
    let startSupport = max(startGround, startWalkable ?? startGround)
    let avoidsWater = !actor.species.sanctuaryUsesWaterSupport
      && actor.species.profile.avoidedPlanting == .shallowWater
    func unsupportedWaterDepth(
      at point: SIMD2<Float>, ground: Float, walkable: Float?
    ) -> Float {
      guard walkable == nil, let water = localWaterHeight(point.x, point.y) else { return 0 }
      return max(0, water - ground)
    }
    let startWaterDepth = unsupportedWaterDepth(
      at: a, ground: startGround, walkable: startWalkable)
    let endGround = groundHeight(b.x, b.y)
    let endWalkable = creatureWalkableSupport(at: b)
    let endWaterDepth = unsupportedWaterDepth(at: b, ground: endGround, walkable: endWalkable)
    // Historical wet positions must be able to complete a saved dry-bank
    // recovery, but they cannot use that exception to move laterally or deeper.
    let escapingWater = avoidsWater && startWaterDepth > 0.005
      && endWaterDepth + 0.005 < startWaterDepth
    for i in 0...6 {
      let p = a + (b - a) * Float(i) / 6
      guard regionalMovementIsPublished(at: p) else { return false }
      let ground = groundHeight(p.x, p.y)
      let walkable = creatureWalkableSupport(at: p)
      let support = max(ground, walkable ?? ground)
      let supportedByWalkable = walkable != nil
      let maximumStep: Float = supportedByWalkable || startWalkable != nil ? 0.5 : 0.28
      if abs(support - startSupport) > maximumStep { return false }
      if avoidsWater {
        let waterDepth = unsupportedWaterDepth(at: p, ground: ground, walkable: walkable)
        if waterDepth > 0.005,
          (!escapingWater || waterDepth > startWaterDepth + 0.005)
        { return false }
      }
      let standingPoint = PersonalConstruction.Location(x: p.x, y: support, z: p.y)
      for fact in constructionFacts where fact.kind == .solid
        && fact.intersects(at: standingPoint, from: support + 0.1, through: support + 0.65)
      { return false }
      for height: Float in [0.3, 0.65] {
        let q = V3(p.x, support + height, p.y)
        for index in world.collisionGrid.candidates(at: q)
          where world.solids[index].value(q) < 0.35
        { return false }
      }
    }
    return !ecologyBlocks(from: a, to: b)
  }
  public var creatureVisible: Bool {
    guard let expedition else { return false }
    let p = expedition.state.creaturePosition
    let delta = V3(p.x, groundHeight(p.x, p.y) + 0.8, p.y) - position
    return length_squared(delta) > 0.001 && dot(normalize(delta), forward) > 0.3
      && creatureLineOfSight
  }
  public var creatureLineOfSight: Bool {
    guard let expedition else { return false }
    let p = expedition.state.creaturePosition
    let target = V3(p.x, groundHeight(p.x, p.y) + 0.8, p.y)
    let delta = target - position
    guard length_squared(delta) > 0.001, length_squared(delta) < 100
    else { return false }
    for i in 1..<20 {
      let q = position + delta * (Float(i) / 20)
      if q.y < groundHeight(q.x, q.z) { return false }
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
    return legacyInteractions ? try expedition.interact(visible: creatureVisible) : try interactLiving()
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
      state.playerElevation ?? (groundHeight(state.player.x, state.player.y) + 1.72),
      state.player.y)
    yaw = state.yaw
    pitch = state.pitch
    reconcileLoadedPlayerElevation()
  }

  /// A content update may change the source terrain beneath a saved position.
  /// Preserve valid historical poses, but reattach obsolete heights to the real
  /// support surface when opening a slot. Strict same-source replay stays in restore(_:).
  private func reconcileLoadedPlayerElevation() {
    let supported = resolvedEyeHeight(state: controller.state)
    if abs(position.y - supported) > 0.55 { position.y = supported }
    syncExpeditionPlayer()
  }
}
