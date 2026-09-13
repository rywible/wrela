import FieldCore
import Foundation
import XCTest

@testable import SanctuaryContent

final class SanctuaryNatureIntentTests: XCTestCase {
  private func origin(_ world: SanctuaryWorld, x: Float = 10, z: Float = 18) -> V3 {
    V3(x, world.groundHeight(x, z) + 4, z)
  }

  private func prepare(
    _ action: SanctuaryNatureIntent.Action, in world: SanctuaryWorld, x: Float = 10, z: Float = 18
  ) -> SanctuaryNatureIntent {
    world.prepareNatureIntent(action, origin: origin(world, x: x, z: z), direction: V3(0, -1, 0), maxReach: 5)
  }

  private func blockSaves(in root: URL) throws {
    // A new SanctuaryWorld has not necessarily saved yet, so its supplied root
    // may not exist. Create the parent before installing the deterministic
    // file-instead-of-directory failure fixture.
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    try Data("not a directory".utf8).write(to: saves)
  }

  func testSupportedGroundPreparesAndCastsTheProductionGardenCommand() throws {
    let world = try SanctuaryWorld(seed: 17)
    let intent = prepare(.plant(.flowers), in: world)
    XCTAssertEqual(intent.radius, world.controller.state.craftTools.brushRadius)
    XCTAssertNil(world.controller.state.garden)
    XCTAssertTrue(world.validateNatureIntent(intent).isValid)

    let id = try world.castNatureIntent(intent)
    XCTAssertEqual(world.controller.state.garden?.patches.first?.id, id)
    XCTAssertEqual(world.controller.state.garden?.patches.first?.planting, .flowers)
  }

  func testSmoothCapturesComposedTargetHeightAndStaleGardenDoesNotMutate() throws {
    let world = try SanctuaryWorld(seed: 17)
    _ = try world.controller.applyNature(
      .sculpt(.raise, at: .init(x: 10, z: 18), radius: 3, amount: 2, targetHeight: nil),
      expectedRevision: 0)
    let intent = prepare(.sculpt(.smooth), in: world)
    XCTAssertEqual(try XCTUnwrap(intent.terrainTargetHeight), world.groundHeight(10, 18), accuracy: 0.0001)
    _ = try world.controller.applyNature(.undo, expectedRevision: 1)
    let before = try world.checkpoint()
    XCTAssertEqual(world.validateNatureIntent(intent).rejection, .staleGarden)
    XCTAssertThrowsError(try world.castNatureIntent(intent))
    XCTAssertEqual(try world.checkpoint(), before)
  }

  func testSmoothCapturesSavedHeightWhileRegionalSupportPublicationLags() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.enableHostCommittedRegionalCollision(
      initialLayout: world.world, garden: nil, boulders: world.controller.state.boulders)
    _ = try world.controller.applyNature(
      .sculpt(.raise, at: .init(x: 10, z: 18), radius: 3, amount: 2, targetHeight: nil),
      expectedRevision: 0)

