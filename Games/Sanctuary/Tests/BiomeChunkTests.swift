import XCTest
@testable import SanctuaryContent
import simd

final class BiomeChunkTests: XCTestCase {
  func testNeighborhoodIsBoundedAndMovesWithTheCamera() {
    let first = SanctuaryTerrainChunkKey.neighborhood(around: SIMD2<Float>(0, 24))
    XCTAssertEqual(first.count, 9)
    XCTAssertTrue(first.allSatisfy(\.intersectsWorld))
    let moved = SanctuaryTerrainChunkKey.neighborhood(around: SIMD2<Float>(2_000, 24))
    XCTAssertEqual(moved.count, 9)
    XCTAssertNotEqual(Set(first), Set(moved))
  }

  func testEdgeNeighborhoodNeverPlansOutsideFiniteWorld() {
    let edge = SanctuaryTerrainChunkKey.neighborhood(around: SIMD2<Float>(16_000, 16_000))
    XCTAssertFalse(edge.isEmpty)
    XCTAssertLessThanOrEqual(edge.count, 4)
    XCTAssertTrue(edge.allSatisfy(\.intersectsWorld))
  }

  func testChunkPlansAreDeterministicAndCarryRealRegionalFeatures() {
    let geography = SanctuaryGeography()
    for landmark in SanctuaryGeography.landmarks {
      let key = SanctuaryTerrainChunkKey(containing: landmark.coordinate)
      let first = geography.plan(for: key)
      XCTAssertEqual(first, geography.plan(for: key), landmark.id)
      XCTAssertFalse(first.features.isEmpty, landmark.id)
      XCTAssertTrue(first.features.allSatisfy { key.minimum.x <= $0.coordinate.x
        && $0.coordinate.x <= key.maximum.x && key.minimum.y <= $0.coordinate.y
        && $0.coordinate.y <= key.maximum.y }, landmark.id)
    }
  }

  func testIntermediateTravelChunksIncludeDiscoveries() {
    let geography = SanctuaryGeography()
    var discoveryCount = 0
    var featureCount = 0
    for z in -8...8 {
      for x in -8...8 {
        let plan = geography.plan(for: .init(x: x, z: z))
        featureCount += plan.features.count
        discoveryCount += plan.features.filter(\.isDiscovery).count
      }
    }
    XCTAssertGreaterThan(featureCount, 5_000)
    XCTAssertGreaterThan(discoveryCount, 30)
  }

  func testStreamedFeatureCollisionUsesGardenComposedHeight() throws {
    let geography = SanctuaryGeography()
    let plan = geography.plan(for: .init(x: 5, z: 5))
    let feature = try XCTUnwrap(plan.features.first {
      ![SanctuaryFeatureKind.reedCluster, .flowerPatch].contains($0.kind)
    })
    var garden = HabitatGarden()
    _ = try garden.apply(
      .sculpt(
        .raise, at: .init(x: feature.coordinate.x, z: feature.coordinate.y),
        radius: 6, amount: 4, targetHeight: nil), expectedRevision: 0)
    var layout = SanctuaryLayout()
    let cabinCount = layout.solids.count
    layout.setStreamedCollision(plans: [plan], garden: garden)
    let solid = try XCTUnwrap(layout.solids.dropFirst(cabinCount).first {
      abs($0.position.x - feature.coordinate.x) < 0.001
        && abs($0.position.z - feature.coordinate.y) < 0.001
    })
    let base = layout.terrain.height(feature.coordinate.x, feature.coordinate.y)
    let expected = garden.surfaceHeight(
      baseHeight: base, at: .init(x: feature.coordinate.x, z: feature.coordinate.y))
    XCTAssertEqual(solid.position.y, expected, accuracy: 0.0001)
    XCTAssertEqual(expected, base + 4, accuracy: 0.0001)
  }

