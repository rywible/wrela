import Foundation
import XCTest
@testable import SanctuaryContent

final class GardenSurfaceInvalidationTests: XCTestCase {
  func testObservedBoundaryFlowerCastChangesDecorationWithoutDirtyingSurface() throws {
    let point = HabitatGarden.Location(x: 0, z: 18.0344)
    var garden = HabitatGarden()
    let before = SanctuaryGardenSurfaceSource(garden: garden)
    let height = garden.surfaceHeight(baseHeight: 1.32, at: point)
    _ = try garden.apply(.plant(.flowers, at: point, radius: 3),
      expectedRevision: garden.revision, atPlaySeconds: 65.45)

    XCTAssertEqual(garden.revision, 1)
    XCTAssertEqual(garden.patches.count, 1)
    XCTAssertGreaterThan(garden.conditions(at: point).flowering, 0)
    XCTAssertEqual(garden.surfaceHeight(baseHeight: 1.32, at: point), height)
    XCTAssertNil(garden.waterSurfaceHeight(baseHeight: 1.32, at: point))
    let after = SanctuaryGardenSurfaceSource(garden: garden)
    XCTAssertEqual(after, before)
    XCTAssertTrue(after.affectedChunkKeys(comparedTo: before).isEmpty)
    XCTAssertTrue(after.coveredChunkKeys.isEmpty)
  }

