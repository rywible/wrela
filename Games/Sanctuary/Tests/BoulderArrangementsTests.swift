import XCTest
@testable import SanctuaryContent
import FieldCompiler
import FieldCore
import simd

final class BoulderArrangementsTests: XCTestCase {
  private func sourceBoulder() throws -> (SanctuaryChunkPlan, SanctuaryChunkFeature) {
    let geography = SanctuaryGeography()
    for z in -12...12 {
      for x in -12...12 {
        let plan = geography.plan(for: .init(x: x, z: z))
        if let feature = plan.features.first(where: { $0.kind == .boulder }) {
          return (plan, feature)
        }
      }
    }
    throw XCTSkip("Deterministic geography produced no boulder in searched chunks")
  }

  func testFeatureIDResolvesBackToDeterministicSourceBoulder() throws {
    let (plan, feature) = try sourceBoulder()
    let geography = SanctuaryGeography()
    XCTAssertEqual(SanctuaryGeography.chunkKey(forFeatureID: feature.id), plan.key)
    XCTAssertEqual(geography.feature(id: feature.id), feature)
    XCTAssertNil(geography.feature(id: "not-a-source-feature"))
  }

  func testPushIsBoundedAttributedAndRejectsAtomically() throws {
    let (_, feature) = try sourceBoulder()
    var arrangements = BoulderArrangements()
    _ = try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(3, 4), distance: 2,
        helperID: "moonhart-001"), expectedRevision: 0)
    let displacement = try XCTUnwrap(arrangements.displacement(for: feature.id))
    XCTAssertEqual(displacement.offset, SIMD2<Float>(1.2, 1.6))
    XCTAssertEqual(displacement.lastHelperID, "moonhart-001")
    XCTAssertEqual(displacement.pushCount, 1)
    XCTAssertEqual(arrangements.resolvedCoordinate(for: feature), feature.coordinate + displacement.offset)

    _ = try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(-1, 0), distance: 1,
        helperID: "stonebear-001"), expectedRevision: arrangements.revision)
    XCTAssertEqual(arrangements.displacement(for: feature.id)?.lastHelperID, "stonebear-001")
    _ = try arrangements.apply(.undo, expectedRevision: arrangements.revision)
    XCTAssertEqual(arrangements.displacement(for: feature.id)?.lastHelperID, "moonhart-001")
    XCTAssertEqual(arrangements.history.last?.helperID, "moonhart-001")

    let before = arrangements
    XCTAssertThrowsError(try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(1, 0), distance: 2.01,
        helperID: "moonhart-001"), expectedRevision: arrangements.revision))
    XCTAssertEqual(arrangements, before)
    XCTAssertThrowsError(try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(.nan, 0), distance: 1,
        helperID: "moonhart-001"), expectedRevision: arrangements.revision))
    XCTAssertEqual(arrangements, before)
  }

  func testOnlyActualBouldersCanMoveAndStaleInputIsAtomic() throws {
    let geography = SanctuaryGeography()
    let plan = geography.plan(for: .init(x: 0, z: 0))
    let other = try XCTUnwrap(plan.features.first { $0.kind != .boulder })
    var arrangements = BoulderArrangements()
    XCTAssertThrowsError(try arrangements.apply(
      .push(featureID: other.id, direction: SIMD2(1, 0), distance: 1, helperID: "helper"),
      expectedRevision: 0)) { error in
        XCTAssertEqual(error as? BoulderArrangementError, .unknownBoulder)
      }
    XCTAssertEqual(arrangements, BoulderArrangements())

    let (_, boulder) = try sourceBoulder()
    XCTAssertThrowsError(try arrangements.apply(
      .push(featureID: boulder.id, direction: SIMD2(1, 0), distance: 1, helperID: "helper"),
      expectedRevision: 3)) { error in
        XCTAssertEqual(error as? BoulderArrangementError, .staleRevision)
      }
    XCTAssertEqual(arrangements, BoulderArrangements())
  }

  func testTotalTravelLimitUndoAndCodableFutureAreExact() throws {
    let (_, feature) = try sourceBoulder()
    var arrangements = BoulderArrangements()
    for _ in 0..<16 {
      _ = try arrangements.apply(
        .push(featureID: feature.id, direction: SIMD2(1, 0), distance: 2,
          helperID: "stonebear-001"), expectedRevision: arrangements.revision)
    }
    XCTAssertEqual(arrangements.displacement(for: feature.id)?.totalTravel, 32)
    let full = arrangements
    XCTAssertThrowsError(try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(-1, 0), distance: 0.05,
        helperID: "stonebear-001"), expectedRevision: arrangements.revision))
    XCTAssertEqual(arrangements, full)

    _ = try arrangements.apply(.undo, expectedRevision: arrangements.revision)
    XCTAssertEqual(arrangements.displacement(for: feature.id)?.totalTravel, 30)
    XCTAssertEqual(arrangements.displacement(for: feature.id)?.offset, SIMD2<Float>(30, 0))
    let restored = try JSONDecoder().decode(
      BoulderArrangements.self, from: JSONEncoder().encode(arrangements))
    XCTAssertEqual(restored, arrangements)
    var futureA = arrangements, futureB = restored
    _ = try futureA.apply(.undo, expectedRevision: futureA.revision)
    _ = try futureB.apply(.undo, expectedRevision: futureB.revision)
    XCTAssertEqual(futureA, futureB)
  }

  func testCollisionAndPresentationResolveTheSameSourceCoordinate() throws {
    let (plan, feature) = try sourceBoulder()
    var arrangements = BoulderArrangements()
    _ = try arrangements.apply(
      .push(featureID: feature.id, direction: SIMD2(-0.6, 0.8), distance: 2,
        helperID: "stonebear-001"), expectedRevision: 0)
    let resolved = arrangements.resolvedCoordinate(for: feature)
    var layout = SanctuaryLayout()
    let cabinCount = layout.solids.count
    layout.setStreamedCollision(plans: [plan], boulders: arrangements)
    let collision = try XCTUnwrap(layout.solids.dropFirst(cabinCount).first {
      $0.name == SanctuaryFeatureKind.boulder.rawValue
        && abs($0.position.x - resolved.x) < 0.001
        && abs($0.position.z - resolved.y) < 0.001
    })
    XCTAssertEqual(SIMD2(collision.position.x, collision.position.z), resolved)
    XCTAssertTrue(arrangements.sourceChunkKeys.contains(plan.key))
    XCTAssertLessThanOrEqual(length(resolved - feature.coordinate), 32)
  }

  func testCollisionFieldMatchesTheRenderedSourceTransform() throws {
    let (plan, feature) = try sourceBoulder()
    let terrain = Terrain()
    let resolved = try XCTUnwrap(
      SanctuaryLayout.resolveFeature(feature, terrain: terrain))
    let field = try XCTUnwrap(SanctuaryLayout.collisionShape(for: resolved))
    var layout = SanctuaryLayout()
    let cabinCount = layout.solids.count
    layout.setStreamedCollision(plans: [plan])
    let solid = try XCTUnwrap(layout.solids.dropFirst(cabinCount).first {
      $0.name == SanctuaryFeatureKind.boulder.rawValue
        && abs($0.position.x - resolved.position.x) < 0.001
        && abs($0.position.z - resolved.position.z) < 0.001
    })
    let sourceMesh = try Mesher.compile(SanctuaryLayout.stoneField, resolution: 18)
    let rotation = simd_quatf(angle: resolved.yaw, axis: V3(0, 1, 0))
    for vertex in sourceMesh.vertices.prefix(64) {
      let local = V3(vertex.position.x, vertex.position.y, vertex.position.z)
      let world = resolved.position + rotation.act(local * resolved.coreScale)
      XCTAssertEqual(solid.value(world), field.value(at: world - resolved.position), accuracy: 0.0001)
      XCTAssertEqual(solid.value(world), 0, accuracy: 0.08)
    }
  }

  func testNaturalWaterOmissionRemovesMatchingCollision() throws {
    let geography = SanctuaryGeography(), terrain = Terrain()
    let lake = geography.landmark(for: .lake).coordinate
    let plans = SanctuaryTerrainChunkKey.neighborhood(around: lake, radius: 3).map {
      geography.plan(for: $0)
    }
    let omitted = try XCTUnwrap(plans.flatMap(\.features).first {
      $0.kind != .seaStack
        && ![SanctuaryFeatureKind.reedCluster, .flowerPatch].contains($0.kind)
        && terrain.water(at: $0.coordinate) != nil
        && SanctuaryLayout.resolveFeature($0, terrain: terrain) == nil
    })
    XCTAssertNil(SanctuaryLayout.resolveFeature(omitted, terrain: terrain))
    var layout = SanctuaryLayout()
    let cabinCount = layout.solids.count
    layout.setStreamedCollision(plans: plans)
    let expected = plans.flatMap(\.features).compactMap {
      SanctuaryLayout.resolveFeature($0, terrain: terrain)
    }.compactMap { SanctuaryLayout.collisionShape(for: $0) }.count
    XCTAssertEqual(layout.solids.count - cabinCount, expected)
  }
}
