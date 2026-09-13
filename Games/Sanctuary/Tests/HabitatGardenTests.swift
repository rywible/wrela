import XCTest

@testable import SanctuaryContent

final class HabitatGardenTests: XCTestCase {
  private func plant(
    _ planting: HabitatGarden.Planting, x: Float, z: Float, radius: Float,
    in garden: inout HabitatGarden
  ) throws -> HabitatGarden.PatchID {
    try garden.apply(
      .plant(planting, at: .init(x: x, z: z), radius: radius), expectedRevision: garden.revision)
  }

  func testRoundTripAndFutureReplayAreExact() throws {
    var original = HabitatGarden()
    _ = try plant(.grove, x: -4, z: 3, radius: 6, in: &original)
    _ = try plant(.flowers, x: 1, z: -2, radius: 3, in: &original)
    let data = try JSONEncoder().encode(original)
    var restored = try JSONDecoder().decode(HabitatGarden.self, from: data)
    XCTAssertEqual(restored, original)

    let command: HabitatGarden.Command = .plant(.shallowWater, at: .init(x: 1, z: -2), radius: 2)
    let referenceID = try original.apply(command, expectedRevision: original.revision)
    let restoredID = try restored.apply(command, expectedRevision: restored.revision)
    XCTAssertEqual(restoredID, referenceID)
    XCTAssertEqual(restored, original)
  }

  func testOverlappingPatchesRemainAfterExplicitRestoreAndUndo() throws {
    var garden = HabitatGarden()
    let water = try plant(.shallowWater, x: 0, z: 0, radius: 5, in: &garden)
    let flowers = try plant(.flowers, x: 0, z: 0, radius: 5, in: &garden)
    XCTAssertTrue(garden.conditions(at: .init(x: 0, z: 0)).hasShallowWater)
    XCTAssertGreaterThan(garden.conditions(at: .init(x: 0, z: 0)).flowering, 0.99)

    XCTAssertEqual(try garden.apply(.restore(water), expectedRevision: garden.revision), water)
    let afterWaterRestore = garden.conditions(at: .init(x: 0, z: 0))
    XCTAssertFalse(afterWaterRestore.hasShallowWater)
    XCTAssertGreaterThan(afterWaterRestore.flowering, 0.99)

    XCTAssertEqual(try garden.apply(.undo, expectedRevision: garden.revision), flowers)
    XCTAssertEqual(garden.conditions(at: .init(x: 0, z: 0)), .init())
  }

  func testInvalidAndStaleCommandsPreserveState() throws {
    var garden = HabitatGarden()
    let before = garden
    XCTAssertThrowsError(
      try garden.apply(.plant(.grove, at: .init(x: .nan, z: 0), radius: 2), expectedRevision: 0)
    )
    XCTAssertEqual(garden, before)

    XCTAssertThrowsError(
      try garden.apply(.plant(.reeds, at: .init(x: 0, z: 0), radius: 2), expectedRevision: 1)
    )
    XCTAssertEqual(garden, before)
    XCTAssertThrowsError(try garden.apply(.restore(99), expectedRevision: 0))
    XCTAssertEqual(garden, before)
  }

  func testConditionsAreLocalizedAndBounded() throws {
    var garden = HabitatGarden()
    _ = try plant(.grove, x: 3, z: -2, radius: 4, in: &garden)
    _ = try plant(.reeds, x: 3, z: -2, radius: 4, in: &garden)
    let center = garden.conditions(at: .init(x: 3, z: -2))
    XCTAssertGreaterThan(center.shelter, 0.99)
    XCTAssertGreaterThan(center.groundcover, 0.5)
    XCTAssertEqual(garden.conditions(at: .init(x: 20, z: -2)), .init())
  }

  func testTerrainCompositionIsDeterministicAndRestoreRevealsOverlap() throws {
    var garden = HabitatGarden()
    let raise = try garden.apply(
      .sculpt(.raise, at: .init(x: 4, z: -1), radius: 6, amount: 2, targetHeight: nil),
      expectedRevision: garden.revision)
    _ = try garden.apply(
      .sculpt(.lower, at: .init(x: 4, z: -1), radius: 6, amount: 0.5, targetHeight: nil),
      expectedRevision: garden.revision)
    XCTAssertEqual(garden.surfaceHeight(baseHeight: 10, at: .init(x: 4, z: -1)), 11.5, accuracy: 0.0001)

    _ = try garden.apply(.restore(raise), expectedRevision: garden.revision)
    XCTAssertEqual(garden.surfaceHeight(baseHeight: 10, at: .init(x: 4, z: -1)), 9.5, accuracy: 0.0001)
  }

