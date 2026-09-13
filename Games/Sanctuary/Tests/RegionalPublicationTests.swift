import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class RegionalPublicationTests: XCTestCase {
  private func layout(
    for world: SanctuaryWorld, around point: SIMD2<Float>,
    garden: HabitatGarden? = nil, boulders: BoulderArrangements? = nil
  ) -> SanctuaryLayout {
    var result = SanctuaryLayout(terrain: world.world.terrain)
    let geography = SanctuaryGeography()
    let plans = SanctuaryTerrainChunkKey.neighborhood(around: point).map {
      geography.plan(for: $0)
    }
    result.setStreamedCollision(plans: plans, garden: garden, boulders: boulders)
    return result
  }

  private func putPlayer(_ world: SanctuaryWorld, at point: SIMD2<Float>) {
    world.camera.position = SIMD3(
      point.x, world.groundHeight(point.x, point.y) + 1.72, point.y)
    world.camera.yaw = 0
    world.camera.pitch = 0
    world.syncExpeditionPlayer()
  }

  func testPendingKeysUnionWithinGenerationAndNewerIntentReplacesTheGate() throws {
    let world = try SanctuaryWorld()
    let point = SIMD2<Float>(256, 256)
    let initial = layout(for: world, around: point)
    world.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: nil, boulders: nil)
    let first = SanctuaryTerrainChunkKey(containing: point)
    let second = SanctuaryTerrainChunkKey(x: first.x + 1, z: first.z)

    world.markRegionalPublicationPending(generation: 4, affectedKeys: [first])
    world.markRegionalPublicationPending(generation: 4, affectedKeys: [second])
    world.markRegionalPublicationPending(generation: 4, affectedKeys: [])
    XCTAssertEqual(world.observations["regionalPendingChunkCount"], "2")
    XCTAssertFalse(world.regionalMovementIsPublished(at: first.center))
    XCTAssertFalse(world.regionalMovementIsPublished(at: second.center))

    world.markRegionalPublicationPending(generation: 5, affectedKeys: [first])
    XCTAssertEqual(world.observations["regionalPendingChunkCount"], "1")
    XCTAssertFalse(world.regionalMovementIsPublished(at: first.center))
    XCTAssertTrue(world.regionalMovementIsPublished(at: second.center))
    world.markRegionalPublicationPending(generation: 4, affectedKeys: [second])
    XCTAssertEqual(world.observations["regionalPendingChunkCount"], "1")
  }

  func testStalePublicationCannotReplaceCurrentLayoutSupportOrSavedProgress() throws {
    let world = try SanctuaryWorld()
    let point = SIMD2<Float>(280, 280)
    putPlayer(world, at: point)
    let initial = layout(for: world, around: point)
    world.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: nil, boulders: nil)
    let saved = try world.checkpoint()

    _ = try world.controller.applyNature(
      .sculpt(.raise, at: .init(x: point.x, z: point.y), radius: 6,
        amount: 2, targetHeight: nil), expectedRevision: 0)
    let editedGarden = try XCTUnwrap(world.controller.state.garden)
    let editedLayout = layout(for: world, around: point, garden: editedGarden)
    let key = SanctuaryTerrainChunkKey(containing: point)
    world.markRegionalPublicationPending(generation: 11, affectedKeys: [key])
    world.markRegionalPublicationPending(generation: 12, affectedKeys: [key])
    let editedState = world.controller.state

    XCTAssertFalse(world.publishRegionalState(
      generation: 11, layout: editedLayout, garden: editedGarden, boulders: nil))
    XCTAssertEqual(world.controller.state, editedState)
    XCTAssertNil(world.presentationGarden)
    XCTAssertFalse(world.regionalMovementIsPublished(at: point))

    XCTAssertTrue(world.publishRegionalState(
      generation: 12, layout: editedLayout, garden: editedGarden, boulders: nil))
    XCTAssertEqual(world.controller.state, editedState)
    XCTAssertEqual(world.presentationGarden, editedGarden)
    XCTAssertTrue(world.regionalMovementIsPublished(at: point))

    try world.restore(saved)
    let restoredState = world.controller.state
    world.markRegionalPublicationPending(generation: 13, affectedKeys: [key])
    XCTAssertFalse(world.publishRegionalState(
      generation: 12, layout: editedLayout, garden: editedGarden, boulders: nil))
    XCTAssertEqual(world.controller.state, restoredState)
    XCTAssertEqual(world.presentationGarden, editedGarden)
    let restoredLayout = layout(for: world, around: point)
    XCTAssertTrue(world.publishRegionalState(
      generation: 13, layout: restoredLayout, garden: nil, boulders: nil))
    XCTAssertEqual(world.controller.state, restoredState)
    XCTAssertNil(world.presentationGarden)
  }

  func testFarSavedPlayerBootstrapsInsidePublishedCollisionCoverage() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let writer = try SanctuaryWorld(root: root)
    let far = SanctuaryGeography().landmark(for: .rainforest).coordinate + SIMD2<Float>(96, 64)
    putPlayer(writer, at: far)
    try writer.controller.save()

    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(SIMD2(reopened.camera.position.x, reopened.camera.position.z), far)
    let initial = layout(
      for: reopened, around: far, garden: reopened.controller.state.garden,
      boulders: reopened.controller.state.boulders)
    let savedState = reopened.controller.state
    reopened.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: reopened.controller.state.garden,
      boulders: reopened.controller.state.boulders)

    XCTAssertEqual(reopened.controller.state, savedState)
    XCTAssertTrue(initial.streamedCollisionKeys.contains(
      SanctuaryTerrainChunkKey(containing: far)))
    XCTAssertTrue(reopened.regionalMovementIsPublished(at: far))
    XCTAssertEqual(reopened.observations["regionalCollisionPublicationPolicy"], "hostCommitted")
  }

  func testMovementStopsAtUnpublishedCoverageAndContinuesAfterCurrentCommit() throws {
    let world = try SanctuaryWorld()
    world.legacyInteractions = false
    let start = SIMD2<Float>(1_023.45, 300)
    putPlayer(world, at: start)
    let initial = layout(for: world, around: SIMD2(256, 300))
    world.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: nil, boulders: nil)

    world.move(SIMD3<Float>(1, 0, 0))
    XCTAssertLessThan(world.camera.position.x, 1_024)
    XCTAssertEqual(world.camera.position.z, start.y, accuracy: 0.001)

    let incoming = layout(for: world, around: SIMD2(1_280, 300))
    world.markRegionalPublicationPending(generation: 7, affectedKeys: [])
    XCTAssertTrue(world.publishRegionalState(
      generation: 7, layout: incoming, garden: nil, boulders: nil))
    world.move(SIMD3<Float>(1, 0, 0))
    XCTAssertGreaterThan(world.camera.position.x, 1_024)
  }

  func testLiveGardenDoesNotChangePublishedSupportBeforeMatchingCommit() throws {
    let world = try SanctuaryWorld()
    world.legacyInteractions = false
    let point = SIMD2<Float>(320, 320)
    putPlayer(world, at: point)
    let initial = layout(for: world, around: point)
    world.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: nil, boulders: nil)
    let oldGround = world.groundHeight(point.x, point.y)

    _ = try world.controller.applyNature(
      .sculpt(.raise, at: .init(x: point.x, z: point.y), radius: 6,
        amount: 3, targetHeight: nil), expectedRevision: 0)
    let garden = try XCTUnwrap(world.controller.state.garden)
    let liveCenterGround = garden.surfaceHeight(
      baseHeight: world.world.terrain.height(point.x, point.y),
      at: .init(x: point.x, z: point.y))
    let liveGround = world.standingTerrainHeight(at: point, state: world.controller.state)
    XCTAssertGreaterThan(liveGround, oldGround + 2)
    XCTAssertEqual(world.groundHeight(point.x, point.y), oldGround, accuracy: 0.001)

    let key = SanctuaryTerrainChunkKey(containing: point)
    world.markRegionalPublicationPending(generation: 21, affectedKeys: [key])
    let before = world.camera.position
    world.move(SIMD3<Float>(0.5, 0, 0))
    XCTAssertEqual(world.camera.position, before)

    let edited = layout(for: world, around: point, garden: garden)
    XCTAssertTrue(world.publishRegionalState(
      generation: 21, layout: edited, garden: garden, boulders: nil))
    XCTAssertEqual(world.groundHeight(point.x, point.y), liveCenterGround, accuracy: 0.0001)
    XCTAssertEqual(world.publishedStandingHeight(at: point), liveGround, accuracy: 0.0001)
    XCTAssertEqual(world.camera.position.y, liveGround + 1.72, accuracy: 0.001)
  }

  func testUnchangedSupportPublicationPreservesExactRestoredCenterPoseAndCheckpoint() throws {
    let world = try SanctuaryWorld()
    world.legacyInteractions = false
    let point = SIMD2<Float>(0, 0)
    let centerGround = world.world.terrain.height(point.x, point.y)
    world.camera.position = SIMD3(point.x, centerGround + 1.72, point.y)
    world.camera.yaw = 0.37
    world.camera.pitch = -0.11
    world.syncExpeditionPlayer()
    let exactPose = world.camera
    let checkpoint = try world.checkpoint()
    let initial = layout(for: world, around: point)
    world.enableHostCommittedRegionalCollision(
      initialLayout: initial, garden: nil, boulders: nil)

    world.markRegionalPublicationPending(generation: 61, affectedKeys: [])
    XCTAssertTrue(world.publishRegionalState(
      generation: 61, layout: initial, garden: nil, boulders: nil))
    XCTAssertEqual(world.camera, exactPose)
    XCTAssertEqual(try world.checkpoint(), checkpoint)

    var flowers = HabitatGarden()
    _ = try flowers.apply(
      .plant(.flowers, at: .init(x: 0, z: 18), radius: 3), expectedRevision: 0)
    let decorated = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(flowers))
    XCTAssertTrue(SanctuaryGardenSurfaceSource(garden: decorated).coveredChunkKeys.isEmpty)
    world.markRegionalPublicationPending(generation: 62, affectedKeys: [])
    XCTAssertTrue(world.publishRegionalState(
      generation: 62, layout: initial, garden: decorated, boulders: nil))
    XCTAssertEqual(world.camera, exactPose)
    XCTAssertEqual(try world.checkpoint(), checkpoint)
    XCTAssertEqual(world.presentationGarden, decorated)
  }

  func testSameRevisionBoundaryEditRejectsObsoleteCommitThenCrossesAndReverses() throws {
    let world = try SanctuaryWorld()
    world.legacyInteractions = false
    let boundaryX: Float = 8_192
    let z: Float = -3_000
    let west = SIMD2<Float>(boundaryX - 0.45, z)
    let east = SIMD2<Float>(boundaryX + 0.45, z)
    putPlayer(world, at: west)
    world.enableHostCommittedRegionalCollision(
      initialLayout: layout(for: world, around: west), garden: nil, boulders: nil)

    let residencyConfiguration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 1, middleChunkRadius: 1, farChunkRadius: 1)
    var residency = SanctuaryBiomeVegetationResidency(configuration: residencyConfiguration)
    func complete(at point: SIMD2<Float>) throws -> SanctuaryVegetationResidencySnapshot {
      let request = try residency.beginRequest(around: point)
      while let job = residency.nextJobs(for: request.token, budget: .oneChunk).first {
        let result = try residency.compile(job)
        _ = residency.publish(result)
      }
      XCTAssertTrue(residency.snapshot.isComplete)
      return residency.snapshot
    }
    func sourceIDsByChunk(
      _ snapshot: SanctuaryVegetationResidencySnapshot
    ) -> [SanctuaryTerrainChunkKey: Set<String>] {
      Dictionary(grouping: snapshot.detailedRecords) {
        SanctuaryTerrainChunkKey(containing: $0.coordinate)
      }.mapValues { Set($0.map(\.id)) }
    }
    let westSnapshot = try complete(at: west)
    let eastSnapshot = try complete(at: east)
    let reverseSnapshot = try complete(at: west)
    let westIDs = sourceIDsByChunk(westSnapshot), eastIDs = sourceIDsByChunk(eastSnapshot)
    for key in Set(westIDs.keys).intersection(eastIDs.keys) {
      XCTAssertEqual(westIDs[key], eastIDs[key], key.id)
    }
    XCTAssertEqual(sourceIDsByChunk(reverseSnapshot), westIDs)
    let center = SanctuaryTerrainChunkKey(containing: east)
    let source = try XCTUnwrap(eastSnapshot.detailedRecords.first {
      $0.providesCollision && SanctuaryTerrainChunkKey(containing: $0.coordinate) == center
    })

    let sculpt = HabitatGarden.Command.sculpt(
      .raise, at: .init(x: boundaryX + 0.8, z: z), radius: 6,
      amount: 2, targetHeight: nil)
    let planting = HabitatGarden.Location(x: source.coordinate.x, z: source.coordinate.y)
    var waterBranch = HabitatGarden(), flowerBranch = HabitatGarden()
    _ = try waterBranch.apply(sculpt, expectedRevision: 0)
    _ = try waterBranch.apply(
      .plant(.shallowWater, at: planting, radius: 2),
      expectedRevision: waterBranch.revision)
    _ = try flowerBranch.apply(sculpt, expectedRevision: 0)
    _ = try flowerBranch.apply(
      .plant(.flowers, at: planting, radius: 2),
      expectedRevision: flowerBranch.revision)
    let persistedWater = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(waterBranch))
    let persistedFlowers = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(flowerBranch))
    let waterReceipt = SanctuaryRegionalSourceReceipt(
      center: center, garden: persistedWater, boulders: nil, construction: nil)
    let flowerReceipt = SanctuaryRegionalSourceReceipt(
      center: center, garden: persistedFlowers, boulders: nil, construction: nil)
    XCTAssertEqual(persistedWater.revision, persistedFlowers.revision)
    XCTAssertNotEqual(waterReceipt, flowerReceipt)
    XCTAssertFalse(SanctuaryVegetationLocalEditMask(garden: persistedWater)
      .decision(for: source).isIncluded)
    let sourceBounds = SanctuaryMapBounds(
      minimum: simd_max(center.minimum, SanctuaryGeography.bounds.minimum),
      maximum: simd_min(center.maximum, SanctuaryGeography.bounds.maximum))
    XCTAssertEqual(try residency.population.query(in: sourceBounds).exact.first {
      $0.id == source.id
    }, source)

    let oldLayout = layout(for: world, around: east, garden: persistedFlowers)
    let currentLayout = layout(for: world, around: east, garden: persistedWater)
    let affected = SanctuaryGardenSurfaceSource(garden: persistedWater).affectedChunkKeys(
      comparedTo: SanctuaryGardenSurfaceSource(garden: persistedFlowers))
    XCTAssertTrue(affected.contains(center))
    world.markRegionalPublicationPending(generation: 71, affectedKeys: affected)
    world.markRegionalPublicationPending(generation: 72, affectedKeys: affected)

    let beforePendingMove = world.camera.position
    world.move(SIMD3<Float>(1, 0, 0))
    XCTAssertLessThan(world.camera.position.x, boundaryX)
    XCTAssertEqual(world.camera.position.z, beforePendingMove.z, accuracy: 0.001)
    let savedProgress = world.controller.state
    XCTAssertFalse(world.publishRegionalState(
      generation: 71, layout: oldLayout, garden: persistedFlowers, boulders: nil))
    XCTAssertEqual(world.controller.state, savedProgress)
    XCTAssertNil(world.presentationGarden)

    XCTAssertTrue(world.publishRegionalState(
      generation: 72, layout: currentLayout, garden: persistedWater, boulders: nil))
    XCTAssertEqual(world.controller.state, savedProgress)
    XCTAssertEqual(world.presentationGarden, persistedWater)
    XCTAssertEqual(world.world.streamedCollisionKeys, currentLayout.streamedCollisionKeys)
    let observedCollisionIDs = Set((world.observations["streamedCollisionChunkIDs"] ?? "")
      .split(separator: ",").map(String.init))
    XCTAssertEqual(observedCollisionIDs, Set(currentLayout.streamedCollisionKeys.map(\.id)))

    world.move(SIMD3<Float>(1, 0, 0))
    XCTAssertGreaterThan(world.camera.position.x, boundaryX)
    world.move(SIMD3<Float>(-1, 0, 0))
    XCTAssertLessThan(world.camera.position.x, boundaryX)
    let afterMovement = world.controller.state
    XCTAssertNotNil(afterMovement.groundContacts)
    var expectedAfterMovement = savedProgress
    // Accepted production movement records real swept ground contacts. The exact state checks
    // above already isolate both publication calls; normalize only that intended movement output.
    expectedAfterMovement.groundContacts = afterMovement.groundContacts
    XCTAssertEqual(afterMovement, expectedAfterMovement)
  }

  func testImmediateAndHostCommittedAdvanceKeepPopulationAndContactsExact() throws {
    let immediate = try SanctuaryWorld(seed: 17)
    let committed = try SanctuaryWorld(seed: 17)
    immediate.legacyInteractions = false
    committed.legacyInteractions = false
    // Match the established journey's active Golden Meadow neighborhood. Nearby
    // actors use detailed collision here, while remote authored-home actors take
    // bounded sparse updates that are independent of presentation residency.
    let player = SIMD2<Float>(2_472, -624)
    putPlayer(immediate, at: player)
    putPlayer(committed, at: player)

    // Exercise the ordinary CPU publication path and give the host policy that
    // exact complete layout. Stable vegetation IDs prove this includes detailed
    // source collision rather than an empty host window.
    immediate.move(.zero)
    let publishedLayout = immediate.world
    XCTAssertTrue(publishedLayout.solids.contains { $0.name.hasPrefix("vegetation-v1:") })
    XCTAssertEqual(publishedLayout.immediateVegetationCachedChunks, 9)
    immediate.move(.zero)
    XCTAssertEqual(immediate.world.immediateVegetationGeneratedChunksLastUpdate, 0)
    XCTAssertEqual(immediate.world.immediateVegetationResolvedChunksLastUpdate, 0)
    committed.camera = immediate.camera
    immediate.syncExpeditionPlayer()
    committed.syncExpeditionPlayer()
    committed.enableHostCommittedRegionalCollision(
      initialLayout: publishedLayout, garden: nil, boulders: nil)
    XCTAssertEqual(committed.controller.state.population, immediate.controller.state.population)
    XCTAssertEqual(committed.controller.state.groundContacts, immediate.controller.state.groundContacts)

    let initialPopulation = immediate.controller.state.population
    for _ in 0..<360 {
      immediate.advance(1 / 60, running: false)
      committed.advance(1 / 60, running: false)
    }

    XCTAssertNotEqual(immediate.controller.state.population, initialPopulation,
      "The comparison must include actual detailed and sparse wildlife updates")
    XCTAssertNotNil(immediate.controller.state.groundContacts,
      "Nearby grounded wildlife must exercise the production contact recorder")
    XCTAssertEqual(committed.controller.state.population, immediate.controller.state.population)
    XCTAssertEqual(committed.controller.state.groundContacts, immediate.controller.state.groundContacts)
    XCTAssertEqual(committed.camera, immediate.camera)
  }
}
