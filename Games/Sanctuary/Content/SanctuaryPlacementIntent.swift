import Foundation
import simd

public extension PersonalConstruction.Primitive {
  /// Bridges and decks are the only player construction intended to stand in
  /// the bounded shallow-water support model. All other primitives need dry ground.
  var supportsShallowWater: Bool { self == .bridge || self == .deck }
}

/// A short-lived construction draft. This is deliberately not part of an
/// expedition save: it is a native interaction state, while confirmed
/// construction remains exclusively in `PersonalConstruction`.
public struct SanctuaryPlacementIntent: Equatable, Sendable {
  /// The transform the player currently intends to place or edit. `selectedPlacementID`
  /// is nil for a new placement and remains stable for an edit preview.
  public struct Draft: Equatable, Sendable {
    public let selectedPlacementID: PersonalConstruction.PlacementID?
    public let primitive: PersonalConstruction.Primitive
    public let location: PersonalConstruction.Location
    public let yawRadians: Float
    public let scale: Float
    public let expectedConstructionRevision: UInt64

    fileprivate init(
      selectedPlacementID: PersonalConstruction.PlacementID?,
      primitive: PersonalConstruction.Primitive, location: PersonalConstruction.Location,
      yawRadians: Float, scale: Float, expectedConstructionRevision: UInt64
    ) {
      self.selectedPlacementID = selectedPlacementID
      self.primitive = primitive
      self.location = location
      self.yawRadians = yawRadians
      self.scale = scale
      self.expectedConstructionRevision = expectedConstructionRevision
    }

    fileprivate var command: PersonalConstruction.Command {
      if let selectedPlacementID {
        return .update(selectedPlacementID, at: location, yawRadians: yawRadians, scale: scale)
      }
      return .place(primitive, at: location, yawRadians: yawRadians, scale: scale)
    }
  }

  /// Facts supplied by the ordinary world raycast and player collision path.
  /// The intent does not recreate line-of-sight or obstruction geometry.
  public struct Context: Equatable, Sendable {
    /// A current, game-owned animal body volume. It is supplied by Sanctuary's
    /// elevation path rather than inferred from a rendered mesh.
    public struct Occupant: Equatable, Sendable {
      public let id: String
      public let center: SIMD2<Float>
      public let radius: Float
      public let bottom: Float
      public let top: Float

      public init(id: String, center: SIMD2<Float>, radius: Float, bottom: Float, top: Float) {
        self.id = id
        self.center = center
        self.radius = radius
        self.bottom = bottom
        self.top = top
      }
    }

    public let playerFeet: PersonalConstruction.Location
    public let reach: Float
    public let eyeHeight: Float
    public let targetIsOutOfReach: Bool
    public let targetIsObstructed: Bool
    /// The current authoritative shallow-water surface at the intended base,
    /// when the target is wet. The terrain ray intentionally returns the bed.
    public let waterSurfaceHeight: Float?
    public let occupants: [Occupant]

    public init(
      playerFeet: PersonalConstruction.Location, reach: Float, eyeHeight: Float,
      targetIsOutOfReach: Bool = false, targetIsObstructed: Bool,
      waterSurfaceHeight: Float? = nil, occupants: [Occupant] = []
    ) {
      self.playerFeet = playerFeet
      self.reach = reach
      self.eyeHeight = eyeHeight
      self.targetIsOutOfReach = targetIsOutOfReach
      self.targetIsObstructed = targetIsObstructed
      self.waterSurfaceHeight = waterSurfaceHeight
      self.occupants = occupants
    }
  }

  public enum Rejection: Equatable, Sendable {
    case staleConstruction
    case targetOutOfReach
    case targetObstructed
    case waterRequiresWalkway
    case wouldTrapPlayer
    case occupiedAnimal
    case invalidContext
    case construction(PersonalConstructionError)
  }

