import FieldCore
import Foundation
import SanctuaryContent

/// Native-only state for a nature brush. The saved expedition receives a
/// command only after `confirm`; aiming and cancellation are deliberately
/// transient.
final class SanctuaryNatureSession {
  struct Preview: Equatable {
    /// The complete source command candidate. Renderers may inspect its action
    /// and strength, but must not mutate or serialize it.
    let currentDraft: SanctuaryNatureIntent
    /// The authoritative target query, retained even for an invalid preview.
    let target: SanctuaryTargetingResult
    let targetPoint: V3
    let radius: Float
    let valid: Bool
    let rejection: SanctuaryNatureIntent.Rejection?
  }

  /// Facts from the last successful save-backed cast. This is separate from a
  /// draft so a rejected follow-up cast never impersonates a committed event.
  struct CommittedOutcome: Equatable {
    let sourceEventID: HabitatGarden.PatchID
    let playSeconds: Double
  }

  private struct AimKey: Equatable {
    let action: SanctuaryNatureIntent.Action
    let position: V3
    let yaw: Float
    let pitch: Float
    let gardenRevision: UInt64
    let constructionRevision: UInt64
    let ecologyRevision: UInt64
    let publishedSupportRevision: UInt64
    let tools: SanctuaryToolSettings
  }

  private var action: SanctuaryNatureIntent.Action?
  private var aimKey: AimKey?
  private(set) var currentDraft: SanctuaryNatureIntent?
  private(set) var currentPreview: Preview?
  private(set) var lastCommittedOutcome: CommittedOutcome?

  var isActive: Bool { action != nil }
  var actionTitle: String {
    guard let action else { return "Nature" }
    switch action {
    case .plant(.flowers): return "Flowers"
    case .plant(.grove): return "Grove"
    case .plant(.reeds): return "Reeds"
    case .plant(.shallowWater): return "Shallow water"
    case .sculpt(.raise): return "Raise earth"
    case .sculpt(.lower): return "Lower earth"
    case .sculpt(.smooth): return "Level earth"
    }
  }

  func begin(
    _ action: SanctuaryNatureIntent.Action, in world: SanctuaryWorld,
    publishedSupportRevision: UInt64
  ) {
    self.action = action
    lastCommittedOutcome = nil
    invalidate()
    _ = refresh(in: world, publishedSupportRevision: publishedSupportRevision)
  }

  /// Updates the brush from the current production camera target. The cached
  /// result is safe to reuse until either the camera, tools, target sources, or
  /// host-published support changes; no preview state is written into an expedition.
  @discardableResult
  func refresh(
    in world: SanctuaryWorld, publishedSupportRevision: UInt64, force: Bool = false
  ) -> Preview? {
    guard let action else { return nil }
    let state = world.controller.state
    let key = AimKey(
      action: action, position: world.camera.position, yaw: world.camera.yaw, pitch: world.camera.pitch,
      gardenRevision: state.garden?.revision ?? 0, constructionRevision: state.buildings.revision,
      ecologyRevision: state.population.ecology.revision,
      publishedSupportRevision: publishedSupportRevision, tools: state.craftTools)
    if !force, key == aimKey { return currentPreview }

    let draft = world.prepareNatureIntent(action)
    let validation = world.validateNatureIntent(draft)
    let preview = Preview(
      currentDraft: draft, target: validation.target, targetPoint: validation.target.point,
      radius: draft.radius, valid: validation.isValid, rejection: validation.rejection)
    aimKey = key
    currentDraft = draft
    currentPreview = preview
    return preview
  }

  /// Re-aims immediately before casting, so an E press received before the
  /// next render frame cannot apply the previous camera target. The world then
  /// repeats its revision and obstruction validation in the atomic commit.
  @discardableResult
  func confirm(
    in world: SanctuaryWorld, publishedSupportRevision: UInt64
  ) throws -> CommittedOutcome {
    guard action != nil else { throw SanctuaryNatureIntentError.rejected(.unsupportedTarget) }
    guard let preview = refresh(
      in: world, publishedSupportRevision: publishedSupportRevision, force: true) else {
      throw SanctuaryNatureIntentError.rejected(.unsupportedTarget)
    }
    guard preview.valid, let draft = currentDraft else {
      throw SanctuaryNatureIntentError.rejected(preview.rejection ?? .unsupportedTarget)
    }
    let eventID = try world.castNatureIntent(draft)
    let outcome = CommittedOutcome(sourceEventID: eventID, playSeconds: world.controller.state.age)
    lastCommittedOutcome = outcome
    invalidate()
    // Keep the selected brush active for a deliberate repeated cast, but
    // rebuild its candidate from the revision produced by the successful one.
    _ = refresh(in: world, publishedSupportRevision: publishedSupportRevision)
    return outcome
  }

  func cancel() { reset() }

  func reset() {
    action = nil
    lastCommittedOutcome = nil
    invalidate()
  }

  static func feedback(for rejection: SanctuaryNatureIntent.Rejection?) -> String {
    switch rejection {
    case .none: return "Ready to cast. Press E."
    case .staleGarden, .staleConstruction, .staleEcology: return "The ground changed. Aim again."
    case .outOfReach: return "Move closer to the brush target."
    case .water: return "Choose dry ground for this brush."
    case .structure: return "Choose open ground away from construction."
    case .occluded: return "That spot is blocked. Choose a clear patch of ground."
    case .unsupportedTarget: return "Aim at a patch of ground."
    }
  }

  private func invalidate() {
    aimKey = nil
    currentDraft = nil
    currentPreview = nil
  }
}
