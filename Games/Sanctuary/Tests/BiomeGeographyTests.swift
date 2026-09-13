import XCTest
@testable import SanctuaryContent
import simd

final class BiomeGeographyTests: XCTestCase {
  private func orderedWeightTotal(_ sample: SanctuaryBiomeSample) -> Float {
    SanctuaryBiome.allCases.reduce(0) { $0 + (sample.weights[$1] ?? 0) }
  }

  func testEveryEnvironmentHasOneValidNamedLandmark() {
    XCTAssertEqual(Set(SanctuaryGeography.landmarks.map(\.biome)), Set(SanctuaryBiome.allCases))
    XCTAssertEqual(Set(SanctuaryGeography.landmarks.map(\.id)).count, SanctuaryGeography.landmarks.count)
    for landmark in SanctuaryGeography.landmarks {
      XCTAssertFalse(landmark.id.isEmpty)
      XCTAssertFalse(landmark.name.isEmpty)
      XCTAssertTrue(SanctuaryGeography.bounds.contains(landmark.coordinate), landmark.id)
    }
  }

  func testLandmarksSampleTheirOwnBiomeAndNormalizeWeights() {
    let geography = SanctuaryGeography()
    for landmark in SanctuaryGeography.landmarks {
      let sample = try! geography.sample(at: landmark.coordinate)
      XCTAssertEqual(sample.primary, landmark.biome, landmark.id)
      XCTAssertEqual(sample.weights.count, SanctuaryBiome.allCases.count)
      XCTAssertEqual(orderedWeightTotal(sample), 1, accuracy: 0.00001)
      if landmark.biome == .lake || landmark.biome == .ocean {
        XCTAssertEqual(sample.proposedTraversal, [.fly])
      } else {
        XCTAssertTrue(sample.proposedTraversal.contains(.walk))
      }
    }
  }

  func testNearbyQueriesVaryContinuously() {
    let geography = SanctuaryGeography()
    let start = SIMD2<Float>(700, -180)
    let a = try! geography.sample(at: start).weights
    let b = try! geography.sample(at: start + SIMD2(1, 1)).weights
    for biome in SanctuaryBiome.allCases {
      XCTAssertLessThan(abs((a[biome] ?? 0) - (b[biome] ?? 0)), 0.01, biome.rawValue)
    }
  }

  func testRoutesFormOneConnectedLandmarkGraphAndDeclareCapabilities() {
    let ids = Set(SanctuaryGeography.landmarks.map(\.id))
    XCTAssertEqual(Set(SanctuaryGeography.routes.map(\.id)).count, SanctuaryGeography.routes.count)
    for route in SanctuaryGeography.routes {
      XCTAssertTrue(ids.contains(route.from), route.id)
      XCTAssertTrue(ids.contains(route.to), route.id)
    }
    var reached: Set<String> = ["cabin-glade"]
    var changed = true
    while changed {
      changed = false
      for route in SanctuaryGeography.routes where ids.contains(route.from) && ids.contains(route.to) {
        if reached.contains(route.from) && reached.insert(route.to).inserted { changed = true }
        if reached.contains(route.to) && reached.insert(route.from).inserted { changed = true }
      }
    }
    XCTAssertEqual(reached, ids)
    XCTAssertTrue(SanctuaryGeography.routes.contains { $0.requiredTraversal == .ride })
    XCTAssertTrue(SanctuaryGeography.routes.contains { $0.requiredTraversal == .fly })
  }

  func testOutsideMapIsNotWalkable() {
    let sample = try! SanctuaryGeography().sample(at: SIMD2(17_000, 0))
    XCTAssertEqual(sample.proposedTraversal, [])
    XCTAssertEqual(orderedWeightTotal(sample), 1, accuracy: 0.00001)
  }

  func testInvalidOrUnreasonablyDistantCoordinatesAreRejected() {
    let geography = SanctuaryGeography()
    for point in [SIMD2<Float>(.nan, 0), SIMD2<Float>(.infinity, 0), SIMD2<Float>(1_000_001, 0)] {
      XCTAssertThrowsError(try geography.sample(at: point)) { error in
        XCTAssertEqual(error as? SanctuaryGeographyError, .invalidCoordinate)
      }
    }
  }

  func testRepeatableSamplesUseTheSameOrderedNormalization() {
    let geography = SanctuaryGeography()
    let point = SIMD2<Float>(2_000, 600)
    let first = try! geography.sample(at: point)
    for _ in 0..<16 {
      XCTAssertEqual(try! geography.sample(at: point), first)
    }
  }

  func testOpenOceanDoesNotProposeWalking() {
    let ocean = SanctuaryGeography.landmarks.first { $0.biome == .ocean }!
    let sample = try! SanctuaryGeography().sample(at: ocean.coordinate)
    XCTAssertEqual(sample.proposedTraversal, [.fly])
    XCTAssertFalse(sample.proposedTraversal.contains(.walk))
  }

  func testWorldIsGrandButFiniteAndClampsToItsEdge() {
    XCTAssertEqual(SanctuaryGeography.bounds.size, SIMD2(repeating: 32_000))
    XCTAssertTrue(SanctuaryGeography.bounds.contains(SIMD2(15_999, -15_999)))
    XCTAssertFalse(SanctuaryGeography.bounds.contains(SIMD2(16_001, 0)))
    XCTAssertEqual(
      SanctuaryGeography.bounds.clamped(SIMD2(20_000, -20_000), inset: 4),
      SIMD2(15_996, -15_996))
    XCTAssertGreaterThan(SanctuaryGeography.bounds.signedDistanceToBoundary(SIMD2.zero), 0)
    XCTAssertLessThan(
      SanctuaryGeography.bounds.signedDistanceToBoundary(SIMD2(20_000, 0)), 0)
  }

  func testMajorDestinationsActuallySpanTheLargeWorld() {
    let distances = SanctuaryGeography.landmarks.map { length($0.coordinate) }
    XCTAssertEqual(distances.min(), 0)
    XCTAssertGreaterThan(distances.max()!, 15_000)
    XCTAssertTrue(SanctuaryGeography.landmarks.allSatisfy {
      SanctuaryGeography.bounds.contains($0.coordinate)
    })
  }
}
