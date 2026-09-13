import FieldCore
import Foundation
import XCTest

@testable import SanctuaryContent

final class SanctuaryPlacementIntentTests: XCTestCase {
  private enum SaveFailure: Error { case unavailable }
  private func location(_ x: Float, _ y: Float, _ z: Float) -> PersonalConstruction.Location {
    .init(x: x, y: y, z: z)
  }

  private func context(
    feet: PersonalConstruction.Location = .init(x: 0, y: 0, z: 0), reach: Float = 5,
    eyeHeight: Float = 1.72, outOfReach: Bool = false, obstructed: Bool = false
  ) -> SanctuaryPlacementIntent.Context {
    .init(
      playerFeet: feet, reach: reach, eyeHeight: eyeHeight,
      targetIsOutOfReach: outOfReach, targetIsObstructed: obstructed)
  }

  func testRejectedPreviewRetainsTheIntendedGhostAndCancelDoesNotMutateConstruction() {
    var construction = PersonalConstruction()
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)

    let preview = intent.preview(in: construction, context: context(obstructed: true))
    XCTAssertEqual(preview?.placement.primitive, .bench)
    XCTAssertEqual(preview?.placement.location, location(3, 0, 0))
    XCTAssertEqual(preview?.rejection, .targetObstructed)
    XCTAssertEqual(construction, PersonalConstruction())

    intent.cancel()
    XCTAssertNil(intent.preview(in: construction, context: context()))
    XCTAssertEqual(construction, PersonalConstruction())
  }

  func testPreviewUsesConstructionCollisionValidationWithoutMutatingIt() throws {
    var construction = PersonalConstruction()
    _ = try construction.apply(.place(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision)
    let before = construction
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)

    let preview = intent.preview(in: construction, context: context())
    XCTAssertEqual(preview?.placement.location, location(3, 0, 0))
    XCTAssertEqual(preview?.rejection, .construction(.overlappingSolid))
    XCTAssertEqual(construction, before)
  }

  func testConfirmCallsTheAtomicCommitOnceThenClearsTheDraft() throws {
    var construction = PersonalConstruction()
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.path, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)
    var commitCount = 0

    let id = try intent.confirm(in: construction, context: context()) { command, revision in
      commitCount += 1
      return try construction.apply(command, expectedRevision: revision)
    }
    XCTAssertEqual(commitCount, 1)
    XCTAssertEqual(construction.placement(id: id)?.primitive, .path)
    XCTAssertNil(intent.draft)
    XCTAssertThrowsError(try intent.confirm(in: construction, context: context()) { _, _ in
      commitCount += 1
      return 0
    })
    XCTAssertEqual(commitCount, 1)
  }

  func testStaleAndFailedCommitKeepTheDraftAndPersistedConstructionUnchanged() throws {
    var construction = PersonalConstruction()
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)
    _ = try construction.apply(.place(.path, at: location(-3, 0, 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision)
    XCTAssertEqual(intent.preview(in: construction, context: context())?.rejection, .staleConstruction)
    let stale = construction
    XCTAssertThrowsError(try intent.confirm(in: construction, context: context()) { command, revision in
      try construction.apply(command, expectedRevision: revision)
    })
    XCTAssertEqual(construction, stale)
    XCTAssertNotNil(intent.draft)

    var retry = SanctuaryPlacementIntent()
    retry.beginPlace(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)
    XCTAssertThrowsError(try retry.confirm(in: construction, context: context()) { _, _ in
      throw SaveFailure.unavailable
    })
    XCTAssertEqual(construction, stale)
    XCTAssertNotNil(retry.draft)
  }

  func testEditDraftKeepsPlacementIdentityAndRejectsOutOfReachAim() throws {
    var construction = PersonalConstruction()
    let id = try construction.apply(.place(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision)
    var intent = SanctuaryPlacementIntent()
    intent.beginEdit(try XCTUnwrap(construction.placement(id: id)), constructionRevision: construction.revision)
    intent.setAim(location(7, 0, 0))
    XCTAssertEqual(intent.preview(in: construction, context: context())?.rejection, .targetOutOfReach)
    intent.setAim(location(4, 0, 0))
    let movedID = try intent.confirm(in: construction, context: context()) { command, revision in
      try construction.apply(command, expectedRevision: revision)
    }
    XCTAssertEqual(movedID, id)
    XCTAssertEqual(construction.placement(id: id)?.location, location(4, 0, 0))
  }

  func testPreviewRejectsAPlacementThatWouldTrapThePlayer() {
    let construction = PersonalConstruction()
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.lantern, at: location(0, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)

    XCTAssertEqual(intent.preview(in: construction, context: context())?.rejection, .wouldTrapPlayer)
  }

  func testRangeLimitPrecedesSolidObstructionWithoutMutatingTheDraft() throws {
    let world = try SanctuaryWorld(seed: 17)
    let open = SIMD2<Float>(10_000, 10_000)
    let range = world.target(
      origin: V3(open.x, world.groundHeight(open.x, open.y) + 50, open.y),
      direction: V3(0, 1, 0), maxReach: 4)
    XCTAssertEqual(range.obstruction, .rangeLimit)
    XCTAssertNil(range.support)

    let wall = try XCTUnwrap(SanctuaryHome.construction.collisionFacts.first { $0.kind == .solid })
    let c = cos(wall.yawRadians), s = sin(wall.yawRadians)
    let outward = V3(s, 0, c)
    let solid = world.target(
      origin: V3(wall.center.x, (wall.bottom + wall.top) * 0.5, wall.center.z)
        + outward * (wall.halfExtents.z + 1), direction: -outward, maxReach: 4)
    XCTAssertEqual(solid.obstruction, .solid)
    XCTAssertNil(solid.support)

    var construction = PersonalConstruction()
    var intent = SanctuaryPlacementIntent()
    intent.beginPlace(.bench, at: location(3, 0, 0), yawRadians: 0, scale: 1,
      constructionRevision: construction.revision)
    let before = construction
    XCTAssertEqual(
      intent.preview(in: construction, context: context(outOfReach: true, obstructed: true))?.rejection,
      .targetOutOfReach)
    XCTAssertThrowsError(try intent.confirm(
      in: construction, context: context(outOfReach: true, obstructed: true),
      commit: { command, revision in try construction.apply(command, expectedRevision: revision) }))
    XCTAssertEqual(construction, before)
    XCTAssertNotNil(intent.draft)
  }
}
