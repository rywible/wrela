import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class BiomeVegetationPopulationTests: XCTestCase {
  private func bounds(
    _ minimum: SIMD2<Float>, _ maximum: SIMD2<Float>
  ) -> SanctuaryMapBounds {
    SanctuaryMapBounds(minimum: minimum, maximum: maximum)
  }

  private func recordsByID(
    _ records: [SanctuaryVegetationRecord]
  ) -> [String: SanctuaryVegetationRecord] {
    Dictionary(uniqueKeysWithValues: records.map { ($0.id, $0) })
  }

  private func parentsByID(
    _ parents: [SanctuaryVegetationParentSummary]
  ) -> [String: SanctuaryVegetationParentSummary] {
    Dictionary(uniqueKeysWithValues: parents.map { ($0.id, $0) })
  }

  func testHabitatScalarsAreFiniteBoundedAndContinuous() throws {
    let field = SanctuaryBiomeHabitatField()
    let p = SanctuaryGeography().landmark(for: .rainforest).coordinate + SIMD2(210, 170)
    let a = try field.sample(at: p)
    let b = try field.sample(at: p + SIMD2(0.01, -0.01))
    for species in SanctuaryVegetationSpecies.allCases {
      let x = a.suitability(for: species)
      let y = b.suitability(for: species)
      XCTAssertTrue(x.isFinite && (0...1).contains(x), species.rawValue)
      XCTAssertLessThan(abs(x - y), 0.02, species.rawValue)
    }
    for value in [a.riparianPotential, a.terrainSuitability, a.routeSuitability] {
      XCTAssertTrue(value.isFinite && (0...1).contains(value))
    }
    for value in [a.habitatMoisture, a.habitatTemperature] {
      XCTAssertTrue(value.isFinite && (0...1).contains(value))
    }
    XCTAssertTrue(a.orographicExposure.isFinite && (-1...1).contains(a.orographicExposure))
    XCTAssertLessThan(abs(a.riparianPotential - b.riparianPotential), 0.02)
    XCTAssertLessThan(abs(a.terrainSuitability - b.terrainSuitability), 0.02)
    XCTAssertLessThan(abs(a.routeSuitability - b.routeSuitability), 0.02)

    let outside = try field.sample(at: SIMD2(16_001, 0))
    XCTAssertFalse(outside.isInsideWorld)
    XCTAssertFalse(outside.permitsPlacement)
    XCTAssertThrowsError(try field.sample(at: SIMD2(.nan, 0)))
  }

  func testSplitAndCombinedQueriesHaveIdenticalExactRecordsAndParents() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let whole = bounds(SIMD2(512, -512), SIMD2(1_536, 512))
    let left = bounds(SIMD2(512, -512), SIMD2(1_024, 512))
    let right = bounds(SIMD2(1_024, -512), SIMD2(1_536, 512))
    let combined = try population.query(in: whole)
    let leftQuery = try population.query(in: left)
    let rightQuery = try population.query(in: right)
    let splitRecords = leftQuery.exact + rightQuery.exact
    let splitParents = leftQuery.parents + rightQuery.parents
    XCTAssertEqual(recordsByID(splitRecords), recordsByID(combined.exact))
    XCTAssertEqual(parentsByID(splitParents), parentsByID(combined.parents))
    XCTAssertEqual(Set(combined.exact.map(\.id)).count, combined.exact.count)
    XCTAssertEqual(Set(combined.parents.map(\.id)).count, combined.parents.count)
  }

  func testOverlappingQueriesReuseExactIDsAndPositions() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let a = try population.query(in: bounds(SIMD2(7_936, -3_456), SIMD2(8_704, -2_688)))
    let b = try population.query(in: bounds(SIMD2(8_192, -3_200), SIMD2(8_960, -2_432)))
    let overlap = bounds(SIMD2(8_192, -3_200), SIMD2(8_704, -2_688))
    let direct = try population.query(in: overlap)
    let aOverlap = a.exact.filter {
      $0.coordinate.x >= overlap.minimum.x && $0.coordinate.x < overlap.maximum.x
        && $0.coordinate.y >= overlap.minimum.y && $0.coordinate.y < overlap.maximum.y
    }
    let bOverlap = b.exact.filter {
      $0.coordinate.x >= overlap.minimum.x && $0.coordinate.x < overlap.maximum.x
        && $0.coordinate.y >= overlap.minimum.y && $0.coordinate.y < overlap.maximum.y
    }
    XCTAssertEqual(recordsByID(aOverlap), recordsByID(direct.exact))
    XCTAssertEqual(recordsByID(bOverlap), recordsByID(direct.exact))
    let commonParentIDs = Set(a.parents.map(\.id)).intersection(b.parents.map(\.id))
    let aParents = parentsByID(a.parents.filter { commonParentIDs.contains($0.id) })
    let bParents = parentsByID(b.parents.filter { commonParentIDs.contains($0.id) })
    XCTAssertEqual(aParents, bParents)
  }

  func testRecordsAreDeterministicFiniteAndModestlyBounded() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let region = bounds(SIMD2(7_680, -3_584), SIMD2(9_216, -2_048))
    let first = try population.query(in: region)
    let second = try population.query(in: region)
    XCTAssertEqual(first, second)
    XCTAssertFalse(first.exact.isEmpty)
    XCTAssertLessThanOrEqual(first.exact.count, 9 * 256)
    for record in first.exact {
      XCTAssertTrue(SanctuaryGeography.bounds.contains(record.coordinate))
      XCTAssertTrue(record.coordinate.x.isFinite && record.coordinate.y.isFinite)
      XCTAssertTrue(record.scale.isFinite && record.scale > 0)
      XCTAssertTrue(record.yaw.isFinite && (0..<(2 * Float.pi)).contains(record.yaw))
      XCTAssertTrue(record.id.hasPrefix("vegetation-v1:"))
    }
  }

  func testRealWaterAndCabinSourcesExcludePlacements() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let terrain = Terrain()
    let cabin = try population.query(in: bounds(SIMD2(-256, -256), SIMD2(256, 256)))
    for record in cabin.exact {
      XCTAssertFalse(
        abs(record.coordinate.x) <= SanctuaryTerrainTessellation.cabinExtent
          && abs(record.coordinate.y) <= SanctuaryTerrainTessellation.cabinExtent)
    }

    let creek = try population.query(in: bounds(SIMD2(1_024, 1_536), SIMD2(2_560, 3_072)))
    for record in cabin.exact + creek.exact {
      XCTAssertNil(terrain.water(at: record.coordinate), record.id)
    }
  }

  func testParentSummariesAreCompleteStableAndNoncolliding() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let region = bounds(SIMD2(7_680, -3_584), SIMD2(8_192, -3_072))
    let query = try population.query(in: region)
    XCTAssertFalse(query.parents.isEmpty)
    for parent in query.parents.prefix(4) {
      let isolated = try population.query(in: parent.bounds)
      let repeated = try XCTUnwrap(isolated.parents.first { $0.id == parent.id })
      XCTAssertEqual(repeated, parent)
      XCTAssertEqual(repeated.childCount, isolated.exact.count)
      XCTAssertEqual(repeated.speciesCounts.values.reduce(0, +), repeated.childCount)
      XCTAssertEqual(repeated.childIDs, isolated.exact.map(\.id).sorted())
      XCTAssertFalse(repeated.providesCollision)
      XCTAssertTrue(repeated.centroid.x.isFinite && repeated.centroid.y.isFinite)
      XCTAssertTrue(repeated.maximumScale.isFinite && repeated.maximumScale > 0)
    }
  }

  func testInvalidAndOversizedQueriesAreRejectedWithoutEnumeration() {
    let population = SanctuaryBiomeVegetationPopulation()
    XCTAssertThrowsError(
      try population.query(in: bounds(SIMD2(.nan, 0), SIMD2(1, 1)))
    ) { XCTAssertEqual($0 as? SanctuaryVegetationPopulationError, .invalidBounds) }
    XCTAssertThrowsError(
      try population.query(in: bounds(SIMD2(10, 10), SIMD2(-10, -10)))
    ) { XCTAssertEqual($0 as? SanctuaryVegetationPopulationError, .invalidBounds) }
    XCTAssertThrowsError(try population.query(in: SanctuaryGeography.bounds)) {
      XCTAssertEqual($0 as? SanctuaryVegetationPopulationError, .queryTooLarge)
    }
  }

  func testResidencyPlanIsBoundedOrderedAndRequiresNoSynchronousGeneration() throws {
    var residency = SanctuaryBiomeVegetationResidency()
    let request = try residency.beginRequest(around: SIMD2(8_300, -3_000))
    XCTAssertEqual(request.desiredChunks.count, 81)
    XCTAssertEqual(request.desiredChunks.filter { $0.tier == .detailed }.count, 9)
    XCTAssertEqual(request.desiredChunks.filter { $0.tier == .community }.count, 16)
    XCTAssertEqual(request.desiredChunks.filter { $0.tier == .canopy }.count, 56)
    XCTAssertEqual(request.candidateCellCount, 81 * 256)
    XCTAssertEqual(request.communityClimateSampleCount, 81 * 16)
    XCTAssertEqual(residency.cachedChunkCount, 0)
    XCTAssertEqual(residency.snapshot, .empty)

    let budget = try SanctuaryVegetationWorkBudget(
      maximumChunks: 2, maximumCandidateCells: 512)
    let jobs = residency.nextJobs(for: request.token, budget: budget)
    XCTAssertEqual(jobs.count, 2)
    XCTAssertTrue(jobs.allSatisfy { $0.requestedTier == .detailed })
    XCTAssertEqual(residency.scheduledChunkCount, 2)
    XCTAssertEqual(residency.cachedChunkCount, 0)
  }

  func testLiveRainHasNoInputToStablePopulationGeneration() throws {
    let habitat = SanctuaryBiomeHabitatField()
    let coordinate = SIMD2<Float>(8_300, -3_000)
    let climateBefore = try habitat.communityClimate(at: coordinate)
    let support = habitat.terrain.height(coordinate.x, coordinate.y)
    _ = try habitat.environment.surfaceSample(
      at: coordinate, supportingHeightMetres: support, gradeRisePerRun: 0,
      weather: SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 1, rain: 1, wind: 0.8))
    let climateAfter = try habitat.communityClimate(at: coordinate)
    XCTAssertEqual(climateBefore, climateAfter)

    let population = SanctuaryBiomeVegetationPopulation(habitat: habitat)
    let region = bounds(SIMD2(8_192, -3_072), SIMD2(8_704, -2_560))
    XCTAssertEqual(try population.query(in: region), try population.query(in: region))
  }

  func testCompiledCanopyAndParentsRetainEveryExactIdentity() throws {
    let configuration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 0, middleChunkRadius: 0, farChunkRadius: 0)
    var residency = SanctuaryBiomeVegetationResidency(configuration: configuration)
    let request = try residency.beginRequest(around: SIMD2(8_300, -3_000))
    let job = try XCTUnwrap(residency.nextJobs(
      for: request.token, budget: .oneChunk).first)
    let compiled = try residency.compile(job)
    let exactIDs = compiled.exact.map(\.id).sorted()
    XCTAssertEqual(compiled.canopy.childIDs, exactIDs)
    XCTAssertEqual(compiled.parents.flatMap(\.childIDs).sorted(), exactIDs)
    XCTAssertEqual(compiled.canopy.childCount, exactIDs.count)
    XCTAssertFalse(compiled.canopy.providesCollision)
    XCTAssertTrue(compiled.canopy.canopyCoverageProxy.isFinite)
    XCTAssertTrue((0...1).contains(compiled.canopy.canopyCoverageProxy))
  }

  func testStaleGenerationCannotPublishAndCurrentSingleChunkPublishesAtomically() throws {
    let configuration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 0, middleChunkRadius: 0, farChunkRadius: 0)
    var residency = SanctuaryBiomeVegetationResidency(configuration: configuration)
    let old = try residency.beginRequest(around: SIMD2(8_300, -3_000))
    let oldJob = try XCTUnwrap(residency.nextJobs(for: old.token, budget: .oneChunk).first)
    let oldResult = try residency.compile(oldJob)

    let current = try residency.beginRequest(around: SIMD2(9_300, -3_000))
    XCTAssertEqual(residency.publish(oldResult), .stale)
    XCTAssertEqual(residency.cachedChunkCount, 0)
    let currentJob = try XCTUnwrap(
      residency.nextJobs(for: current.token, budget: .oneChunk).first)
    let result = try residency.compile(currentJob)
    guard case let .published(snapshot) = residency.publish(result) else {
      return XCTFail("Current complete tier did not publish")
    }
    XCTAssertTrue(snapshot.isComplete)
    XCTAssertEqual(snapshot.requestToken, current.token)
    XCTAssertEqual(snapshot.chunks, [
      SanctuaryVegetationResidentChunk(key: current.centerKey, tier: .detailed)
    ])
    XCTAssertLessThanOrEqual(residency.cachedChunkCount, configuration.maximumCachedChunks)
  }

  func testExplicitCancellationPreservesPublishedGenerationCacheAndIDs() throws {
    let configuration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 0, middleChunkRadius: 0, farChunkRadius: 0)
    var residency = SanctuaryBiomeVegetationResidency(configuration: configuration)
    let initial = try residency.beginRequest(around: SIMD2(8_300, -3_000))
    let initialJob = try XCTUnwrap(
      residency.nextJobs(for: initial.token, budget: .oneChunk).first)
    let initialResult = try residency.compile(initialJob)
    guard case .published = residency.publish(initialResult) else {
      return XCTFail("Initial generation did not publish")
    }
    let before = residency.snapshot
    let beforeCache = residency.cachedChunkCount

    let cancelled = try residency.beginRequest(around: SIMD2(9_300, -3_000))
    let cancelledJob = try XCTUnwrap(
      residency.nextJobs(for: cancelled.token, budget: .oneChunk).first)
    let cancelledResult = try residency.compile(cancelledJob)
    XCTAssertTrue(residency.cancel(requestToken: cancelled.token))
    XCTAssertFalse(residency.cancel(requestToken: cancelled.token))
    XCTAssertEqual(residency.publish(cancelledResult), .stale)
    XCTAssertEqual(residency.snapshot, before)
    XCTAssertEqual(residency.cachedChunkCount, beforeCache)
  }

  func testPersistedLocalEditsExcludeWithoutChangingSourceIdentity() throws {
    let population = SanctuaryBiomeVegetationPopulation()
    let region = bounds(SIMD2(7_680, -3_584), SIMD2(9_216, -2_048))
    let original = try XCTUnwrap(
      population.query(in: region).exact.first(where: \.providesCollision))
    XCTAssertTrue(original.providesCollision)
    let dry = SanctuaryVegetationLocalEditMask().decision(for: original)
    XCTAssertTrue(dry.isIncluded)
    XCTAssertEqual(dry.sourceID, original.id)

    var garden = HabitatGarden()
    _ = try garden.apply(
      .plant(.shallowWater,
        at: .init(x: original.coordinate.x, z: original.coordinate.y), radius: 0.5),
      expectedRevision: garden.revision)
    let restoredGarden = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(garden))
    let water = SanctuaryVegetationLocalEditMask(garden: restoredGarden).decision(for: original)
    XCTAssertEqual(water.sourceID, original.id)
    XCTAssertEqual(water.exclusion, .authoredShallowWater)

    var construction = PersonalConstruction()
    _ = try construction.apply(
      .place(.path,
        at: .init(x: original.coordinate.x, y: 0, z: original.coordinate.y),
        yawRadians: 0, scale: 1), expectedRevision: construction.revision)
    let restoredConstruction = try JSONDecoder().decode(
      PersonalConstruction.self, from: JSONEncoder().encode(construction))
    let built = SanctuaryVegetationLocalEditMask(
      construction: restoredConstruction).decision(for: original)
    XCTAssertEqual(built.sourceID, original.id)
    XCTAssertEqual(built.exclusion, .construction)

    var sculpt = HabitatGarden()
    _ = try sculpt.apply(
      .sculpt(.raise,
        at: .init(x: original.coordinate.x, z: original.coordinate.y), radius: 1,
        amount: 1, targetHeight: nil), expectedRevision: sculpt.revision)
    XCTAssertTrue(SanctuaryVegetationLocalEditMask(garden: sculpt).decision(for: original).isIncluded)
    XCTAssertEqual(try population.query(in: region).exact.first { $0.id == original.id }, original)
  }

  func testNearTierPublishesWhileOldSceneRemainsAndMiddleCompletesLater() throws {
    let configuration = try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 0, middleChunkRadius: 1, farChunkRadius: 1)
    var residency = SanctuaryBiomeVegetationResidency(configuration: configuration)
    let request = try residency.beginRequest(around: SIMD2(8_300, -3_000))
    let allBudget = try SanctuaryVegetationWorkBudget(
      maximumChunks: 9, maximumCandidateCells: 9 * 256)
    let jobs = residency.nextJobs(for: request.token, budget: allBudget)
    XCTAssertEqual(jobs.count, 9)
    let centerJob = try XCTUnwrap(jobs.first { $0.requestedTier == .detailed })
    let centerResult = try residency.compile(centerJob)
    guard case let .published(near) = residency.publish(centerResult) else {
      return XCTFail("Complete near tier did not publish")
    }
    XCTAssertFalse(near.isComplete)
    XCTAssertEqual(near.chunks.count, 1)
    XCTAssertEqual(near.chunks.first?.tier, .detailed)

    var finalSnapshot: SanctuaryVegetationResidencySnapshot?
    for job in jobs where job.key != centerJob.key {
      let result = try residency.compile(job)
      if case let .published(value) = residency.publish(result) {
        finalSnapshot = value
      }
    }
    let final = try XCTUnwrap(finalSnapshot)
    XCTAssertTrue(final.isComplete)
    XCTAssertEqual(final.chunks.count, 9)
    XCTAssertEqual(Set(final.chunks.map(\.key)).count, final.chunks.count)
    XCTAssertEqual(final.chunks.filter { $0.tier == .detailed }.count, 1)
    XCTAssertEqual(final.chunks.filter { $0.tier == .community }.count, 8)
    XCTAssertTrue(final.farCanopies.isEmpty)
    XCTAssertLessThanOrEqual(residency.cachedChunkCount, configuration.maximumCachedChunks)

    let adjacent = try residency.beginRequest(
      around: SIMD2(8_300 + SanctuaryTerrainChunkKey.size, -3_000))
    let entering = residency.nextJobs(for: adjacent.token, budget: allBudget)
    XCTAssertEqual(entering.count, 3, "A one-chunk move adds only one 3-chunk strip")
    for job in entering { residency.abandon(job) }

    // Replacing incomplete requests cannot retain pending populations from every teleport.
    for x: Float in [-12_000, -9_000, -6_000, -3_000, 0, 3_000, 6_000, 9_000, 12_000] {
      _ = try residency.beginRequest(around: SIMD2(x, -3_000))
      XCTAssertLessThanOrEqual(residency.snapshot.chunks.count,
        configuration.maximumResidentChunks)
      XCTAssertLessThanOrEqual(residency.cachedChunkCount,
        configuration.maximumCachedChunks)
    }
  }

  func testInvalidResidencyInputsAndTooSmallBudgetRejectWithoutWork() throws {
    XCTAssertThrowsError(try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 2, middleChunkRadius: 1, farChunkRadius: 4))
    XCTAssertThrowsError(try SanctuaryVegetationResidencyConfiguration(
      nearChunkRadius: 1, middleChunkRadius: 2, farChunkRadius: 5))
    XCTAssertThrowsError(try SanctuaryVegetationWorkBudget(
      maximumChunks: 0, maximumCandidateCells: 256))
    var residency = SanctuaryBiomeVegetationResidency()
    XCTAssertThrowsError(try residency.beginRequest(around: SIMD2(.nan, 0)))
    XCTAssertThrowsError(try residency.beginRequest(around: SIMD2(16_001, 0)))
    let request = try residency.beginRequest(around: SIMD2(0, 0))
    let small = try SanctuaryVegetationWorkBudget(
      maximumChunks: 1, maximumCandidateCells: 255)
    XCTAssertTrue(residency.nextJobs(for: request.token, budget: small).isEmpty)
    XCTAssertEqual(residency.scheduledChunkCount, 0)
  }
}
