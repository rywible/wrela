import FieldCore
import Foundation
import SanctuaryContent
import SimulationCore
import TestKit
import simd

/// Exact operation and source counts from a residency traversal. Wall time and renderer/collision
/// assembly time are deliberately absent; the test runner must measure them around this workload.
public struct SanctuaryVegetationResidencyWorkloadMetrics: Codable, Equatable, Sendable {
  public let name: String
  public let startCoordinate: [Float]
  public let endCoordinate: [Float]
  public let requestToken: UInt64
  public let desiredChunkCount: Int
  public let generatedChunkCount: Int
  public let reusedChunkCount: Int
  public let candidateCellsGenerated: Int
  public let communityClimateSamplesGenerated: Int
  public let exactSourceRecordsGenerated: Int
  public let collisionSourceRecordsGenerated: Int
  public let distinctSourceIDCount: Int
  public let duplicateSourceIDCount: Int
  public let parentSummariesGenerated: Int
  public let canopySummariesGenerated: Int
  public let cacheHighWaterChunks: Int
  public let publicationCount: Int
  public let stalePublicationCount: Int
  public let finalDetailedRecordCount: Int
  public let finalCommunitySummaryCount: Int
  public let finalCanopySummaryCount: Int
  public let finalSourceIDCount: Int
  public let finalDistinctSourceIDCount: Int
  public let finalDuplicateSourceIDCount: Int
  public let finalSnapshotRevision: UInt64
  public let finalSnapshotComplete: Bool
}

public struct SanctuaryVegetationCancellationWorkloadMetrics: Codable, Equatable, Sendable {
  public let cancelledToken: UInt64
  public let stalePublicationCount: Int
  public let snapshotRevisionBefore: UInt64
  public let snapshotRevisionAfter: UInt64
  public let cachedChunksBefore: Int
  public let cachedChunksAfter: Int
  public let publishedSourceIDsUnchanged: Bool
}

public struct SanctuaryVegetationEditWorkloadMetrics: Codable, Equatable, Sendable {
  public let sourceID: String
  public let parentContainsSourceID: Bool
  public let canopyContainsSourceID: Bool
  public let dryIncluded: Bool
  public let persistedWaterExclusion: String?
  public let constructionExclusion: String?
  public let sourceRecordUnchangedAfterEdits: Bool
  public let collisionSource: Bool
}

public struct SanctuaryVegetationBoundaryEditWorkloadMetrics: Codable, Equatable, Sendable {
  public let westCoordinate: [Float]
  public let eastCoordinate: [Float]
  public let westToken: UInt64
  public let eastToken: UInt64
  public let reverseToken: UInt64
  public let eastGeneratedChunkCount: Int
  public let reverseGeneratedChunkCount: Int
  public let stableOverlapChunkCount: Int
  public let stableOverlapSourceIDCount: Int
  public let mismatchedOverlapChunkCount: Int
  public let stalePublicationCount: Int
  public let stalePublicationPreservedSnapshot: Bool
  public let persistedEditSourceID: String
  public let persistedSourceRecordUnchanged: Bool
  public let sameRevisionReceiptsDistinct: Bool
  public let sculptHeightDelta: Float
  public let excludedFromPresentationSource: Bool
  public let excludedFromCollisionSource: Bool
  public let blockedWhilePending: Bool
  public let crossedBoundary: Bool
  public let reversedBoundary: Bool
  public let detailAndCollisionChunkIDsEqual: Bool
}

public struct SanctuaryVegetationFastTraversalWorkloadMetrics: Codable, Equatable, Sendable {
  public let travelModes: [String]
  public let walkingDistanceMetres: Float
  public let ridingDistanceMetres: Float
  public let flyingDistanceMetres: Float
  public let crossedWalkingChunkBoundary: Bool
  public let crossedRidingChunkBoundary: Bool
  public let crossedFlyingChunkBoundary: Bool
  public let collisionNeighborhoodsMatched: Bool
  public let generatedChunkCount: Int
  public let reusedChunkCount: Int
  public let cacheHighWaterChunks: Int
  public let maximumScheduledJobsPerTurn: Int
  public let finalSnapshotComplete: Bool
}