  func testDecorationPreservesExistingSculptAndWaterSourceAcrossSaveReopen() throws {
    let point = HabitatGarden.Location(x: 511, z: 24)
    var garden = HabitatGarden()
    _ = try garden.apply(.sculpt(.raise, at: point, radius: 4, amount: 2, targetHeight: nil),
      expectedRevision: garden.revision)
    _ = try garden.apply(.plant(.shallowWater, at: point, radius: 2), expectedRevision: garden.revision)
    let before = SanctuaryGardenSurfaceSource(garden: garden)
    let height = garden.surfaceHeight(baseHeight: 3, at: point)
    let water = garden.waterSurfaceHeight(baseHeight: 3, at: point)
    for planting in [HabitatGarden.Planting.flowers, .reeds, .grove] {
      _ = try garden.apply(.plant(planting, at: point, radius: 3), expectedRevision: garden.revision)
    }
    let reopened = try JSONDecoder().decode(HabitatGarden.self, from: JSONEncoder().encode(garden))
    XCTAssertEqual(reopened, garden)
    XCTAssertEqual(reopened.patches.count, 4)
    XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: reopened), before)
    XCTAssertTrue(SanctuaryGardenSurfaceSource(garden: reopened).affectedChunkKeys(comparedTo: before).isEmpty)
    XCTAssertEqual(reopened.surfaceHeight(baseHeight: 3, at: point), height)
    XCTAssertEqual(reopened.waterSurfaceHeight(baseHeight: 3, at: point), water)
  }

  func testWaterAndUndoInvalidateBothSidesOfItsBankAppearanceBoundary() throws {
    let point = HabitatGarden.Location(x: 510, z: 24)
    var garden = HabitatGarden()
    let dry = SanctuaryGardenSurfaceSource(garden: garden)
    _ = try garden.apply(.plant(.shallowWater, at: point, radius: 1), expectedRevision: 0)
    let wet = SanctuaryGardenSurfaceSource(garden: garden)
    let expected: Set<SanctuaryTerrainChunkKey> = [.init(x: 0, z: 0), .init(x: 1, z: 0)]
    XCTAssertNotEqual(wet, dry)
    XCTAssertEqual(wet.affectedChunkKeys(comparedTo: dry), expected)
    XCTAssertNotNil(garden.waterSurfaceHeight(baseHeight: 0, at: point))

    _ = try garden.apply(.undo, expectedRevision: garden.revision)
    let restored = SanctuaryGardenSurfaceSource(garden: garden)
    XCTAssertEqual(restored, dry)
    XCTAssertEqual(restored.affectedChunkKeys(comparedTo: wet), expected)
    XCTAssertNil(garden.waterSurfaceHeight(baseHeight: 0, at: point))
  }

  func testSameRevisionSculptBranchesHaveDifferentSupportAndDirtyFootprints() throws {
    let point = HabitatGarden.Location(x: 0, z: 24)
    var raised = HabitatGarden(), lowered = HabitatGarden()
    _ = try raised.apply(.sculpt(.raise, at: point, radius: 3, amount: 1, targetHeight: nil), expectedRevision: 0)
    _ = try lowered.apply(.sculpt(.lower, at: point, radius: 3, amount: 1, targetHeight: nil), expectedRevision: 0)
    XCTAssertEqual(raised.revision, lowered.revision)
    XCTAssertNotEqual(raised.surfaceHeight(baseHeight: 0, at: point), lowered.surfaceHeight(baseHeight: 0, at: point))
    let a = SanctuaryGardenSurfaceSource(garden: raised), b = SanctuaryGardenSurfaceSource(garden: lowered)
    XCTAssertNotEqual(a, b)
    XCTAssertEqual(a.affectedChunkKeys(comparedTo: b), [.init(x: -1, z: 0), .init(x: 0, z: 0)])
    XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: nil).affectedChunkKeys(comparedTo: a), a.coveredChunkKeys)
  }

  func testSculptSourcePreservesNoncommutativeSavedOrder() throws {
    let point = HabitatGarden.Location(x: 16, z: 24)
    let raise = HabitatGarden.Command.sculpt(.raise, at: point, radius: 3, amount: 4, targetHeight: nil)
    let smooth = HabitatGarden.Command.sculpt(.smooth, at: point, radius: 3, amount: 0.5, targetHeight: 1)
    var first = HabitatGarden(), second = HabitatGarden()
    for command in [raise, smooth] { _ = try first.apply(command, expectedRevision: first.revision) }
    for command in [smooth, raise] { _ = try second.apply(command, expectedRevision: second.revision) }
    XCTAssertNotEqual(first.surfaceHeight(baseHeight: 0, at: point), second.surfaceHeight(baseHeight: 0, at: point))
    XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: first).terrainPatches.map(\.operation), [.raise, .smooth])
    XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: second).terrainPatches.map(\.operation), [.smooth, .raise])
    XCTAssertNotEqual(SanctuaryGardenSurfaceSource(garden: first), SanctuaryGardenSurfaceSource(garden: second))
  }

  func testSameRevisionDecorationBranchesHaveDistinctActualWorkerReceipts() throws {
    let point = HabitatGarden.Location(x: 0, z: 18.0344)
    var flowers = HabitatGarden(), reeds = HabitatGarden()
    _ = try flowers.apply(.plant(.flowers, at: point, radius: 3), expectedRevision: 0, atPlaySeconds: 10)
    _ = try reeds.apply(.plant(.reeds, at: point, radius: 3), expectedRevision: 0, atPlaySeconds: 10)
    let restoredFlowers = try JSONDecoder().decode(HabitatGarden.self, from: JSONEncoder().encode(flowers))
    let restoredReeds = try JSONDecoder().decode(HabitatGarden.self, from: JSONEncoder().encode(reeds))
    func receipt(_ garden: HabitatGarden) -> SanctuaryRegionalSourceReceipt {
      SanctuaryRegionalSourceReceipt(center: .init(x: 0, z: 0), garden: garden,
        boulders: nil, construction: nil)
    }
    XCTAssertEqual(restoredFlowers.revision, restoredReeds.revision)
    XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: restoredFlowers),
      SanctuaryGardenSurfaceSource(garden: restoredReeds))
    // This is the concrete key consumed before World polls or commits worker results.
    // Comparing only the surface would incorrectly retain the old decorated branch.
    XCTAssertNotEqual(receipt(restoredFlowers), receipt(restoredReeds))
    XCTAssertEqual(receipt(restoredFlowers), receipt(flowers))
    XCTAssertEqual(receipt(restoredReeds).garden?.patches.first?.planting, .reeds)
  }

  func testDecorationUndoRestoreReceiptsChangeWithoutSurfaceWork() throws {
    let point = HabitatGarden.Location(x: 0, z: 18.0344)
    var garden = HabitatGarden()
    _ = try garden.apply(.plant(.grove, at: point, radius: 3), expectedRevision: 0, atPlaySeconds: 10)
    let saved = try JSONEncoder().encode(garden)
    func receipt(_ value: HabitatGarden?) -> SanctuaryRegionalSourceReceipt {
      SanctuaryRegionalSourceReceipt(center: .init(x: 0, z: 0), garden: value,
        boulders: nil, construction: nil)
    }
    let planted = receipt(garden)
    _ = try garden.apply(.undo, expectedRevision: garden.revision)
    let undone = receipt(garden)
    let restored = receipt(try JSONDecoder().decode(HabitatGarden.self, from: saved))
    XCTAssertNotEqual(planted, undone)
    XCTAssertEqual(restored, planted)
    XCTAssertEqual(undone.garden?.patches.count, 0)
    XCTAssertEqual(restored.garden?.patches.first?.planting, .grove)
    for candidate in [planted, undone, restored] {
      XCTAssertEqual(SanctuaryGardenSurfaceSource(garden: candidate.garden),
        SanctuaryGardenSurfaceSource(garden: nil))
      XCTAssertTrue(SanctuaryGardenSurfaceSource(garden: candidate.garden).coveredChunkKeys.isEmpty)
    }
  }

  func testGroundCoverMaskPreservesDecorationAndRestoresExactSourceIndices() throws {
    let roots: [SIMD3<Float>] = [.init(20, 1, 24), .init(20.6, 1, 24), .init(28, 1, 24)]
    var garden = HabitatGarden()
    let original = SanctuaryGroundCoverMask(garden: garden, construction: nil)
    func visible(_ mask: SanctuaryGroundCoverMask) -> [Int] {
      roots.indices.filter { !mask.excludes(root: roots[$0], height: 0.4, radius: 0.2) }
    }
    for planting in [HabitatGarden.Planting.flowers, .reeds, .grove] {
      _ = try garden.apply(.plant(planting, at: .init(x: 20, z: 24), radius: 2), expectedRevision: garden.revision)
    }
    XCTAssertEqual(SanctuaryGroundCoverMask(garden: garden, construction: nil), original)
    XCTAssertEqual(visible(original), [0, 1, 2])
    _ = try garden.apply(.plant(.shallowWater, at: .init(x: 20, z: 24), radius: 1), expectedRevision: garden.revision)
    let saved = try JSONEncoder().encode(garden)
    let wet = SanctuaryGroundCoverMask(garden: garden, construction: nil)
    XCTAssertEqual(visible(wet), [2])
    let reopened = try JSONDecoder().decode(HabitatGarden.self, from: saved)
    XCTAssertEqual(SanctuaryGroundCoverMask(garden: reopened, construction: nil), wet)
    XCTAssertEqual(visible(SanctuaryGroundCoverMask(garden: reopened, construction: nil)), [2])
    _ = try garden.apply(.undo, expectedRevision: garden.revision)
    XCTAssertEqual(SanctuaryGroundCoverMask(garden: garden, construction: nil), original)
    XCTAssertEqual(visible(SanctuaryGroundCoverMask(garden: garden, construction: nil)), [0, 1, 2])
  }

  func testGroundCoverSculptBranchesClearOnlySavedFootprintsWithoutMovingRoots() throws {
    var a = HabitatGarden(), b = HabitatGarden()
    _ = try a.apply(.sculpt(.raise, at: .init(x: 20, z: 24), radius: 1, amount: 1, targetHeight: nil), expectedRevision: 0)
    _ = try b.apply(.sculpt(.lower, at: .init(x: 28, z: 24), radius: 1, amount: 1, targetHeight: nil), expectedRevision: 0)
    XCTAssertEqual(a.revision, b.revision)
    let first = SanctuaryGroundCoverMask(garden: a, construction: nil)
    let second = SanctuaryGroundCoverMask(garden: b, construction: nil)
    XCTAssertNotEqual(first, second)
    XCTAssertTrue(first.excludes(root: .init(20, 1, 24), height: 0.4, radius: 0.2))
    XCTAssertFalse(second.excludes(root: .init(20, 1, 24), height: 0.4, radius: 0.2))
    XCTAssertTrue(second.excludes(root: .init(28, 1, 24), height: 0.4, radius: 0.2))
    // The blade extent is clipped at the edge too; roots are never resampled or shifted.
    XCTAssertTrue(first.excludes(root: .init(21.1, 1, 24), height: 0.4, radius: 0.2))
    XCTAssertFalse(first.excludes(root: .init(21.3, 1, 24), height: 0.4, radius: 0.2))
  }

  func testGroundCoverUsesRotatedConstructionAndActualVerticalSupport() throws {
    var construction = PersonalConstruction()
    _ = try construction.apply(.place(.path, at: .init(x: 20, y: 1, z: 24),
      yawRadians: .pi / 2, scale: 1), expectedRevision: 0)
    let mask = SanctuaryGroundCoverMask(garden: nil, construction: construction)
    XCTAssertTrue(mask.excludes(root: .init(20, 0.985, 25), height: 0.4, radius: 0.15))
    XCTAssertFalse(mask.excludes(root: .init(21, 0.985, 24), height: 0.4, radius: 0.15))
    XCTAssertFalse(mask.excludes(root: .init(20, -2, 24), height: 0.4, radius: 0.15))
    let restored = try JSONDecoder().decode(PersonalConstruction.self, from: JSONEncoder().encode(construction))
    XCTAssertEqual(SanctuaryGroundCoverMask(garden: nil, construction: restored), mask)
    _ = try construction.apply(.undo, expectedRevision: construction.revision)
    XCTAssertTrue(SanctuaryGroundCoverMask(garden: nil, construction: construction).isEmpty)
  }

  func testPlantedReedsUseOnlyPublishedConstructionAndRecoverExactIndicesOnMoveUndoReopen() throws {
    var garden = HabitatGarden()
    for planting in [HabitatGarden.Planting.shallowWater, .reeds] {
      _ = try garden.apply(.plant(planting, at: .init(x: 20, z: 24), radius: 5),
        expectedRevision: garden.revision)
    }
    let savedGarden = try JSONEncoder().encode(garden)
    let ids = garden.patches.map(\.id)
    let roots: [SIMD3<Float>] = [.init(20, 0.5, 24), .init(20, 0.5, 28),
      .init(22, 0.5, 24), .init(20, -2, 24), .init(20, 2, 24)]
    func visible(_ construction: PersonalConstruction) -> [Int] {
      // This is appendGarden's construction-only mask: authored water must not
      // hide reeds outside a deck, unlike the natural dry ground-cover mask.
      let mask = SanctuaryGroundCoverMask(garden: nil, construction: construction)
      return roots.indices.filter { !mask.excludes(root: roots[$0], height: 1.2, radius: 0.2) }
    }
    var construction = PersonalConstruction()
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    _ = try construction.apply(.place(.bridge, at: .init(x: 20, y: 1, z: 24),
      yawRadians: .pi / 2, scale: 1.5), expectedRevision: 0)
    let placement = try XCTUnwrap(construction.placements.first)
    XCTAssertEqual(visible(construction), [2, 3, 4])
    let reopened = try JSONDecoder().decode(PersonalConstruction.self,
      from: JSONEncoder().encode(construction))
    XCTAssertEqual(visible(reopened), [2, 3, 4])
    _ = try construction.apply(.update(placement.id, at: .init(x: 40, y: 1, z: 24),
      yawRadians: 0, scale: 0.5), expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    _ = try construction.apply(.undo, expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [2, 3, 4])
    _ = try construction.apply(.undo, expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    XCTAssertEqual(garden.patches.map(\.id), ids)
    XCTAssertEqual(try JSONDecoder().decode(HabitatGarden.self, from: savedGarden), garden)
  }

}