  /// Always carries the intended draft while one is active. A renderer can keep
  /// its ghost visible when `rejection` explains why confirmation is unavailable.
  public struct Preview: Equatable, Sendable {
    public let placement: Draft
    public let command: PersonalConstruction.Command
    public let rejection: Rejection?

    public var isValid: Bool { rejection == nil }
  }

  public private(set) var draft: Draft?

  public init() { draft = nil }

  public var selectedPlacementID: PersonalConstruction.PlacementID? { draft?.selectedPlacementID }

  public mutating func beginPlace(
    _ primitive: PersonalConstruction.Primitive, at location: PersonalConstruction.Location,
    yawRadians: Float, scale: Float, constructionRevision: UInt64
  ) {
    draft = .init(
      selectedPlacementID: nil, primitive: primitive, location: location, yawRadians: yawRadians,
      scale: scale, expectedConstructionRevision: constructionRevision)
  }

  public mutating func beginEdit(
    _ placement: PersonalConstruction.Placement, constructionRevision: UInt64
  ) {
    draft = .init(
      selectedPlacementID: placement.id, primitive: placement.primitive, location: placement.location,
      yawRadians: placement.yawRadians, scale: placement.scale,
      expectedConstructionRevision: constructionRevision)
  }

  public mutating func setAim(_ location: PersonalConstruction.Location) {
    guard let draft else { return }
    self.draft = .init(
      selectedPlacementID: draft.selectedPlacementID, primitive: draft.primitive, location: location,
      yawRadians: draft.yawRadians, scale: draft.scale,
      expectedConstructionRevision: draft.expectedConstructionRevision)
  }

  public mutating func turn45() {
    guard let draft else { return }
    let yaw = draft.yawRadians + .pi / 4
    self.draft = .init(
      selectedPlacementID: draft.selectedPlacementID, primitive: draft.primitive,
      location: draft.location, yawRadians: atan2(sin(yaw), cos(yaw)), scale: draft.scale,
      expectedConstructionRevision: draft.expectedConstructionRevision)
  }

  public mutating func setScale(_ scale: Float) {
    guard let draft else { return }
    self.draft = .init(
      selectedPlacementID: draft.selectedPlacementID, primitive: draft.primitive,
      location: draft.location, yawRadians: draft.yawRadians, scale: scale,
      expectedConstructionRevision: draft.expectedConstructionRevision)
  }

  /// Cancelling is entirely transient: no expedition or construction value changes.
  public mutating func cancel() { draft = nil }

  public func preview(in construction: PersonalConstruction, context: Context) -> Preview? {
    guard let draft else { return nil }
    let command = draft.command
    return .init(placement: draft, command: command,
      rejection: rejection(for: draft, command: command, in: construction, context: context))
  }

  /// Rechecks the current construction and invokes the supplied atomic save path once.
  /// A failed save, stale revision, or rejected candidate leaves this draft available
  /// for correction; a successful commit clears it, preventing a duplicate confirm.
  @discardableResult
  public mutating func confirm(
    in construction: PersonalConstruction, context: Context,
    commit: (PersonalConstruction.Command, UInt64) throws -> PersonalConstruction.PlacementID
  ) throws -> PersonalConstruction.PlacementID {
    guard let preview = preview(in: construction, context: context) else {
      throw SanctuaryPlacementIntentError.noActivePlacement
    }
    if let rejection = preview.rejection {
      throw SanctuaryPlacementIntentError.rejected(rejection)
    }
    let placementID = try commit(preview.command, preview.placement.expectedConstructionRevision)
    draft = nil
    return placementID
  }