/// Reproducible CPU source workloads. They execute the real population/residency implementation
/// one bounded chunk job at a time and return facts for the harness artifact. They do not claim a
/// frame time, GPU upload cost, resolved collision count, or native visual result.
public enum SanctuaryVegetationResidencyWorkloads {
  public static let rainforestCenter = SIMD2<Float>(8_300, -3_000)

  /// Performance runner entries. Root registers these directly in WrelaTest so they remain
  /// source workloads instead of pretending to be fixed-tick `GameSimulation` workloads.
  public static var performanceWorkloads: [Workload] {
    [
      Workload("vegetation-residency-cold",
        definition: "default radii 1/2/4; one chunk job; cold 9x9 residency") {
        let value = try cold()
        guard value.desiredChunkCount == 81, value.generatedChunkCount == 81,
          value.duplicateSourceIDCount == 0, value.finalDuplicateSourceIDCount == 0,
          value.finalSnapshotComplete
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return checksum(value)
      },
      Workload("vegetation-residency-entering-strip",
        definition: "warm default residency; traverse east exactly one 512m chunk") {
        let value = try enteringStrip()
        guard value.desiredChunkCount == 81, value.generatedChunkCount == 9,
          value.reusedChunkCount == 72, value.duplicateSourceIDCount == 0,
          value.finalDuplicateSourceIDCount == 0, value.finalSnapshotComplete
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return checksum(value)
      },
      Workload("vegetation-residency-cancel",
        definition: "warm residency; compile one entering job; cancel before publication") {
        let value = try cancellation()
        guard value.stalePublicationCount == 1,
          value.snapshotRevisionBefore == value.snapshotRevisionAfter,
          value.cachedChunksBefore == value.cachedChunksAfter,
          value.publishedSourceIDsUnchanged
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return Double(value.cachedChunksAfter + value.stalePublicationCount)
      },
      Workload("vegetation-residency-persistent-edit",
        definition: "real exact record; persisted local water and construction exclusions") {
        let value = try localPersistentEdits()
        guard value.parentContainsSourceID, value.canopyContainsSourceID, value.dryIncluded,
          value.persistedWaterExclusion == SanctuaryVegetationLocalExclusion.authoredShallowWater.rawValue,
          value.constructionExclusion == SanctuaryVegetationLocalExclusion.construction.rawValue,
          value.sourceRecordUnchangedAfterEdits, value.collisionSource
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return Double(value.sourceID.utf8.reduce(UInt64(0)) { $0 + UInt64($1) })
      },
      Workload("vegetation-residency-boundary-edit-reversal",
        definition: "cross and reverse one 512m boundary; persisted sculpt/water edit; stale host and residency tokens") {
        let value = try boundaryEditReversal()
        guard value.eastGeneratedChunkCount == 9, value.reverseGeneratedChunkCount == 9,
          value.stableOverlapChunkCount == 72, value.stableOverlapSourceIDCount > 0,
          value.mismatchedOverlapChunkCount == 0, value.stalePublicationCount == 1,
          value.stalePublicationPreservedSnapshot, value.persistedSourceRecordUnchanged,
          value.sameRevisionReceiptsDistinct, value.sculptHeightDelta > 1,
          value.excludedFromPresentationSource, value.excludedFromCollisionSource,
          value.blockedWhilePending, value.crossedBoundary, value.reversedBoundary,
          value.detailAndCollisionChunkIDsEqual
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return Double(value.stableOverlapSourceIDCount + value.eastGeneratedChunkCount
          + value.reverseGeneratedChunkCount + value.stalePublicationCount)
      },
      Workload("vegetation-residency-fast-traversal",
        definition: "production walk/ride/fly movement; flying 512m boundary; default 9x9 residency, one chunk job per turn") {
        let value = try fastTraversal()
        guard value.travelModes == ["walking", "riding", "flying"],
          value.walkingDistanceMetres > 19, value.ridingDistanceMetres > 8,
          value.flyingDistanceMetres > 239, value.crossedWalkingChunkBoundary,
          value.crossedRidingChunkBoundary, value.crossedFlyingChunkBoundary,
          value.collisionNeighborhoodsMatched, value.generatedChunkCount == 90,
          value.reusedChunkCount == 72, value.cacheHighWaterChunks <= 162,
          value.maximumScheduledJobsPerTurn == 1, value.finalSnapshotComplete
        else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }
        return Double(value.generatedChunkCount + value.reusedChunkCount)
          + Double(value.walkingDistanceMetres + value.ridingDistanceMetres
            + value.flyingDistanceMetres)
      },
    ]
  }

  public static func cold(
    center: SIMD2<Float> = rainforestCenter
  ) throws -> SanctuaryVegetationResidencyWorkloadMetrics {
    var residency = SanctuaryBiomeVegetationResidency()
    return try complete(
      name: "vegetation-residency-cold", residency: &residency,
      start: center, end: center)
  }

  /// Warms the complete default residency, then moves exactly one 512 m chunk east. The returned
  /// generation counts therefore measure only the entering strip; overlap is served from cache.
  public static func enteringStrip(
    start: SIMD2<Float> = rainforestCenter
  ) throws -> SanctuaryVegetationResidencyWorkloadMetrics {
    var residency = SanctuaryBiomeVegetationResidency()
    _ = try complete(
      name: "vegetation-residency-warmup", residency: &residency,
      start: start, end: start)
    let end = start + SIMD2<Float>(SanctuaryTerrainChunkKey.size, 0)
    return try complete(
      name: "vegetation-residency-entering-strip", residency: &residency,
      start: start, end: end)
  }

  /// A compiled result is cancelled before publication. Publishing that stale token must preserve
  /// the prior complete snapshot, source IDs, revision and cache occupancy exactly.
  public static func cancellation(
    start: SIMD2<Float> = rainforestCenter
  ) throws -> SanctuaryVegetationCancellationWorkloadMetrics {
    var residency = SanctuaryBiomeVegetationResidency()
    _ = try complete(
      name: "vegetation-residency-cancel-warmup", residency: &residency,
      start: start, end: start)
    let request = try residency.beginRequest(
      around: start + SIMD2<Float>(SanctuaryTerrainChunkKey.size, 0))
    // Beginning a nearby request may coherently re-tier already cached children. Cancellation
    // protects this published baseline from the subsequently compiled in-flight result.
    let before = residency.snapshot
    let beforeCache = residency.cachedChunkCount
    guard let job = residency.nextJobs(for: request.token, budget: .oneChunk).first else {
      throw SanctuaryVegetationResidencyWorkloadError.missingJob
    }
    let result = try residency.compile(job)
    guard residency.cancel(requestToken: request.token) else {
      throw SanctuaryVegetationResidencyWorkloadError.cancellationRejected
    }
    let publication = residency.publish(result)
    let after = residency.snapshot
    return SanctuaryVegetationCancellationWorkloadMetrics(
      cancelledToken: request.token,
      stalePublicationCount: publication == .stale ? 1 : 0,
      snapshotRevisionBefore: before.revision, snapshotRevisionAfter: after.revision,
      cachedChunksBefore: beforeCache, cachedChunksAfter: residency.cachedChunkCount,
      publishedSourceIDsUnchanged: sourceIDs(in: before) == sourceIDs(in: after))
  }

  /// Uses a real exact record, round-trips the authored water edit through Codable persistence,
  /// and applies a construction footprint independently. Both decisions retain the source ID.
  public static func localPersistentEdits(
    center: SIMD2<Float> = rainforestCenter
  ) throws -> SanctuaryVegetationEditWorkloadMetrics {
    let configuration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 0, middleChunkRadius: 1, farChunkRadius: 1)
    var residency = SanctuaryBiomeVegetationResidency(configuration: configuration)
    let request = try residency.beginRequest(around: center)
    var source: (SanctuaryVegetationCompiledChunk, SanctuaryVegetationRecord)?
    while let job = residency.nextJobs(for: request.token, budget: .oneChunk).first {
      let compiled = try residency.compile(job)
      _ = residency.publish(compiled)
      if source == nil, let record = compiled.exact.first(where: \.providesCollision) {
        source = (compiled, record)
      }
    }
    guard let (compiled, record) = source else {
      throw SanctuaryVegetationResidencyWorkloadError.missingSourceRecord
    }
    let dry = SanctuaryVegetationLocalEditMask().decision(for: record)

    var garden = HabitatGarden()
    _ = try garden.apply(
      .plant(.shallowWater,
        at: .init(x: record.coordinate.x, z: record.coordinate.y), radius: 0.5),
      expectedRevision: garden.revision)
    let persistedGarden = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(garden))
    let water = SanctuaryVegetationLocalEditMask(garden: persistedGarden).decision(for: record)

    var construction = PersonalConstruction()
    _ = try construction.apply(
      .place(.path,
        at: .init(x: record.coordinate.x, y: 0, z: record.coordinate.y),
        yawRadians: 0, scale: 1), expectedRevision: construction.revision)
    let persistedConstruction = try JSONDecoder().decode(
      PersonalConstruction.self, from: JSONEncoder().encode(construction))
    let built = SanctuaryVegetationLocalEditMask(
      construction: persistedConstruction).decision(for: record)
    let repeated = try residency.population.query(in: compiled.canopy.bounds).exact.first {
      $0.id == record.id
    }
    return SanctuaryVegetationEditWorkloadMetrics(
      sourceID: record.id,
      parentContainsSourceID: compiled.parents.flatMap(\.childIDs).contains(record.id),
      canopyContainsSourceID: compiled.canopy.childIDs.contains(record.id),
      dryIncluded: dry.isIncluded,
      persistedWaterExclusion: water.exclusion?.rawValue,
      constructionExclusion: built.exclusion?.rawValue,
      sourceRecordUnchangedAfterEdits: repeated == record,
      collisionSource: record.providesCollision)
  }

  /// Crosses and reverses one canonical chunk boundary through the real residency and
  /// SanctuaryWorld publication APIs. The saved edit changes support and local inclusion without
  /// regenerating its deterministic source record. Renderer upload timing remains project-owned.
  public static func boundaryEditReversal(
    near rainforest: SIMD2<Float> = rainforestCenter
  ) throws -> SanctuaryVegetationBoundaryEditWorkloadMetrics {
    let centerKey = SanctuaryTerrainChunkKey(containing: rainforest)
    let boundaryX = Float(centerKey.x) * SanctuaryTerrainChunkKey.size
    let west = SIMD2<Float>(boundaryX - 0.45, rainforest.y)
    let east = SIMD2<Float>(boundaryX + 0.45, rainforest.y)
    var residency = SanctuaryBiomeVegetationResidency()
    let westMetrics = try complete(
      name: "vegetation-residency-boundary-west", residency: &residency,
      start: west, end: west)
    let westSnapshot = residency.snapshot

    let eastRequest = try residency.beginRequest(around: east)
    var firstEastResult: SanctuaryVegetationCompiledChunk?
    var eastGenerated = 0
    while let job = residency.nextJobs(for: eastRequest.token, budget: .oneChunk).first {
      let result = try residency.compile(job)
      if firstEastResult == nil { firstEastResult = result }
      eastGenerated += 1
      _ = residency.publish(result)
    }
    guard residency.snapshot.isComplete, let staleResult = firstEastResult else {
      throw SanctuaryVegetationResidencyWorkloadError.missingJob
    }
    let eastSnapshot = residency.snapshot
    let westByChunk = sourceIDsByChunk(in: westSnapshot)
    let eastByChunk = sourceIDsByChunk(in: eastSnapshot)
    let overlap = Set(westByChunk.keys).intersection(eastByChunk.keys)
    let mismatches = overlap.filter { westByChunk[$0] != eastByChunk[$0] }
    let stableOverlapIDs = overlap.reduce(0) { $0 + (westByChunk[$1]?.count ?? 0) }

    let reverseRequest = try residency.beginRequest(around: west)
    let beforeStale = residency.snapshot
    let stale = residency.publish(staleResult)
    let afterStale = residency.snapshot
    var reverseGenerated = 0
    while let job = residency.nextJobs(for: reverseRequest.token, budget: .oneChunk).first {
      let result = try residency.compile(job)
      _ = residency.publish(result)
      reverseGenerated += 1
    }
    let eastCenterKey = SanctuaryTerrainChunkKey(containing: east)
    guard residency.snapshot.isComplete,
      let source = eastSnapshot.detailedRecords.first(where: {
        $0.providesCollision && SanctuaryTerrainChunkKey(containing: $0.coordinate) == eastCenterKey
      })
    else { throw SanctuaryVegetationResidencyWorkloadError.missingSourceRecord }

    let editLocation = HabitatGarden.Location(x: source.coordinate.x, z: source.coordinate.y)
    var waterBranch = HabitatGarden()
    _ = try waterBranch.apply(
      .sculpt(.raise, at: editLocation, radius: 6, amount: 2, targetHeight: nil),
      expectedRevision: 0)
    _ = try waterBranch.apply(
      .plant(.shallowWater, at: editLocation, radius: 2),
      expectedRevision: waterBranch.revision)
    let persistedWater = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(waterBranch))
    var flowerBranch = HabitatGarden()
    _ = try flowerBranch.apply(
      .sculpt(.raise, at: editLocation, radius: 6, amount: 2, targetHeight: nil),
      expectedRevision: 0)
    _ = try flowerBranch.apply(
      .plant(.flowers, at: editLocation, radius: 2),
      expectedRevision: flowerBranch.revision)
    let persistedFlowers = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(flowerBranch))
    let sourceKey = SanctuaryTerrainChunkKey(containing: source.coordinate)
    let waterReceipt = SanctuaryRegionalSourceReceipt(
      center: sourceKey, garden: persistedWater, boulders: nil, construction: nil)
    let flowerReceipt = SanctuaryRegionalSourceReceipt(
      center: sourceKey, garden: persistedFlowers, boulders: nil, construction: nil)
    let decision = SanctuaryVegetationLocalEditMask(garden: persistedWater).decision(for: source)
    let repeated = try residency.population.query(in: chunkBounds(sourceKey)).exact.first {
      $0.id == source.id
    }
    let baseHeight = Terrain().height(source.coordinate.x, source.coordinate.y)
    let editedHeight = persistedWater.surfaceHeight(baseHeight: baseHeight, at: editLocation)

    let simulation = try SanctuaryWorld()
    simulation.legacyInteractions = false
    simulation.camera.position = SIMD3(
      west.x, simulation.groundHeight(west.x, west.y) + 1.72, west.y)
    let geography = SanctuaryGeography()
    let initialPlans = SanctuaryTerrainChunkKey.neighborhood(around: west).map {
      geography.plan(for: $0)
    }
    var initialLayout = SanctuaryLayout(terrain: simulation.world.terrain)
    initialLayout.setStreamedCollision(plans: initialPlans)
    simulation.enableHostCommittedRegionalCollision(
      initialLayout: initialLayout, garden: nil, boulders: nil)
    let affected = SanctuaryGardenSurfaceSource(garden: persistedWater).affectedChunkKeys(
      comparedTo: SanctuaryGardenSurfaceSource(garden: persistedFlowers))
    let detailPlans = SanctuaryTerrainChunkKey.neighborhood(around: east).map {
      geography.plan(for: $0)
    }
    var obsoleteLayout = SanctuaryLayout(terrain: simulation.world.terrain)
    obsoleteLayout.setStreamedCollision(plans: detailPlans, garden: persistedFlowers)
    var currentLayout = SanctuaryLayout(terrain: simulation.world.terrain)
    currentLayout.setStreamedCollision(plans: detailPlans, garden: persistedWater)
    simulation.markRegionalPublicationPending(generation: 41, affectedKeys: affected)
    simulation.markRegionalPublicationPending(generation: 42, affectedKeys: affected)
    let stateBeforeObsoleteCommit = simulation.controller.state
    let positionBeforePendingMove = simulation.camera.position
    simulation.move(SIMD3<Float>(1, 0, 0))
    let blockedWhilePending = simulation.camera.position.x < boundaryX
      && simulation.camera.position.z == positionBeforePendingMove.z
    let obsoleteRejected = !simulation.publishRegionalState(
      generation: 41, layout: obsoleteLayout, garden: persistedFlowers, boulders: nil)
      && simulation.controller.state == stateBeforeObsoleteCommit
    guard simulation.publishRegionalState(
      generation: 42, layout: currentLayout, garden: persistedWater, boulders: nil)
    else { throw SanctuaryVegetationResidencyWorkloadError.currentPublicationRejected }
    simulation.move(SIMD3<Float>(1, 0, 0))
    let crossed = simulation.camera.position.x > boundaryX
    simulation.move(SIMD3<Float>(-1, 0, 0))
    let reversed = simulation.camera.position.x < boundaryX
    let collisionIDs = Set((simulation.observations["streamedCollisionChunkIDs"] ?? "")
      .split(separator: ",").map(String.init))
    let detailIDs = Set(detailPlans.map(\.key.id))

    return SanctuaryVegetationBoundaryEditWorkloadMetrics(
      westCoordinate: [west.x, west.y], eastCoordinate: [east.x, east.y],
      westToken: westMetrics.requestToken, eastToken: eastRequest.token,
      reverseToken: reverseRequest.token, eastGeneratedChunkCount: eastGenerated,
      reverseGeneratedChunkCount: reverseGenerated, stableOverlapChunkCount: overlap.count,
      stableOverlapSourceIDCount: stableOverlapIDs,
      mismatchedOverlapChunkCount: mismatches.count,
      stalePublicationCount: stale == .stale ? 1 : 0,
      stalePublicationPreservedSnapshot: beforeStale == afterStale && obsoleteRejected,
      persistedEditSourceID: source.id, persistedSourceRecordUnchanged: repeated == source,
      sameRevisionReceiptsDistinct: persistedWater.revision == persistedFlowers.revision
        && waterReceipt != flowerReceipt,
      sculptHeightDelta: editedHeight - baseHeight,
      excludedFromPresentationSource: !decision.isIncluded,
      excludedFromCollisionSource: !decision.isIncluded && source.providesCollision,
      blockedWhilePending: blockedWhilePending, crossedBoundary: crossed,
      reversedBoundary: reversed,
      detailAndCollisionChunkIDsEqual: collisionIDs == detailIDs
        && currentLayout.streamedCollisionKeys == Set(detailPlans.map(\.key)))
  }

  /// Fixture setup positions authored willing companions before the traversal. The performance
  /// harness times the complete repeatable workload, including fixture setup and residency.
  /// Movement under test uses only `SanctuaryWorld.move` and `advance`; residency uses the
  /// production default 9x9 source scheduler with its one-chunk publication budget. Renderer
  /// upload, live wind/cloud work, GPU time and process memory remain native-profile evidence.
  public static func fastTraversal() throws -> SanctuaryVegetationFastTraversalWorkloadMetrics {
    let walking = try SanctuaryWorld()
    walking.legacyInteractions = false
    let walkingStart = SIMD2<Float>(-10, 20)
    walking.camera = PlayerCamera(
      position: V3(
        walkingStart.x, walking.groundHeight(walkingStart.x, walkingStart.y) + 1.72,
        walkingStart.y),
      yaw: 0, pitch: -0.08)
    walking.syncExpeditionPlayer()
    walking.move(.zero)

    let riding = try SanctuaryWorld()
    riding.legacyInteractions = false
    guard try SanctuaryStreamingScenarios.configure(
      fixture: SanctuaryStreamingScenarios.alpineFixture, in: riding)
    else { throw SanctuaryVegetationResidencyWorkloadError.invalidResult }

    let flying = try SanctuaryWorld()
    flying.legacyInteractions = false
    guard let glider = flying.controller.state.population.actor(id: "canopy-glider-001") else {
      throw SanctuaryVegetationResidencyWorkloadError.missingSourceRecord
    }
    let flyingStart = glider.position + SIMD2<Float>(0, 4)
    flying.camera = PlayerCamera(
      position: V3(
        flyingStart.x, flying.groundHeight(flyingStart.x, flyingStart.y) + 1.72,
        flyingStart.y),
      yaw: 0, pitch: -0.08)
    flying.syncExpeditionPlayer()
    try SanctuaryFixtureRelationships.declareFamiliar("canopy-glider-001", in: flying)
    _ = try flying.control("fly")
    flying.move(.zero)

    // These are the production movements whose mode, distance, chunk crossing and collision
    // neighborhoods are asserted; the harness also includes the explicit setup above in timing.
    let walkingBefore = SIMD2(walking.camera.position.x, walking.camera.position.z)
    walking.move(V3(20, 0, 0))
    walking.advance(1 / 60, running: true)
    let walkingAfter = SIMD2(walking.camera.position.x, walking.camera.position.z)

    let ridingBefore = SIMD2(riding.camera.position.x, riding.camera.position.z)
    riding.move(V3(0, 0, 3))
    riding.advance(1 / 60, running: true)
    let ridingAfter = SIMD2(riding.camera.position.x, riding.camera.position.z)

    let flyingBefore = SIMD2(flying.camera.position.x, flying.camera.position.z)
    var residency = SanctuaryBiomeVegetationResidency()
    let warmup = try complete(
      name: "vegetation-residency-fast-traversal-warmup", residency: &residency,
      start: flyingBefore, end: flyingBefore)
    flying.move(V3(20, 0, 0))
    flying.advance(1 / 60, running: true)
    let flyingAfter = SIMD2(flying.camera.position.x, flying.camera.position.z)
    let entering = try complete(
      name: "vegetation-residency-fast-traversal-entering", residency: &residency,
      start: flyingBefore, end: flyingAfter)

    let worlds = [walking, riding, flying]
    let collisionMatches = worlds.allSatisfy { world in
      let point = SIMD2(world.camera.position.x, world.camera.position.z)
      let expected = Set(SanctuaryTerrainChunkKey.neighborhood(around: point).map(\.id))
      let actual = Set((world.observations["streamedCollisionChunkIDs"] ?? "")
        .split(separator: ",").map(String.init))
      return expected == actual
    }
    return SanctuaryVegetationFastTraversalWorkloadMetrics(
      travelModes: worlds.map { $0.controller.state.travel.mode.rawValue },
      walkingDistanceMetres: distance(walkingBefore, walkingAfter),
      ridingDistanceMetres: distance(ridingBefore, ridingAfter),
      flyingDistanceMetres: distance(flyingBefore, flyingAfter),
      crossedWalkingChunkBoundary: SanctuaryTerrainChunkKey(containing: walkingBefore)
        != SanctuaryTerrainChunkKey(containing: walkingAfter),
      crossedRidingChunkBoundary: SanctuaryTerrainChunkKey(containing: ridingBefore)
        != SanctuaryTerrainChunkKey(containing: ridingAfter),
      crossedFlyingChunkBoundary: SanctuaryTerrainChunkKey(containing: flyingBefore)
        != SanctuaryTerrainChunkKey(containing: flyingAfter),
      collisionNeighborhoodsMatched: collisionMatches,
      generatedChunkCount: warmup.generatedChunkCount + entering.generatedChunkCount,
      reusedChunkCount: warmup.reusedChunkCount + entering.reusedChunkCount,
      cacheHighWaterChunks: max(warmup.cacheHighWaterChunks, entering.cacheHighWaterChunks),
      maximumScheduledJobsPerTurn: 1,
      finalSnapshotComplete: entering.finalSnapshotComplete)
  }

  private static func complete(
    name: String, residency: inout SanctuaryBiomeVegetationResidency,
    start: SIMD2<Float>, end: SIMD2<Float>
  ) throws -> SanctuaryVegetationResidencyWorkloadMetrics {
    let startingRevision = residency.snapshot.revision
    let request = try residency.beginRequest(around: end)
    var generatedChunks = 0
    var candidates = 0
    var climates = 0
    var exactRecords = 0
    var collisionSources = 0
    var parentSummaries = 0
    var canopySummaries = 0
    var cacheHighWater = residency.cachedChunkCount
    var stale = 0
    var generatedIDs: [String] = []

    while let job = residency.nextJobs(for: request.token, budget: .oneChunk).first {
      let result = try residency.compile(job)
      generatedChunks += 1
      candidates += job.candidateCellCount
      climates += job.communityClimateSampleCount
      exactRecords += result.exact.count
      collisionSources += result.exact.filter(\.providesCollision).count
      parentSummaries += result.parents.count
      canopySummaries += 1
      generatedIDs += result.exact.map(\.id)
      switch residency.publish(result) {
      case .stale: stale += 1
      case .cached: break
      case .published: break
      }
      cacheHighWater = max(cacheHighWater, residency.cachedChunkCount)
    }
    let distinctIDs = Set(generatedIDs)
    let finalIDs = sourceIDList(in: residency.snapshot)
    let finalDistinctIDs = Set(finalIDs)
    return SanctuaryVegetationResidencyWorkloadMetrics(
      name: name, startCoordinate: [start.x, start.y], endCoordinate: [end.x, end.y],
      requestToken: request.token, desiredChunkCount: request.desiredChunks.count,
      generatedChunkCount: generatedChunks,
      reusedChunkCount: request.desiredChunks.count - generatedChunks,
      candidateCellsGenerated: candidates, communityClimateSamplesGenerated: climates,
      exactSourceRecordsGenerated: exactRecords,
      collisionSourceRecordsGenerated: collisionSources,
      distinctSourceIDCount: distinctIDs.count,
      duplicateSourceIDCount: generatedIDs.count - distinctIDs.count,
      parentSummariesGenerated: parentSummaries, canopySummariesGenerated: canopySummaries,
      cacheHighWaterChunks: cacheHighWater,
      publicationCount: Int(residency.snapshot.revision - startingRevision),
      stalePublicationCount: stale,
      finalDetailedRecordCount: residency.snapshot.detailedRecords.count,
      finalCommunitySummaryCount: residency.snapshot.communityParents.count,
      finalCanopySummaryCount: residency.snapshot.farCanopies.count,
      finalSourceIDCount: finalIDs.count, finalDistinctSourceIDCount: finalDistinctIDs.count,
      finalDuplicateSourceIDCount: finalIDs.count - finalDistinctIDs.count,
      finalSnapshotRevision: residency.snapshot.revision,
      finalSnapshotComplete: residency.snapshot.isComplete)
  }

  private static func sourceIDs(
    in snapshot: SanctuaryVegetationResidencySnapshot
  ) -> Set<String> {
    Set(sourceIDList(in: snapshot))
  }

  private static func sourceIDList(
    in snapshot: SanctuaryVegetationResidencySnapshot
  ) -> [String] {
    snapshot.detailedRecords.map(\.id)
      + snapshot.communityParents.flatMap(\.childIDs)
      + snapshot.farCanopies.flatMap(\.childIDs)
  }

  private static func sourceIDsByChunk(
    in snapshot: SanctuaryVegetationResidencySnapshot
  ) -> [SanctuaryTerrainChunkKey: Set<String>] {
    var result: [SanctuaryTerrainChunkKey: Set<String>] = [:]
    for chunk in snapshot.chunks { result[chunk.key] = [] }
    for record in snapshot.detailedRecords {
      result[SanctuaryTerrainChunkKey(containing: record.coordinate), default: []].insert(record.id)
    }
    for parent in snapshot.communityParents {
      result[SanctuaryTerrainChunkKey(containing: parent.centroid), default: []]
        .formUnion(parent.childIDs)
    }
    for canopy in snapshot.farCanopies {
      result[canopy.key, default: []].formUnion(canopy.childIDs)
    }
    return result
  }

  private static func chunkBounds(_ key: SanctuaryTerrainChunkKey) -> SanctuaryMapBounds {
    SanctuaryMapBounds(
      minimum: simd_max(key.minimum, SanctuaryGeography.bounds.minimum),
      maximum: simd_min(key.maximum, SanctuaryGeography.bounds.maximum))
  }

  private static func checksum(
    _ value: SanctuaryVegetationResidencyWorkloadMetrics
  ) -> Double {
    Double(value.candidateCellsGenerated + value.communityClimateSamplesGenerated
      + value.exactSourceRecordsGenerated + value.collisionSourceRecordsGenerated
      + value.parentSummariesGenerated + value.canopySummariesGenerated
      + value.finalDetailedRecordCount + value.finalCommunitySummaryCount
      + value.finalCanopySummaryCount + value.finalSourceIDCount)
  }
}

public enum SanctuaryVegetationResidencyWorkloadError: Error, Equatable, Sendable {
  case missingJob
  case missingSourceRecord
  case cancellationRejected
  case currentPublicationRejected
  case invalidResult
}