  func testSmoothWaterAndInvalidTerrainPreserveState() throws {
    var garden = HabitatGarden()
    _ = try plant(.shallowWater, x: 0, z: 0, radius: 4, in: &garden)
    _ = try garden.apply(
      .sculpt(.smooth, at: .init(x: 0, z: 0), radius: 4, amount: 1, targetHeight: 3),
      expectedRevision: garden.revision)
    XCTAssertEqual(garden.surfaceHeight(baseHeight: 10, at: .init(x: 0, z: 0)), 3, accuracy: 0.0001)
    XCTAssertEqual(garden.waterSurfaceHeight(baseHeight: 10, at: .init(x: 0, z: 0))!, 3.2, accuracy: 0.0001)

    let before = garden
    XCTAssertThrowsError(try garden.apply(
      .sculpt(.smooth, at: .init(x: 0, z: 0), radius: 2, amount: 1, targetHeight: nil),
      expectedRevision: garden.revision))
    XCTAssertEqual(garden, before)
  }

  func testLegacyGardenWithoutTerrainFieldsMigratesWithDefaults() throws {
    var garden = HabitatGarden()
    _ = try plant(.flowers, x: 2, z: 3, radius: 2, in: &garden)
    var legacy = try JSONSerialization.jsonObject(with: JSONEncoder().encode(garden)) as! [String: Any]
    legacy.removeValue(forKey: "terrainPatches")
    legacy.removeValue(forKey: "activeOrder")
    let restored = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONSerialization.data(withJSONObject: legacy))
    XCTAssertEqual(restored.patches, garden.patches)
    XCTAssertTrue(restored.terrainPatches.isEmpty)
    XCTAssertEqual(restored.revision, garden.revision)
  }

  func testPlayTimePersistsAcrossReplayAndUndo() throws {
    var garden = HabitatGarden()
    let water = try garden.apply(
      .plant(.shallowWater, at: .init(x: 0, z: 0), radius: 3),
      expectedRevision: garden.revision, atPlaySeconds: 12.5)
    let earth = try garden.apply(
      .sculpt(.raise, at: .init(x: 0, z: 0), radius: 3, amount: 1, targetHeight: nil),
      expectedRevision: garden.revision, atPlaySeconds: 13)
    XCTAssertEqual(
      try XCTUnwrap(try XCTUnwrap(garden.patches.first(where: { $0.id == water })).createdAtPlaySeconds),
      12.5)
    XCTAssertEqual(
      try XCTUnwrap(try XCTUnwrap(garden.terrainPatches.first(where: { $0.id == earth })).createdAtPlaySeconds),
      13)

    let data = try JSONEncoder().encode(garden)
    var replay = try JSONDecoder().decode(HabitatGarden.self, from: data)
    XCTAssertEqual(replay, garden)
    XCTAssertEqual(try replay.apply(.undo, expectedRevision: replay.revision), earth)
    XCTAssertEqual(try XCTUnwrap(try XCTUnwrap(replay.patches.first).createdAtPlaySeconds), 12.5)
    let replayed = try replay.apply(
      .sculpt(.raise, at: .init(x: 0, z: 0), radius: 3, amount: 1, targetHeight: nil),
      expectedRevision: replay.revision, atPlaySeconds: 13)
    XCTAssertEqual(replayed, earth + 1)
    XCTAssertEqual(try XCTUnwrap(try XCTUnwrap(replay.terrainPatches.last).createdAtPlaySeconds), 13)
  }

  func testInvalidPlayTimeAndFutureWorldValidationPreserveGarden() throws {
    var garden = HabitatGarden()
    let before = garden
    XCTAssertThrowsError(try garden.apply(
      .plant(.flowers, at: .init(x: 0, z: 0), radius: 2), expectedRevision: garden.revision,
      atPlaySeconds: -0.01))
    XCTAssertEqual(garden, before)
    XCTAssertThrowsError(try garden.apply(
      .plant(.flowers, at: .init(x: 0, z: 0), radius: 2), expectedRevision: garden.revision,
      atPlaySeconds: .nan))
    XCTAssertEqual(garden, before)

    _ = try garden.apply(
      .plant(.flowers, at: .init(x: 0, z: 0), radius: 2), expectedRevision: garden.revision,
      atPlaySeconds: 4)
    XCTAssertThrowsError(try garden.validate(createdAtOrBefore: 3))
    XCTAssertNoThrow(try garden.validate(createdAtOrBefore: 4))

    var legacy = HabitatGarden()
    _ = try legacy.apply(.plant(.flowers, at: .init(x: 0, z: 0), radius: 2), expectedRevision: 0)
    XCTAssertNoThrow(try legacy.validate(createdAtOrBefore: 0))
  }
}