  private func rejection(
    for draft: Draft, command: PersonalConstruction.Command, in construction: PersonalConstruction,
    context: Context
  ) -> Rejection? {
    guard context.playerFeet.x.isFinite, context.playerFeet.y.isFinite, context.playerFeet.z.isFinite,
      context.reach.isFinite, context.reach > 0, context.eyeHeight.isFinite, context.eyeHeight > 0,
      context.waterSurfaceHeight.map(\.isFinite) ?? true, context.occupants.count <= 25,
      context.occupants.allSatisfy({
        !$0.id.isEmpty && $0.id.count <= 128 && $0.center.x.isFinite && $0.center.y.isFinite
          && $0.radius.isFinite && $0.radius > 0 && $0.bottom.isFinite && $0.top.isFinite
          && $0.bottom < $0.top
      })
    else { return .invalidContext }
    guard construction.revision == draft.expectedConstructionRevision else {
      return .staleConstruction
    }
    let dx = draft.location.x - context.playerFeet.x
    let dz = draft.location.z - context.playerFeet.z
    guard dx.isFinite, dz.isFinite, sqrt(dx * dx + dz * dz) <= context.reach else {
      return .targetOutOfReach
    }
    guard !context.targetIsOutOfReach else { return .targetOutOfReach }
    guard !context.targetIsObstructed else { return .targetObstructed }
    if let water = context.waterSurfaceHeight, water > draft.location.y + 0.01,
      !draft.primitive.supportsShallowWater {
      return .waterRequiresWalkway
    }
    do {
      let candidate = try construction.validatedCandidate(
        command, expectedRevision: draft.expectedConstructionRevision)
      if candidate.blocksStanding(at: context.playerFeet, eyeHeight: context.eyeHeight) {
        return .wouldTrapPlayer
      }
      let affectedID = draft.selectedPlacementID ?? candidate.placements.last?.id
      guard let affectedID else { return .construction(.invalidState) }
      let affectedFacts = candidate.collisionFacts.filter { $0.placementID == affectedID }
      if affectedFacts.contains(where: { fact in
        context.occupants.contains { Self.overlaps(fact, occupant: $0) }
      }) {
        return .occupiedAnimal
      }
    } catch let error as PersonalConstructionError {
      return .construction(error)
    } catch {
      return .construction(.invalidState)
    }
    return nil
  }

  /// Shared by transient preview validation and the save-backed construction
  /// transaction, so an actor that moves after a preview cannot be intersected
  /// by the committed candidate.
  static func overlaps(
    _ fact: PersonalConstruction.CollisionFact, occupant: Context.Occupant
  ) -> Bool {
    guard fact.bottom < occupant.top - 0.001, fact.top > occupant.bottom + 0.001 else {
      return false
    }
    let dx = occupant.center.x - fact.center.x
    let dz = occupant.center.y - fact.center.z
    let c = cos(fact.yawRadians), s = sin(fact.yawRadians)
    let localX = dx * c - dz * s
    let localZ = dx * s + dz * c
    let nearestX = min(fact.halfExtents.x, max(-fact.halfExtents.x, localX))
    let nearestZ = min(fact.halfExtents.z, max(-fact.halfExtents.z, localZ))
    let ox = localX - nearestX, oz = localZ - nearestZ
    return ox * ox + oz * oz <= occupant.radius * occupant.radius
  }
}

public enum SanctuaryPlacementIntentError: LocalizedError, Equatable, Sendable {
  case noActivePlacement
  case rejected(SanctuaryPlacementIntent.Rejection)

  public var errorDescription: String? {
    switch self {
    case .noActivePlacement: return "Choose a construction before confirming it."
    case let .rejected(reason):
      switch reason {
      case .staleConstruction: return "The construction changed. Aim it again."
      case .targetOutOfReach: return "Aim within the current construction reach."
      case .targetObstructed: return "That placement target is obstructed."
      case .waterRequiresWalkway: return "Only a bridge or deck can be placed in shallow water."
      case .wouldTrapPlayer: return "That construction would trap you."
      case .occupiedAnimal: return "Wait for the nearby animal to move before placing construction."
      case .invalidContext: return "The placement target is not usable."
      case let .construction(error): return error.errorDescription
      }
    }
  }
}