  func testArbitraryTerrainEditSelectsOnlyLocalOneMetreCells() throws {
    let key = SanctuaryTerrainChunkKey(x: 4, z: 3)
    let center = key.minimum + SIMD2<Float>(123.37, 211.42)
    var garden = HabitatGarden()
    _ = try garden.apply(
      .sculpt(.raise, at: .init(x: center.x, z: center.y), radius: 6, amount: 4,
        targetHeight: nil), expectedRevision: 0)
    let cells = SanctuaryTerrainTessellation.refinedCells(in: key, garden: garden)
    XCTAssertFalse(cells.isEmpty)
    XCTAssertLessThan(cells.count, 12)
    let containing = SanctuaryTerrainCell(
      x: Int(floor((center.x - key.minimum.x) / 8)),
      z: Int(floor((center.y - key.minimum.y) / 8)))
    XCTAssertTrue(cells.contains(containing))

    let terrain = Terrain()
    func composed(_ p: SIMD2<Float>) -> Float {
      garden.surfaceHeight(
        baseHeight: terrain.height(p.x, p.y), at: .init(x: p.x, z: p.y))
    }
    let local = center - key.minimum
    let lower = key.minimum + SIMD2(floor(local.x), floor(local.y))
    let fraction = center - lower
    let a = lower, b = lower + SIMD2<Float>(1, 0)
    let c = lower + SIMD2<Float>(0, 1), d = lower + SIMD2<Float>(1, 1)
    let planar: Float
    if fraction.x + fraction.y <= 1 {
      planar = composed(a) * (1 - fraction.x - fraction.y)
        + composed(b) * fraction.x + composed(c) * fraction.y
    } else {
      planar = composed(b) * (1 - fraction.y) + composed(c) * (1 - fraction.x)
        + composed(d) * (fraction.x + fraction.y - 1)
    }
    XCTAssertEqual(planar, composed(center), accuracy: 0.12)
  }

  func testTerrainEditStraddlingChunkEdgeRefinesBothMatchingEdges() throws {
    let center = SIMD2<Float>(512.35, 1_234.6)
    var garden = HabitatGarden()
    _ = try garden.apply(
      .sculpt(.lower, at: .init(x: center.x, z: center.y), radius: 6, amount: 3,
        targetHeight: nil), expectedRevision: 0)
    let left = SanctuaryTerrainChunkKey(containing: center - SIMD2<Float>(1, 0))
    let right = SanctuaryTerrainChunkKey(containing: center + SIMD2<Float>(1, 0))
    XCTAssertNotEqual(left, right)
    let leftCells = SanctuaryTerrainTessellation.refinedCells(in: left, garden: garden)
    let rightCells = SanctuaryTerrainTessellation.refinedCells(in: right, garden: garden)
    XCTAssertTrue(leftCells.contains { $0.x == SanctuaryTerrainTessellation.baseResolution - 1 })
    XCTAssertTrue(rightCells.contains { $0.x == 0 })
    let boundaryX = right.minimum.x
    let sampleZ = floor(center.y) + 0.375
    let terrain = Terrain()
    func composed(_ z: Float) -> Float {
      garden.surfaceHeight(
        baseHeight: terrain.height(boundaryX, z),
        at: .init(x: boundaryX, z: z))
    }
    let z0 = floor(sampleZ), fraction = sampleZ - z0
    let edgeInterpolation = composed(z0) * (1 - fraction) + composed(z0 + 1) * fraction
    XCTAssertEqual(edgeInterpolation, composed(sampleZ), accuracy: 0.12)
  }

  func testCabinBoundaryAlignsExactlyWithCoarseCellEdges() {
    XCTAssertEqual(SanctuaryTerrainTessellation.cabinExtent, 160)
    XCTAssertEqual(
      SanctuaryTerrainTessellation.cabinExtent * 2
        / Float(SanctuaryTerrainTessellation.cabinResolution), 1)
    let key = SanctuaryTerrainChunkKey(x: 0, z: 0)
    let firstOutside = SanctuaryTerrainTessellation.cellBounds(
      in: key, cell: .init(x: 20, z: 20))
    let lastInside = SanctuaryTerrainTessellation.cellBounds(
      in: key, cell: .init(x: 19, z: 19))
    XCTAssertEqual(lastInside.maximum.x, 160)
    XCTAssertEqual(firstOutside.minimum.x, 160)
  }
}
