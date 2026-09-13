import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class BuildingEditingTests: XCTestCase {
  private func world(root: URL? = nil) throws -> SanctuaryWorld {
    let world = try SanctuaryWorld(root: root, seed: 17)
    world.camera.position = SIMD3(2, world.groundHeight(2, 23) + 1.72, 23)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    return world
  }

  func testProductionControlsSelectTurnResizeMoveRemoveUndoAndReopen() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    _ = try sanctuary.control("build-bench")
    let id = try XCTUnwrap(sanctuary.controller.state.buildings.placements.first?.id)

    _ = try sanctuary.control("building-select")
    XCTAssertEqual(sanctuary.controller.state.buildings.selectedPlacementID, id)
    _ = try sanctuary.control("building-turn")
    XCTAssertEqual(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)).yawRadians,
      .pi / 4, accuracy: 0.0001)
    _ = try sanctuary.control("building-larger")
    _ = try sanctuary.control("building-resize")
    XCTAssertEqual(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)).scale,
      1.25, accuracy: 0.0001)

    sanctuary.camera.position = SIMD3(2, sanctuary.groundHeight(2, 30) + 1.72, 30)
    sanctuary.syncExpeditionPlayer()
    _ = try sanctuary.control("building-move")
    XCTAssertEqual(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)).location.z,
      25, accuracy: 0.0001)
    _ = try sanctuary.control("building-remove")
    XCTAssertNil(sanctuary.controller.state.buildings.placement(id: id))
    _ = try sanctuary.control("undoBuilding")
    XCTAssertEqual(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)).location.z,
      25, accuracy: 0.0001)

    try sanctuary.controller.save()
    let expected = sanctuary.controller.state
    let reopened = try SanctuaryWorld(root: root, seed: 17)
    XCTAssertEqual(reopened.controller.state, expected)
  }

  func testWaterSupportRejectsDryConstructionAndRecoversThroughBothAdapters() throws {
    let sanctuary = try SanctuaryWorld(seed: 17)
    let first = SIMD2<Float>(100, 100)
    func standBehind(_ target: SIMD2<Float>) {
      let z = target.y + sanctuary.controller.state.craftTools.reach
      sanctuary.camera.position = SIMD3(target.x, sanctuary.groundHeight(target.x, z) + 1.72, z)
      sanctuary.camera.yaw = 0
      sanctuary.syncExpeditionPlayer()
    }
    func waterTarget(_ point: SIMD2<Float>) -> PersonalConstruction.Location {
      .init(x: point.x, y: sanctuary.groundHeight(point.x, point.y), z: point.y)
    }
    func context(_ location: PersonalConstruction.Location) -> SanctuaryPlacementIntent.Context {
      .init(
        playerFeet: .init(
          x: sanctuary.camera.position.x, y: sanctuary.camera.position.y - 1.72,
          z: sanctuary.camera.position.z),
        reach: sanctuary.controller.state.craftTools.reach, eyeHeight: 1.72,
        targetIsObstructed: false,
        waterSurfaceHeight: sanctuary.constructionWaterSurfaceHeight(at: location))
    }

    _ = try sanctuary.controller.applyNature(
      .plant(.shallowWater, at: .init(x: first.x, z: first.y), radius: 3), expectedRevision: 0)
    standBehind(first)
    let firstBed = waterTarget(first)
    let firstWater = try XCTUnwrap(sanctuary.constructionWaterSurfaceHeight(at: firstBed))
    XCTAssertGreaterThan(firstWater, firstBed.y + 0.01)

    // Native intent preserves its draft and the full save on a dry-construction refusal.
    var nativeBench = SanctuaryPlacementIntent()
    nativeBench.beginPlace(.bench, at: firstBed, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    XCTAssertEqual(
      nativeBench.preview(in: sanctuary.controller.state.buildings, context: context(firstBed))?.rejection,
      .waterRequiresWalkway)
    let beforeNativeRefusal = try sanctuary.checkpoint()
    XCTAssertThrowsError(try nativeBench.confirm(
      in: sanctuary.controller.state.buildings, context: context(firstBed),
      commit: { command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision) }))
    XCTAssertEqual(try sanctuary.checkpoint(), beforeNativeRefusal)
    XCTAssertNotNil(nativeBench.draft)

    var nativeBridge = SanctuaryPlacementIntent()
    let firstBridgeSupport = sanctuary.constructionPlacementLocation(for: .bridge, at: firstBed)
    nativeBridge.beginPlace(.bridge, at: firstBridgeSupport, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    XCTAssertNil(nativeBridge.preview(
      in: sanctuary.controller.state.buildings, context: context(firstBridgeSupport))?.rejection)
    let nativeBridgeID = try nativeBridge.confirm(
      in: sanctuary.controller.state.buildings, context: context(firstBridgeSupport),
      commit: { command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision) })
    XCTAssertEqual(try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: nativeBridgeID)).location.y,
      firstWater, accuracy: 0.0001)

    // The retained immediate control path applies the identical water policy.
    let second = SIMD2<Float>(110, 100)
    _ = try sanctuary.controller.applyNature(
      .plant(.shallowWater, at: .init(x: second.x, z: second.y), radius: 3), expectedRevision: 1)
    standBehind(second)
    let beforeLegacyRefusal = try sanctuary.checkpoint()
    XCTAssertThrowsError(try sanctuary.control("build-bench"))
    XCTAssertEqual(try sanctuary.checkpoint(), beforeLegacyRefusal)
    _ = try sanctuary.control("build-bridge")
    let legacyBridge = try XCTUnwrap(sanctuary.controller.state.buildings.placements.last)
    XCTAssertEqual(legacyBridge.primitive, .bridge)
    XCTAssertEqual(legacyBridge.location.y,
      try XCTUnwrap(sanctuary.constructionWaterSurfaceHeight(at: legacyBridge.location)), accuracy: 0.0001)
  }

  private func blockSaves(in root: URL) throws {
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    try Data("not a directory".utf8).write(to: saves)
  }

  private func savedDocuments(in root: URL) throws -> [String: Data] {
    let saves = root.appendingPathComponent("saves")
    return try ["expedition.json", "expedition.previous.json"].reduce(into: [:]) { documents, name in
      let url = saves.appendingPathComponent(name)
      if FileManager.default.fileExists(atPath: url.path) {
        documents[name] = try Data(contentsOf: url)
      }
    }
  }

  private func persistedNextPlacementID(_ construction: PersonalConstruction) throws -> UInt64 {
    let document = try JSONSerialization.jsonObject(with: JSONEncoder().encode(construction))
    let value = try XCTUnwrap((document as? [String: Any])?["nextPlacementID"] as? NSNumber)
    return value.uint64Value
  }

  func testWaterBridgeMoveUndoAndFailedPersistencePreserveTheCommittedState() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root, seed: 17)
    let waterPoint = SIMD2<Float>(100, 100)
    let waterBed = PersonalConstruction.Location(
      x: waterPoint.x, y: world.groundHeight(waterPoint.x, waterPoint.y), z: waterPoint.y)
    _ = try world.controller.applyNature(
      .plant(.shallowWater, at: .init(x: waterPoint.x, z: waterPoint.y), radius: 4),
      expectedRevision: 0)
    let bridgeID = try world.commitPlacement(
      .place(.bridge, at: waterBed, yawRadians: 0, scale: 1), expectedRevision: 0)
    let waterBridge = try XCTUnwrap(world.controller.state.buildings.placement(id: bridgeID))
    XCTAssertEqual(waterBridge.location.y,
      try XCTUnwrap(world.constructionWaterSurfaceHeight(at: waterBed)), accuracy: 0.0001)

    let dryPoint = PersonalConstruction.Location(
      x: 104.1, y: world.groundHeight(104.1, 100), z: 100)
    _ = try world.commitPlacement(
      .update(bridgeID, at: dryPoint, yawRadians: waterBridge.yawRadians, scale: waterBridge.scale),
      expectedRevision: world.controller.state.buildings.revision)
    XCTAssertEqual(world.controller.state.buildings.placement(id: bridgeID)?.location, dryPoint)

    XCTAssertEqual(try world.commitPlacement(
      .undo, expectedRevision: world.controller.state.buildings.revision), bridgeID)
    XCTAssertEqual(world.controller.state.buildings.placement(id: bridgeID), waterBridge)

    let beforeFailedSave = try world.checkpoint()
    try blockSaves(in: root)
    XCTAssertThrowsError(try world.commitPlacement(
      .update(bridgeID, at: dryPoint, yawRadians: waterBridge.yawRadians, scale: waterBridge.scale),
      expectedRevision: world.controller.state.buildings.revision))
    XCTAssertEqual(try world.checkpoint(), beforeFailedSave)
  }

  func testFailedPlacementUndoPreservesRemovedMemoryAndDiskState() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    _ = try sanctuary.controller.applyNature(
      .plant(.flowers, at: .init(x: 100, z: 100), radius: 2), expectedRevision: 0)
    let id = try sanctuary.commitPlacement(
      .place(.bench, at: .init(x: 10, y: sanctuary.groundHeight(10, 20), z: 20),
        yawRadians: 0, scale: 1), expectedRevision: 0)
    _ = try sanctuary.commitPlacement(
      .remove(id), expectedRevision: sanctuary.controller.state.buildings.revision)

    let removedState = sanctuary.controller.state
    let removedCheckpoint = try sanctuary.checkpoint()
    let documents = try savedDocuments(in: root)
    XCTAssertFalse(documents.isEmpty)
    XCTAssertNil(removedState.buildings.placement(id: id))
    XCTAssertEqual(removedState.garden?.revision, 1)
    XCTAssertEqual(try persistedNextPlacementID(removedState.buildings), 2)
    XCTAssertEqual(removedState.buildings.history.count, 2)

    // `ExpeditionStore.save` writes atomically, so removing directory write
    // permission makes its temporary-write/replace fail while the current
    // primary and backup documents stay in place.
    let saves = root.appendingPathComponent("saves")
    let attributes = try FileManager.default.attributesOfItem(atPath: saves.path)
    let originalPermissions = try XCTUnwrap(attributes[.posixPermissions])
    try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: saves.path)
    var permissionsRestored = false
    defer {
      if !permissionsRestored {
        try? FileManager.default.setAttributes(
          [.posixPermissions: originalPermissions], ofItemAtPath: saves.path)
      }
    }
    XCTAssertThrowsError(try sanctuary.commitPlacement(
      .undo, expectedRevision: sanctuary.controller.state.buildings.revision))
    XCTAssertEqual(try sanctuary.checkpoint(), removedCheckpoint)
    XCTAssertEqual(sanctuary.controller.state, removedState)
    XCTAssertNil(sanctuary.controller.state.buildings.placement(id: id))
    XCTAssertEqual(try persistedNextPlacementID(sanctuary.controller.state.buildings), 2)
    XCTAssertEqual(sanctuary.controller.state.buildings.history.count, 2)
    XCTAssertEqual(sanctuary.controller.state.garden, removedState.garden)
    XCTAssertEqual(try savedDocuments(in: root), documents)

    try FileManager.default.setAttributes(
      [.posixPermissions: originalPermissions], ofItemAtPath: saves.path)
    permissionsRestored = true
    let reopened = try SanctuaryWorld(root: root, seed: 17)
    XCTAssertEqual(reopened.controller.state, removedState)
    XCTAssertNil(reopened.controller.state.buildings.placement(id: id))
    XCTAssertEqual(try persistedNextPlacementID(reopened.controller.state.buildings), 2)
    XCTAssertEqual(reopened.controller.state.buildings.history.count, 2)
    XCTAssertEqual(reopened.controller.state.garden, removedState.garden)
  }

  func testHistoricalWetBenchLoadsUnchangedAndNewWaterMutationsRemainAtomic() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let coast = SIMD2<Float>(11_728.999, -7_182.9937)
    let historicalLocation = PersonalConstruction.Location(
      x: coast.x, y: 5.7485833, z: coast.y)
    let world = try SanctuaryWorld(root: root, seed: 17)

    var historical = PersonalConstruction()
    let id = try historical.apply(.place(.bench, at: historicalLocation, yawRadians: 0, scale: 1),
      expectedRevision: 0)
    _ = try historical.apply(.select(id), expectedRevision: 1)
    _ = try historical.apply(.update(id, at: historicalLocation, yawRadians: .pi / 4, scale: 1),
      expectedRevision: 2)
    _ = try historical.apply(.update(id, at: historicalLocation, yawRadians: .pi / 4, scale: 1.25),
      expectedRevision: 3)
    _ = try historical.apply(.deselect, expectedRevision: 4)
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: coast.x, z: coast.y), radius: 4),
        expectedRevision: 0)
      _ = try state.applyNature(
        .plant(.reeds, at: .init(x: coast.x, z: coast.y), radius: 3),
        expectedRevision: 1)
      state.construction = historical
    }
    let expectedConstruction = world.controller.state.buildings
    let expectedPlacement = try XCTUnwrap(expectedConstruction.placement(id: id))
    let expectedHistory = expectedConstruction.history

    // Decoding preserves the established wet bench exactly; there is no load-time
    // migration, deletion, elevation rewrite, or history rewrite.
    let reopened = try SanctuaryWorld(root: root, seed: 17)
    XCTAssertEqual(reopened.controller.state.buildings, expectedConstruction)
    XCTAssertEqual(reopened.controller.state.buildings.placement(id: id), expectedPlacement)
    XCTAssertEqual(reopened.controller.state.buildings.history, expectedHistory)

    let beforeRejectedMutation = try reopened.checkpoint()
    XCTAssertThrowsError(try reopened.commitPlacement(
      .update(id, at: expectedPlacement.location, yawRadians: 0, scale: expectedPlacement.scale),
      expectedRevision: reopened.controller.state.buildings.revision))
    XCTAssertEqual(try reopened.checkpoint(), beforeRejectedMutation)

    let water = try XCTUnwrap(reopened.constructionWaterSurfaceHeight(at: historicalLocation))
    XCTAssertGreaterThan(water, historicalLocation.y + 0.01)
    let bridgeID = try reopened.commitPlacement(
      .place(.bridge, at: historicalLocation, yawRadians: 0, scale: 1),
      expectedRevision: reopened.controller.state.buildings.revision)
    XCTAssertEqual(try XCTUnwrap(reopened.controller.state.buildings.placement(id: bridgeID)).location.y,
      water, accuracy: 0.0001)
  }

  func testMissingSelectionAndFailedSelectionSavePreserveProductionCheckpoint() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    let before = try sanctuary.checkpoint()
    XCTAssertThrowsError(try sanctuary.control("building-move"))
    XCTAssertEqual(try sanctuary.checkpoint(), before)

    _ = try sanctuary.control("build-bench")
    let built = try sanctuary.checkpoint()
    try FileManager.default.removeItem(at: root.appendingPathComponent("saves"))
    try Data("not a directory".utf8).write(to: root.appendingPathComponent("saves"))
    XCTAssertThrowsError(try sanctuary.control("building-select"))
    XCTAssertEqual(try sanctuary.checkpoint(), built)
  }

  func testProductionAnimalOccupancyRejectsSunhareAndAllowsClearBenchMove() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    let actor = try XCTUnwrap(sanctuary.controller.state.population.actor(id: "sunhare-001"))
    let blockedLocation = PersonalConstruction.Location(
      x: actor.position.x, y: sanctuary.groundHeight(actor.position.x, actor.position.y), z: actor.position.y)
    let blockedContext = SanctuaryPlacementIntent.Context(
      playerFeet: .init(x: 2, y: sanctuary.camera.position.y - 1.72, z: 23),
      reach: sanctuary.controller.state.craftTools.reach, eyeHeight: 1.72, targetIsObstructed: false,
      occupants: sanctuary.placementOccupants(near: blockedLocation))
    XCTAssertTrue(blockedContext.occupants.contains { $0.id == actor.id })
    var blocked = SanctuaryPlacementIntent()
    blocked.beginPlace(.bench, at: blockedLocation, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    XCTAssertEqual(
      blocked.preview(in: sanctuary.controller.state.buildings, context: blockedContext)?.rejection,
      .occupiedAnimal)
    let before = try sanctuary.checkpoint()
    XCTAssertThrowsError(try blocked.confirm(in: sanctuary.controller.state.buildings, context: blockedContext) {
      command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision)
    })
    XCTAssertEqual(try sanctuary.checkpoint(), before)

    // A candidate that was previewed before the actor arrived must also fail
    // in the save-backed construction transaction. This intentionally supplies
    // stale empty occupancy facts to prove confirmation does not trust them.
    var stalePreview = SanctuaryPlacementIntent()
    stalePreview.beginPlace(.bench, at: blockedLocation, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    let emptyOccupants = SanctuaryPlacementIntent.Context(
      playerFeet: blockedContext.playerFeet, reach: sanctuary.controller.state.craftTools.reach,
      eyeHeight: 1.72, targetIsObstructed: false)
    XCTAssertNil(stalePreview.preview(in: sanctuary.controller.state.buildings, context: emptyOccupants)?.rejection)
    XCTAssertThrowsError(try stalePreview.confirm(
      in: sanctuary.controller.state.buildings, context: emptyOccupants,
      commit: { command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision) }))
    XCTAssertEqual(try sanctuary.checkpoint(), before)
    XCTAssertNotNil(stalePreview.draft)

    let clearLocation = PersonalConstruction.Location(
      x: actor.position.x, y: sanctuary.groundHeight(actor.position.x, actor.position.y + 1.5),
      z: actor.position.y + 1.5)
    let clearContext = SanctuaryPlacementIntent.Context(
      playerFeet: blockedContext.playerFeet, reach: sanctuary.controller.state.craftTools.reach,
      eyeHeight: 1.72, targetIsObstructed: false,
      occupants: sanctuary.placementOccupants(near: clearLocation))
    var clear = SanctuaryPlacementIntent()
    clear.beginPlace(.bench, at: clearLocation, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    let id = try clear.confirm(in: sanctuary.controller.state.buildings, context: clearContext) {
      command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision)
    }
    var blockedMove = SanctuaryPlacementIntent()
    blockedMove.beginEdit(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)),
      constructionRevision: sanctuary.controller.state.buildings.revision)
    blockedMove.setAim(blockedLocation)
    XCTAssertEqual(
      blockedMove.preview(in: sanctuary.controller.state.buildings, context: blockedContext)?.rejection,
      .occupiedAnimal)
    let beforeBlockedMove = try sanctuary.checkpoint()
    XCTAssertThrowsError(try blockedMove.confirm(in: sanctuary.controller.state.buildings, context: blockedContext) {
      command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision)
    })
    XCTAssertEqual(try sanctuary.checkpoint(), beforeBlockedMove)
    XCTAssertEqual(sanctuary.controller.state.buildings.placement(id: id)?.location, clearLocation)

    let movedLocation = PersonalConstruction.Location(
      x: actor.position.x, y: sanctuary.groundHeight(actor.position.x, actor.position.y + 2.5),
      z: actor.position.y + 2.5)
    let movedContext = SanctuaryPlacementIntent.Context(
      playerFeet: blockedContext.playerFeet, reach: sanctuary.controller.state.craftTools.reach,
      eyeHeight: 1.72, targetIsObstructed: false,
      occupants: sanctuary.placementOccupants(near: movedLocation))
    var moved = SanctuaryPlacementIntent()
    moved.beginEdit(
      try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id)),
      constructionRevision: sanctuary.controller.state.buildings.revision)
    moved.setAim(movedLocation)
    XCTAssertEqual(try moved.confirm(in: sanctuary.controller.state.buildings, context: movedContext) {
      command, revision in try sanctuary.commitPlacement(command, expectedRevision: revision)
    }, id)
    XCTAssertEqual(sanctuary.controller.state.buildings.placement(id: id)?.location, movedLocation)
  }

  func testProductionHoveringOccupantAllowsBridgeBelowItsBody() throws {
    let sanctuary = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(
      sanctuary.controller.state.population.actor(id: "canopy-glider-001"))
    let location = PersonalConstruction.Location(
      x: actor.position.x, y: sanctuary.groundHeight(actor.position.x, actor.position.y), z: actor.position.y)
    sanctuary.camera.position = SIMD3(location.x, location.y + 1.72, location.z)
    sanctuary.syncExpeditionPlayer()
    let context = SanctuaryPlacementIntent.Context(
      playerFeet: .init(x: location.x, y: location.y, z: location.z),
      reach: sanctuary.controller.state.craftTools.reach, eyeHeight: 1.72, targetIsObstructed: false,
      occupants: sanctuary.placementOccupants(near: location))
    XCTAssertTrue(context.occupants.contains { $0.id == actor.id && $0.bottom > location.y + 1 })
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.bridge, at: location, yawRadians: 0, scale: 1,
      constructionRevision: sanctuary.controller.state.buildings.revision)
    XCTAssertNil(intent.preview(in: sanctuary.controller.state.buildings, context: context)?.rejection)
    let placementID = try intent.confirm(
      in: sanctuary.controller.state.buildings, context: context,
      commit: { command, expectedRevision in
        try sanctuary.commitPlacement(command, expectedRevision: expectedRevision)
      })
    XCTAssertEqual(sanctuary.controller.state.buildings.placement(id: placementID)?.primitive, .bridge)
  }

  func testFinishEditingClearsSelectionAndLeavesFollowingTurnAsAToolSetting() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    _ = try sanctuary.control("build-bench")
    let id = try XCTUnwrap(sanctuary.controller.state.buildings.placements.first?.id)
    let before = try XCTUnwrap(sanctuary.controller.state.buildings.placement(id: id))
    _ = try sanctuary.control("building-select")
    XCTAssertEqual(sanctuary.controller.state.buildings.selectedPlacementID, id)
    _ = try sanctuary.control("building-finish")
    XCTAssertNil(sanctuary.controller.state.buildings.selectedPlacementID)
    XCTAssertEqual(sanctuary.controller.state.buildings.placement(id: id), before)

    _ = try sanctuary.control("building-turn")
    XCTAssertEqual(sanctuary.controller.state.buildings.placement(id: id), before)
    XCTAssertEqual(sanctuary.controller.state.craftTools.buildingRotation, .pi / 4, accuracy: 0.0001)
    try sanctuary.controller.save()
    let reopened = try SanctuaryWorld(root: root, seed: 17)
    XCTAssertNil(reopened.controller.state.buildings.selectedPlacementID)
  }

  func testFailedFinishSavePreservesSelectionAndPlacement() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let sanctuary = try world(root: root)
    _ = try sanctuary.control("build-bench")
    _ = try sanctuary.control("building-select")
    let before = try sanctuary.checkpoint()
    try FileManager.default.removeItem(at: root.appendingPathComponent("saves"))
    try Data("not a directory".utf8).write(to: root.appendingPathComponent("saves"))
    XCTAssertThrowsError(try sanctuary.control("building-finish"))
    XCTAssertEqual(try sanctuary.checkpoint(), before)
  }
}
