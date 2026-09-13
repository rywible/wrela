import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class WaterFieldTests: XCTestCase {
  func testExistingNaturalInteriorsRetainBodySurfaceAndDepth() throws {
    let terrain = Terrain()
    let geography = SanctuaryGeography()
    let samples: [(SanctuaryWaterBody, SIMD2<Float>, Float)] = [
      (.creek, geography.landmark(for: .meadow).coordinate, 0.42),
      (.wetland, geography.landmark(for: .wetland).coordinate + SIMD2(30, 0), 0.28),
      (.tidepool, geography.landmark(for: .tidepool).coordinate + SIMD2(60, 0), 0.34),
    ]
    for (body, point, expectedDepth) in samples {
      let field = try XCTUnwrap(terrain.waterField(for: body, at: point))
      XCTAssertGreaterThanOrEqual(field.signedCoverage, Terrain.waterShoreTransitionWidth, body.rawValue)
      XCTAssertEqual(field.depth, expectedDepth, accuracy: 0.0001, body.rawValue)
      XCTAssertEqual(field.surfaceHeight, field.bedHeight + expectedDepth, accuracy: 0.0001)
      XCTAssertEqual(terrain.water(at: point), field.waterSample)
    }

    for body in [SanctuaryWaterBody.lake, .ocean] {
      let point = geography.landmark(for: body == .lake ? .lake : .ocean).coordinate
      let field = try XCTUnwrap(terrain.waterField(for: body, at: point))
      let production = try XCTUnwrap(terrain.water(at: point))
      XCTAssertEqual(field.waterSample, production)
      XCTAssertEqual(field.surfaceHeight, production.surfaceHeight, accuracy: 0.0001)
      XCTAssertEqual(field.depth, production.depth, accuracy: 0.0001)
    }
  }

  func testCreekSignedBankTapersContinuouslyToBed() throws {
    let terrain = Terrain()
    let a = SIMD2<Float>(700, -180) * 3.5
    let b = SIMD2<Float>(480, 620) * 3.5
    let midpoint = (a + b) * 0.5
    let tangent = normalize(b - a)
    let normal = SIMD2<Float>(-tangent.y, tangent.x)
    let waviness = abs(15 * sin((midpoint.x + midpoint.y) * 0.009))
    let bank = midpoint + normal * (13 + waviness)

    let boundary = try XCTUnwrap(terrain.waterField(for: .creek, at: bank))
    let oneMetreInside = try XCTUnwrap(terrain.waterField(for: .creek, at: bank - normal))
    let twoMetresInside = try XCTUnwrap(terrain.waterField(for: .creek, at: bank - normal * 2))
    let outside = try XCTUnwrap(terrain.waterField(for: .creek, at: bank + normal))
    XCTAssertEqual(boundary.signedCoverage, 0, accuracy: 0.002)
    XCTAssertEqual(boundary.surfaceHeight, boundary.bedHeight, accuracy: 0.0001)
    XCTAssertEqual(oneMetreInside.depth, 0.21, accuracy: 0.002)
    XCTAssertEqual(twoMetresInside.depth, 0.42, accuracy: 0.002)
    XCTAssertLessThan(outside.signedCoverage, 0)
    XCTAssertEqual(outside.surfaceHeight, outside.bedHeight, accuracy: 0.0001)
    XCTAssertNil(outside.waterSample)
  }

  func testResolvedNaturalFieldMatchesProductionAcrossRegions() {
    let terrain = Terrain()
    let geography = SanctuaryGeography()
    var points = SanctuaryGeography.landmarks.map(\.coordinate)
    points += [
      geography.landmark(for: .wetland).coordinate + SIMD2(30, 0),
      geography.landmark(for: .tidepool).coordinate + SIMD2(60, 0),
      SIMD2(0, 0), SIMD2(15_900, 15_900),
    ]
    for point in points {
      let production = terrain.water(at: point)
      let resolved = terrain.resolvedWaterField(at: point)
      XCTAssertEqual(resolved?.waterSample, production, "\(point)")
      if let resolved {
        XCTAssertGreaterThan(resolved.signedCoverage, 0)
        XCTAssertGreaterThanOrEqual(resolved.surfaceHeight, resolved.bedHeight)
      }
    }
  }

  func testComposedRaisedBedDriesNaturalWaterAndNeverLeavesSurfaceBelowBed() throws {
    let terrain = Terrain()
    let lake = SanctuaryGeography.moonlakeFootprint
    let point = lake.center + SIMD2(lake.radii.x * 0.99, 0)
    XCTAssertEqual(terrain.water(at: point)?.body, .lake)

    var garden = HabitatGarden()
    for _ in 0..<2 {
      _ = try garden.apply(
        .sculpt(.raise, at: .init(x: point.x, z: point.y), radius: 16, amount: 4,
          targetHeight: nil),
        expectedRevision: garden.revision)
    }
    let base = terrain.height(point.x, point.y)
    let composed = garden.surfaceHeight(baseHeight: base, at: .init(x: point.x, z: point.y))
    let lakeField = try XCTUnwrap(terrain.waterField(for: .lake, at: point, bedHeight: composed))
    XCTAssertLessThanOrEqual(lakeField.signedCoverage, 0)
    XCTAssertEqual(lakeField.surfaceHeight, composed, accuracy: 0.0001)
    XCTAssertEqual(lakeField.depth, 0)
    XCTAssertNil(terrain.resolvedWaterField(at: point, garden: garden))
  }

  func testGardenFieldUsesSavedUnionAndComposedBed() throws {
    var garden = HabitatGarden()
    _ = try garden.apply(
      .plant(.shallowWater, at: .init(x: 2, z: 3), radius: 4),
      expectedRevision: garden.revision)
    _ = try garden.apply(
      .sculpt(.lower, at: .init(x: 2, z: 3), radius: 4, amount: 2, targetHeight: nil),
      expectedRevision: garden.revision)

    let center = try XCTUnwrap(garden.waterField(baseHeight: 10, at: .init(x: 2, z: 3)))
    XCTAssertEqual(center.source, .garden)
    XCTAssertEqual(center.signedCoverage, 4, accuracy: 0.0001)
    XCTAssertEqual(center.bedHeight, 8, accuracy: 0.0001)
    XCTAssertEqual(center.depth, 0.2, accuracy: 0.0001)
    XCTAssertEqual(center.surfaceHeight, 8.2, accuracy: 0.0001)

    let dry = try XCTUnwrap(garden.waterField(baseHeight: 10, at: .init(x: 7, z: 3)))
    XCTAssertEqual(dry.signedCoverage, -1, accuracy: 0.0001)
    XCTAssertFalse(dry.isWet)
    XCTAssertEqual(dry.surfaceHeight, dry.bedHeight, accuracy: 0.0001)

    let restored = try JSONDecoder().decode(HabitatGarden.self, from: JSONEncoder().encode(garden))
    XCTAssertEqual(restored.waterField(baseHeight: 10, at: .init(x: 2, z: 3)), center)
  }

  func testResolvedWaterChoosesHigherSurfaceBetweenAuthoredAndNatural() throws {
    let terrain = Terrain()
    let point = SanctuaryGeography().landmark(for: .meadow).coordinate
    XCTAssertEqual(terrain.water(at: point)?.body, .creek)
    var garden = HabitatGarden()
    _ = try garden.apply(
      .plant(.shallowWater, at: .init(x: point.x, z: point.y), radius: 4),
      expectedRevision: garden.revision)
    let resolved = try XCTUnwrap(terrain.resolvedWaterField(at: point, garden: garden))
    XCTAssertEqual(resolved.source, .natural(.creek))
    XCTAssertEqual(resolved.depth, 0.42, accuracy: 0.0001)

    let dryPoint = SIMD2<Float>(80, 40)
    _ = try garden.apply(
      .plant(.shallowWater, at: .init(x: dryPoint.x, z: dryPoint.y), radius: 4),
      expectedRevision: garden.revision)
    let authored = try XCTUnwrap(terrain.resolvedWaterField(at: dryPoint, garden: garden))
    XCTAssertEqual(authored.source, .garden)
    XCTAssertEqual(authored.depth, 0.2, accuracy: 0.0001)
  }

  func testFieldsRejectInvalidOrOutOfWorldQueries() {
    let terrain = Terrain()
    XCTAssertNil(terrain.waterField(for: .creek, at: SIMD2(.nan, 0)))
    XCTAssertNil(terrain.waterField(for: .lake, at: SIMD2(0, .infinity)))
    XCTAssertNil(terrain.waterField(for: .ocean, at: SIMD2(16_001, 0)))
    XCTAssertNil(terrain.waterField(for: .creek, at: SIMD2(0, 0), bedHeight: .nan))
    XCTAssertNil(terrain.resolvedWaterField(at: SIMD2(-16_001, 0)))

    var garden = HabitatGarden()
    XCTAssertNil(garden.waterField(baseHeight: 0, at: .init(x: 0, z: 0)))
    XCTAssertNil(garden.waterField(baseHeight: .nan, at: .init(x: 0, z: 0)))
  }
}