    // The current native support is intentionally still the prior publication.
    XCTAssertNil(world.presentationGarden)
    let smooth = prepare(.sculpt(.smooth), in: world)
    let savedGarden = try XCTUnwrap(world.controller.state.garden)
    let savedHeight = savedGarden.surfaceHeight(
      baseHeight: world.world.terrain.height(10, 18), at: .init(x: 10, z: 18))
    XCTAssertEqual(try XCTUnwrap(smooth.terrainTargetHeight), savedHeight, accuracy: 0.0001)
    XCTAssertGreaterThan(abs(savedHeight - world.groundHeight(10, 18)), 0.0001)
  }

  func testProductionBrushRadiusExtremesRemainWithinGardenBounds() throws {
    let world = try SanctuaryWorld(seed: 17)
    for _ in 0..<20 { _ = try world.control("brush-smaller") }
    XCTAssertEqual(world.controller.state.craftTools.brushRadius, 1)
    XCTAssertEqual(prepare(.plant(.flowers), in: world).radius, 1)
    XCTAssertEqual(prepare(.plant(.grove), in: world).radius, 2)
    XCTAssertEqual(prepare(.sculpt(.raise), in: world).radius, 2)

    for _ in 0..<20 { _ = try world.control("brush-larger") }
    XCTAssertEqual(world.controller.state.craftTools.brushRadius, HabitatGarden.maximumRadius)
    XCTAssertEqual(prepare(.plant(.grove), in: world).radius, HabitatGarden.maximumRadius)
    XCTAssertEqual(prepare(.sculpt(.lower), in: world).radius, HabitatGarden.maximumRadius)
  }

  func testWaterPolicyAndStructureTargetsPreserveState() throws {
    let waterWorld = try SanctuaryWorld(seed: 17)
    _ = try waterWorld.controller.applyNature(
      .plant(.shallowWater, at: .init(x: 10, z: 18), radius: 3), expectedRevision: 0)
    let water = prepare(.plant(.grove), in: waterWorld)
    let waterBefore = try waterWorld.checkpoint()
    XCTAssertEqual(waterWorld.validateNatureIntent(water).rejection, .water)
    XCTAssertThrowsError(try waterWorld.castNatureIntent(water))
    XCTAssertEqual(try waterWorld.checkpoint(), waterBefore)

    // Reeds are the deliberate wet-bank companion to invited shallow water.
    // The native transient brush must match the existing direct control path.
    let reeds = prepare(.plant(.reeds), in: waterWorld)
    XCTAssertTrue(waterWorld.validateNatureIntent(reeds).isValid)
    let reedsID = try waterWorld.castNatureIntent(reeds)
    XCTAssertEqual(waterWorld.controller.state.garden?.patches.map(\.id), [1, reedsID])
    XCTAssertEqual(waterWorld.controller.state.garden?.patches.last?.planting, .reeds)

    // The native host may publish water support after its saved garden. The
    // transient ray remains on the prior ground until publication, but a cast
    // must already reject that saved water target.
    let publishingWaterWorld = try SanctuaryWorld(seed: 17)
    publishingWaterWorld.enableHostCommittedRegionalCollision(
      initialLayout: publishingWaterWorld.world, garden: nil,
      boulders: publishingWaterWorld.controller.state.boulders)
    _ = try publishingWaterWorld.controller.applyNature(
      .plant(.shallowWater, at: .init(x: 10, z: 18), radius: 3), expectedRevision: 0)
    XCTAssertNil(publishingWaterWorld.presentationGarden)
    let beforePublication = try publishingWaterWorld.checkpoint()
    let delayedWater = prepare(.plant(.grove), in: publishingWaterWorld)
    XCTAssertEqual(publishingWaterWorld.validateNatureIntent(delayedWater).rejection, .water)
    XCTAssertThrowsError(try publishingWaterWorld.castNatureIntent(delayedWater))
    XCTAssertEqual(try publishingWaterWorld.checkpoint(), beforePublication)

    let structureWorld = try SanctuaryWorld(seed: 17)
    let cabinOrigin = origin(structureWorld, x: 0, z: 26) + V3(0, 2, 0)
    let structure = structureWorld.prepareNatureIntent(
      .plant(.flowers), origin: cabinOrigin, direction: V3(0, -1, 0), maxReach: 5)
    XCTAssertEqual(structureWorld.validateNatureIntent(structure).rejection, .structure)
  }

  func testFailedPersistenceAndUncastIntentPreservePreviouslySavedGarden() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root, seed: 17)
    let initial = prepare(.plant(.flowers), in: world)
    let untouched = try world.checkpoint()
    XCTAssertNil(world.controller.state.garden)
    XCTAssertEqual(try world.checkpoint(), untouched)

    _ = try world.castNatureIntent(initial)
    let saved = try world.checkpoint()
    let intent = prepare(.plant(.reeds), in: world, x: 12, z: 18)

    try blockSaves(in: root)
    XCTAssertThrowsError(try world.castNatureIntent(intent))
    XCTAssertEqual(try world.checkpoint(), saved)
  }
}
