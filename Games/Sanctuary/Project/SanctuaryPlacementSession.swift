import Foundation
import SanctuaryContent

/// Native-only orchestration for player construction previews. The session is
/// intentionally absent from `Expedition`: saving, loading, or cancelling can
/// never serialize an unfinished placement.
final class SanctuaryPlacementSession {
  struct Aim: Equatable {
    let location: PersonalConstruction.Location
    /// Range exhaustion is distinct from a ray obstruction: it can become
    /// valid simply by increasing the current construction reach.
    let targetIsOutOfReach: Bool
    let targetIsObstructed: Bool
    let distance: Float
    let supportRevision: UInt64
  }

  private enum Mode: Equatable { case placing, editing, moving }
  private struct PreviewKey: Equatable {
    let draft: SanctuaryPlacementIntent.Draft?
    let mode: Mode?
    let aim: Aim
    let constructionRevision: UInt64
    let context: SanctuaryPlacementIntent.Context
  }

  private(set) var intent = SanctuaryPlacementIntent()
  private var mode: Mode?
  private var cachedKey: PreviewKey?
  private var cachedPreview: SanctuaryPlacementIntent.Preview?

  var isActive: Bool { intent.draft != nil }
  var selectedPlacementID: PersonalConstruction.PlacementID? { intent.selectedPlacementID }
  var currentPreview: SanctuaryPlacementIntent.Preview? { cachedPreview }
  /// A fixed edit keeps surveying the existing draft transform. Moving and new
  /// placements instead follow the camera target.
  var requiresFixedSupportTarget: Bool { mode == .editing }

  func reset() {
    intent.cancel()
    mode = nil
    cachedKey = nil
    cachedPreview = nil
  }

  func beginPlace(
    _ primitive: PersonalConstruction.Primitive, in world: SanctuaryWorld, aim: Aim
  ) {
    world.syncExpeditionPlayer()
    let tools = world.controller.state.craftTools
    let yaw = normalized(world.camera.yaw + tools.buildingRotation)
    intent.beginPlace(
      primitive, at: aim.location, yawRadians: yaw, scale: tools.buildingScale,
      constructionRevision: world.controller.state.buildings.revision)
    mode = .placing
    invalidate()
  }

  /// Selecting is read-only until confirmation. The original transform remains
  /// in the draft so a player can rotate or resize before choosing Move.
  func select(in world: SanctuaryWorld) throws {
    guard let placement = world.placementSelectionCandidate() else {
      throw PersonalConstructionError.noReachablePlacement
    }
    intent.beginEdit(placement, constructionRevision: world.controller.state.buildings.revision)
    mode = .editing
    invalidate()
  }

  func chooseMove() throws {
    guard intent.draft != nil, (mode == .editing || mode == .moving) else {
      throw PersonalConstructionError.noSelectedPlacement
    }
    mode = .moving
    invalidate()
  }

  func turn45() throws {
    guard intent.draft != nil else { throw PersonalConstructionError.noSelectedPlacement }
    intent.turn45()
    invalidate()
  }

  func changeScale(by delta: Float) throws {
    guard let draft = intent.draft else { throw PersonalConstructionError.noSelectedPlacement }
    intent.setScale(min(PersonalConstruction.maximumScale, max(PersonalConstruction.minimumScale, draft.scale + delta)))
    invalidate()
  }

  func preview(in world: SanctuaryWorld, aim: Aim) -> SanctuaryPlacementIntent.Preview? {
    guard intent.draft != nil else { return nil }
    if mode == .placing || mode == .moving { setAimFromSupport(in: world, aim: aim) }
    let context = placementContext(in: world, aim: aim)
    let key = PreviewKey(
      draft: intent.draft, mode: mode, aim: aim,
      constructionRevision: world.controller.state.buildings.revision, context: context)
    if key == cachedKey { return cachedPreview }
    let preview = intent.preview(in: world.controller.state.buildings, context: context)
    cachedKey = key
    cachedPreview = preview
    return preview
  }

  @discardableResult
  func confirm(in world: SanctuaryWorld, aim: Aim) throws -> PersonalConstruction.PlacementID {
    // E can arrive after a camera update but before the next render preview.
    // Match the moving/new preview transform here; a fixed edit deliberately
    // retains its selected transform until the player chooses Move.
    if mode == .placing || mode == .moving {
      setAimFromSupport(in: world, aim: aim)
      invalidate()
    }
    let context = placementContext(in: world, aim: aim)
    let placementID = try intent.confirm(
      in: world.controller.state.buildings, context: context,
      commit: { command, revision in try world.commitPlacement(command, expectedRevision: revision) })
    mode = nil
    invalidate()
    return placementID
  }

  func cancel() { reset() }

  /// Reverses the latest persisted construction operation through the same
  /// candidate transaction as a placement confirmation. A transient ghost is
  /// never part of history, so discard it before undoing the durable layout.
  @discardableResult
  func undo(in world: SanctuaryWorld) throws -> PersonalConstruction.PlacementID {
    reset()
    return try world.commitPlacement(.undo, expectedRevision: world.controller.state.buildings.revision)
  }

  @discardableResult
  func remove(in world: SanctuaryWorld) throws -> PersonalConstruction.PlacementID {
    guard let placementID = intent.selectedPlacementID, let draft = intent.draft else {
      throw PersonalConstructionError.noSelectedPlacement
    }
    let removed = try world.commitPlacement(
      .remove(placementID), expectedRevision: draft.expectedConstructionRevision)
    reset()
    return removed
  }

  private func setAimFromSupport(in world: SanctuaryWorld, aim: Aim) {
    guard let draft = intent.draft else { return }
    intent.setAim(world.constructionPlacementLocation(for: draft.primitive, at: aim.location))
  }

  private func placementContext(
    in world: SanctuaryWorld, aim: Aim
  ) -> SanctuaryPlacementIntent.Context {
    let state = world.controller.state
    let eye: Float = state.travel.mode == .riding ? 2.5 : 1.72
    let feet = (state.playerElevation ?? world.camera.position.y) - eye
    let draft = intent.draft
    let occupants = draft.map { world.placementOccupants(near: $0.location) } ?? []
    let waterSurfaceHeight = draft.flatMap { world.constructionWaterSurfaceHeight(at: $0.location) }
    return .init(
      playerFeet: .init(x: state.player.x, y: feet, z: state.player.y),
      reach: state.craftTools.reach, eyeHeight: eye,
      targetIsOutOfReach: aim.targetIsOutOfReach, targetIsObstructed: aim.targetIsObstructed,
      waterSurfaceHeight: waterSurfaceHeight, occupants: occupants)
  }

  private func invalidate() {
    cachedKey = nil
    cachedPreview = nil
  }

  private func normalized(_ angle: Float) -> Float { atan2(sin(angle), cos(angle)) }
}
