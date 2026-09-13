import Foundation
import simd

/// Player-owned, free basic construction with deterministic collision facts.
///
/// This is a placement model, not a structural simulation: supports, terrain
/// deformation, weathering, resource costs, and automatic pathfinding are all
/// intentionally outside its bounded contract.
public struct PersonalConstruction: Codable, Equatable, Sendable {
  public static let maximumPlacementCount = 128
  public static let maximumHistoryCount = 128
  public static let minimumScale: Float = 0.5
  public static let maximumScale: Float = 3
  public static let minimumElevation: Float = -900
  public static let maximumElevation: Float = 900

  public typealias PlacementID = UInt64

  public enum Primitive: String, Codable, CaseIterable, Sendable {
    case cabin, path, bridge, deck, bench, lantern, fence

    /// Unscaled local half-extents in metres. The origin is at ground/base level.
    public var halfExtents: SIMD3<Float> {
      switch self {
      case .cabin: return SIMD3(2.8, 1.8, 2.4)
      case .path: return SIMD3(1.4, 0.06, 0.55)
      case .bridge: return SIMD3(2.7, 0.18, 0.85)
      case .deck: return SIMD3(2.1, 0.14, 1.7)
      case .bench: return SIMD3(0.9, 0.55, 0.35)
      case .lantern: return SIMD3(0.18, 1.1, 0.18)
      case .fence: return SIMD3(1.25, 0.8, 0.09)
      }
    }

    public var collisionKind: CollisionKind {
      switch self {
      case .path, .bridge, .deck: return .walkable
      case .cabin, .bench, .lantern, .fence: return .solid
      }
    }
  }

  public enum CollisionKind: String, Codable, Equatable, Sendable { case solid, walkable }

  /// World-space location in metres. `y` is the construction base elevation.
  public struct Location: Codable, Equatable, Sendable {
    public var x: Float
    public var y: Float
    public var z: Float
    public init(x: Float, y: Float, z: Float) { self.x = x; self.y = y; self.z = z }
  }

  /// A durable player placement. Scale is uniform to keep collision and rendering aligned.
  public struct Placement: Codable, Equatable, Sendable {
    public let id: PlacementID
    public let primitive: Primitive
    public let location: Location
    public let yawRadians: Float
    public let scale: Float
    public init(
      id: PlacementID, primitive: Primitive, location: Location, yawRadians: Float, scale: Float
    ) {
      self.id = id; self.primitive = primitive; self.location = location
      self.yawRadians = yawRadians; self.scale = scale
    }
  }

  /// Broad-phase facts are generated from the exact persisted placement transform.
  /// A cabin expands into a floor, walls, roof, and a front doorway gap so ordinary
  /// movement can enter it. Consumers can perform their own narrow phase from the
  /// same primitive source mesh.
  public struct CollisionFact: Codable, Equatable, Sendable {
    public let placementID: PlacementID
    public let kind: CollisionKind
    public let center: Location
    public let yawRadians: Float
    public let halfExtents: SIMD3<Float>
    public let bottom: Float
    public let top: Float

    /// Local oriented-box check shared by movement and construction previews.
    public func contains(_ point: Location) -> Bool {
      guard point.x.isFinite, point.y.isFinite, point.z.isFinite,
        point.y >= bottom, point.y <= top
      else { return false }
      let dx = point.x - center.x
      let dz = point.z - center.z
      let c = cos(yawRadians), s = sin(yawRadians)
      let localX = dx * c - dz * s
      let localZ = dx * s + dz * c
      return abs(localX) <= halfExtents.x && abs(localZ) <= halfExtents.z
    }

    /// Tests a standing body's vertical span against this fact at one horizontal point.
    /// A narrow rail can fall between individual height probes, so consumers use the
    /// continuous interval while keeping the same exact oriented horizontal bounds.
    public func intersects(
      at location: Location, from bottom: Float, through top: Float
    ) -> Bool {
      guard location.x.isFinite, location.z.isFinite, bottom.isFinite, top.isFinite,
        bottom <= top, self.bottom <= top, bottom <= self.top
      else { return false }
      let probeY = max(bottom, min(self.top, top))
      return contains(.init(x: location.x, y: probeY, z: location.z))
    }
  }

  /// One compact reversible edit. It records only the affected placement, never a
  /// construction snapshot; `index` restores deterministic placement order.
  public struct HistoryRecord: Codable, Equatable, Sendable {
    public let before: Placement?
    public let after: Placement?
    public let index: Int

    public init(before: Placement?, after: Placement?, index: Int) {
      self.before = before; self.after = after; self.index = index
    }
  }

  public enum Command: Codable, Equatable, Sendable {
    case place(Primitive, at: Location, yawRadians: Float, scale: Float)
    case update(PlacementID, at: Location, yawRadians: Float, scale: Float)
    case remove(PlacementID)
    case select(PlacementID)
    /// Ends an editing session without changing placements or reversible history.
    case deselect
    case undo
  }

  public private(set) var revision: UInt64
  public private(set) var placements: [Placement]
  public private(set) var selectedPlacementID: PlacementID?
  public private(set) var history: [HistoryRecord]
  private var nextPlacementID: PlacementID

  public init() {
    revision = 0; placements = []; selectedPlacementID = nil; history = []; nextPlacementID = 1
  }

  public func placement(id: PlacementID) -> Placement? { placements.first { $0.id == id } }

  /// Applies an edit to an isolated value using the same bounds, overlap, history,
  /// and revision checks as a committed placement. Placement previews use this to
  /// avoid maintaining a second collision or construction-validation path.
  public func validatedCandidate(_ command: Command, expectedRevision: UInt64) throws -> PersonalConstruction {
    var candidate = self
    _ = try candidate.apply(command, expectedRevision: expectedRevision)
    return candidate
  }

  /// Applies a revision-checked command atomically. Rejected commands preserve state.
  @discardableResult
  public mutating func apply(_ command: Command, expectedRevision: UInt64) throws -> PlacementID {
    guard expectedRevision == revision else { throw PersonalConstructionError.staleRevision }
    var candidate = self
    let affectedID: PlacementID
    switch command {
    case let .place(primitive, location, yawRadians, scale):
      guard candidate.placements.count < Self.maximumPlacementCount else {
        throw PersonalConstructionError.placementLimitReached
      }
      let placement = Placement(
        id: candidate.nextPlacementID, primitive: primitive, location: location,
        yawRadians: yawRadians, scale: scale)
      try candidate.validate(placement: placement)
      guard !candidate.hasSolidCollision(with: placement) else {
        throw PersonalConstructionError.overlappingSolid
      }
      affectedID = placement.id
      candidate.placements.append(placement)
      candidate.record(.init(before: nil, after: placement, index: candidate.placements.count - 1))
      guard candidate.nextPlacementID < .max else { throw PersonalConstructionError.identifierExhausted }
      candidate.nextPlacementID += 1
    case let .update(id, location, yawRadians, scale):
      guard let index = candidate.placements.firstIndex(where: { $0.id == id }) else {
        throw PersonalConstructionError.unknownPlacement
      }
      let old = candidate.placements[index]
      let updated = Placement(
        id: old.id, primitive: old.primitive, location: location, yawRadians: yawRadians, scale: scale)
      try candidate.validate(placement: updated)
      candidate.placements.remove(at: index)
      guard !candidate.hasSolidCollision(with: updated) else {
        throw PersonalConstructionError.overlappingSolid
      }
      candidate.placements.insert(updated, at: index)
      candidate.record(.init(before: old, after: updated, index: index))
      affectedID = id
    case let .remove(id):
      guard let index = candidate.placements.firstIndex(where: { $0.id == id }) else {
        throw PersonalConstructionError.unknownPlacement
      }
      let removed = candidate.placements[index]
      affectedID = removed.id
      candidate.placements.remove(at: index)
      candidate.record(.init(before: removed, after: nil, index: index))
      if candidate.selectedPlacementID == id { candidate.selectedPlacementID = nil }
    case let .select(id):
      guard candidate.placements.contains(where: { $0.id == id }) else {
        throw PersonalConstructionError.unknownPlacement
      }
      candidate.selectedPlacementID = id
      affectedID = id
    case .deselect:
      guard let selected = candidate.selectedPlacementID else {
        throw PersonalConstructionError.noSelectedPlacement
      }
      candidate.selectedPlacementID = nil
      affectedID = selected
    case .undo:
      if let record = candidate.history.popLast() {
        affectedID = try candidate.reverse(record)
      } else {
        guard let placement = candidate.placements.last else { throw PersonalConstructionError.nothingToUndo }
        affectedID = placement.id
        candidate.placements.removeLast()
        if candidate.selectedPlacementID == placement.id { candidate.selectedPlacementID = nil }
      }
    }
    guard candidate.revision < .max else { throw PersonalConstructionError.identifierExhausted }
    candidate.revision += 1
    try candidate.validate()
    self = candidate
    return affectedID
  }

  /// Collision facts are stable from `placements` and can be passed directly to movement.
  public var collisionFacts: [CollisionFact] { placements.flatMap(Self.facts(for:)) }

  /// Returns all persisted construction facts containing a world-space point.
  public func collisionFacts(at location: Location) -> [CollisionFact] {
    collisionFacts.filter { $0.contains(location) }
  }

  /// Returns whether an edited solid would occupy any ordinary standing sample.
  /// Controls call this on their candidate before saving, so an edit cannot trap
  /// the player in a newly moved, turned, or enlarged solid.
  public func blocksStanding(at location: Location, eyeHeight: Float) -> Bool {
    guard location.x.isFinite, location.y.isFinite, location.z.isFinite,
      eyeHeight.isFinite, eyeHeight > 0
    else { return true }
    return collisionFacts.contains { fact in
      fact.kind == .solid && fact.intersects(
        at: location, from: location.y + 0.1, through: location.y + max(1, eyeHeight - 0.17))
    }
  }

  public func validate() throws {
    guard placements.count <= Self.maximumPlacementCount, history.count <= Self.maximumHistoryCount,
      nextPlacementID > 0, revision < .max,
      Set(placements.map(\.id)).count == placements.count,
      placements.allSatisfy({ $0.id > 0 && $0.id < nextPlacementID }),
      selectedPlacementID.map({ selected in
        placements.contains(where: { placement in placement.id == selected })
      }) ?? true
    else { throw PersonalConstructionError.invalidState }
    for placement in placements { try validate(placement: placement) }
    for record in history { try validate(record: record) }
    for (index, placement) in placements.enumerated() where placement.primitive.collisionKind == .solid {
      let candidateFacts = Self.facts(for: placement).filter { $0.kind == .solid }
      let earlierFacts = placements[..<index].flatMap(Self.facts(for:)).filter { $0.kind == .solid }
      guard !candidateFacts.contains(where: { candidate in
        earlierFacts.contains { Self.factsOverlap(candidate, $0) }
      }) else { throw PersonalConstructionError.overlappingSolid }
    }
  }

  private func validate(placement: Placement) throws {
    guard placement.location.x.isFinite, placement.location.y.isFinite, placement.location.z.isFinite,
      placement.yawRadians.isFinite, placement.scale.isFinite,
      SanctuaryGeography.bounds.contains(SIMD2(placement.location.x, placement.location.z)),
      (Self.minimumElevation...Self.maximumElevation).contains(placement.location.y),
      (-Float.pi...Float.pi).contains(placement.yawRadians),
      (Self.minimumScale...Self.maximumScale).contains(placement.scale)
    else { throw PersonalConstructionError.invalidPlacement }
  }

  private func validate(record: HistoryRecord) throws {
    guard record.index >= 0, record.index <= Self.maximumPlacementCount,
      record.before != nil || record.after != nil,
      record.before.map({ $0.id > 0 && $0.id < nextPlacementID }) ?? true,
      record.after.map({ $0.id > 0 && $0.id < nextPlacementID }) ?? true,
      (record.before == nil || record.after == nil
        || (record.before?.id == record.after?.id && record.before?.primitive == record.after?.primitive))
    else { throw PersonalConstructionError.invalidState }
    if let before = record.before { try validate(placement: before) }
    if let after = record.after { try validate(placement: after) }
  }

  private mutating func record(_ record: HistoryRecord) {
    history.append(record)
    if history.count > Self.maximumHistoryCount { history.removeFirst(history.count - Self.maximumHistoryCount) }
  }

  private mutating func reverse(_ record: HistoryRecord) throws -> PlacementID {
    switch (record.before, record.after) {
    case let (nil, after?):
      guard record.index < placements.count, placements[record.index] == after else {
        throw PersonalConstructionError.invalidState
      }
      placements.remove(at: record.index)
      if selectedPlacementID == after.id { selectedPlacementID = nil }
      return after.id
    case let (before?, nil):
      guard record.index <= placements.count, !placements.contains(where: { $0.id == before.id }) else {
        throw PersonalConstructionError.invalidState
      }
      placements.insert(before, at: record.index)
      selectedPlacementID = before.id
      return before.id
    case let (before?, after?):
      guard record.index < placements.count, placements[record.index] == after else {
        throw PersonalConstructionError.invalidState
      }
      placements[record.index] = before
      selectedPlacementID = before.id
      return before.id
    case (nil, nil):
      throw PersonalConstructionError.invalidState
    }
  }

  private func hasSolidCollision(with placement: Placement) -> Bool {
    guard placement.primitive.collisionKind == .solid else { return false }
    let facts = Self.facts(for: placement).filter { $0.kind == .solid }
    let existing = placements.flatMap(Self.facts(for:)).filter { $0.kind == .solid }
    return facts.contains { candidate in
      existing.contains { Self.factsOverlap(candidate, $0) }
    }
  }

  private static func fact(
    for placement: Placement, localCenter: SIMD3<Float> = .zero, halfExtents: SIMD3<Float>? = nil,
    kind: CollisionKind? = nil
  ) -> CollisionFact {
    let scaledLocal = localCenter * placement.scale
    let c = cos(placement.yawRadians), s = sin(placement.yawRadians)
    let center = Location(
      x: placement.location.x + scaledLocal.x * c + scaledLocal.z * s,
      y: placement.location.y + scaledLocal.y,
      z: placement.location.z - scaledLocal.x * s + scaledLocal.z * c)
    let resolvedExtents = (halfExtents ?? placement.primitive.halfExtents) * placement.scale
    return CollisionFact(
      placementID: placement.id, kind: kind ?? placement.primitive.collisionKind, center: center,
      yawRadians: placement.yawRadians, halfExtents: resolvedExtents,
      bottom: center.y - resolvedExtents.y, top: center.y + resolvedExtents.y)
  }

  private static func facts(for placement: Placement) -> [CollisionFact] {
    switch placement.primitive {
    case .bridge:
      return walkableFacts(for: placement, profile: SanctuaryWalkableConstructionProfile.bridge)
    case .deck:
      return walkableFacts(for: placement, profile: SanctuaryWalkableConstructionProfile.deck)
    case .cabin:
      break
    default:
      return [fact(for: placement, localCenter: SIMD3(0, placement.primitive.halfExtents.y, 0))]
    }
    // The cabin front is local -Z. Its split front wall leaves a 2.2 m doorway.
    return [
      fact(for: placement, localCenter: SIMD3(0, 0.07, 0), halfExtents: SIMD3(2.8, 0.07, 2.4), kind: .walkable),
      fact(for: placement, localCenter: SIMD3(-2.68, 1.8, 0), halfExtents: SIMD3(0.12, 1.8, 2.4), kind: .solid),
      fact(for: placement, localCenter: SIMD3(2.68, 1.8, 0), halfExtents: SIMD3(0.12, 1.8, 2.4), kind: .solid),
      fact(for: placement, localCenter: SIMD3(0, 1.8, 2.28), halfExtents: SIMD3(2.8, 1.8, 0.12), kind: .solid),
      fact(for: placement, localCenter: SIMD3(-1.9, 1.8, -2.28), halfExtents: SIMD3(0.78, 1.8, 0.12), kind: .solid),
      fact(for: placement, localCenter: SIMD3(1.9, 1.8, -2.28), halfExtents: SIMD3(0.78, 1.8, 0.12), kind: .solid),
      fact(for: placement, localCenter: SIMD3(0, 3.68, 0), halfExtents: SIMD3(2.8, 0.12, 2.4), kind: .solid),
    ]
  }

  /// The shared authored profile controls both the rendered deck surface and
  /// its collision. Rails become conservative solids, while the platform stays
  /// the only walkable fact so its centerline remains usable.
  private static func walkableFacts(
    for placement: Placement, profile: SanctuaryWalkableConstructionProfile.Profile
  ) -> [CollisionFact] {
    [fact(
      for: placement, localCenter: profile.platform.center,
      halfExtents: profile.platform.halfExtents, kind: .walkable
    )] + profile.rails.map { rail in
      let bounds = rail.bounds
      return fact(for: placement, localCenter: bounds.center, halfExtents: bounds.halfExtents, kind: .solid)
    }
  }

  /// Conservative rotated-box overlap for strict placement rejection.
  private static func factsOverlap(_ a: CollisionFact, _ b: CollisionFact) -> Bool {
    guard a.bottom < b.top && b.bottom < a.top else { return false }
    let dx = a.center.x - b.center.x, dz = a.center.z - b.center.z
    let horizontalDistance = sqrt(dx * dx + dz * dz)
    let aRadius = sqrt(a.halfExtents.x * a.halfExtents.x + a.halfExtents.z * a.halfExtents.z)
    let bRadius = sqrt(b.halfExtents.x * b.halfExtents.x + b.halfExtents.z * b.halfExtents.z)
    return horizontalDistance < aRadius + bRadius
  }

  private enum CodingKeys: String, CodingKey {
    case revision, placements, selectedPlacementID, history, nextPlacementID
  }
  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    revision = try container.decodeIfPresent(UInt64.self, forKey: .revision) ?? 0
    placements = try container.decodeIfPresent([Placement].self, forKey: .placements) ?? []
    selectedPlacementID = try container.decodeIfPresent(PlacementID.self, forKey: .selectedPlacementID)
    history = try container.decodeIfPresent([HistoryRecord].self, forKey: .history) ?? []
    let maximumID = placements.map(\.id).max() ?? 0
    guard maximumID < .max else { throw PersonalConstructionError.invalidState }
    nextPlacementID = try container.decodeIfPresent(PlacementID.self, forKey: .nextPlacementID)
      ?? (maximumID + 1)
    try validate()
    try validateHistoryChainAtDecode()
  }

  /// Decoding validates the retained suffix as a reversible chain exactly once.
  /// A trimmed oldest record needs no state before that record, so bounded-history
  /// saves remain compatible while corrupt after-states are rejected.
  private func validateHistoryChainAtDecode() throws {
    var replay = placements
    for record in history.reversed() {
      switch (record.before, record.after) {
      case let (nil, after?):
        guard record.index < replay.count, replay[record.index] == after else {
          throw PersonalConstructionError.invalidState
        }
        replay.remove(at: record.index)
      case let (before?, nil):
        guard record.index <= replay.count, !replay.contains(where: { $0.id == before.id }) else {
          throw PersonalConstructionError.invalidState
        }
        replay.insert(before, at: record.index)
      case let (before?, after?):
        guard record.index < replay.count, replay[record.index] == after else {
          throw PersonalConstructionError.invalidState
        }
        replay[record.index] = before
      case (nil, nil):
        throw PersonalConstructionError.invalidState
      }
    }
  }
}

public enum PersonalConstructionError: LocalizedError, Equatable, Sendable {
  case staleRevision, placementLimitReached, identifierExhausted, unknownPlacement, nothingToUndo
  case invalidPlacement, overlappingSolid, noSelectedPlacement, noReachablePlacement, wouldTrapPlayer, invalidState
  public var errorDescription: String? {
    switch self {
    case .staleRevision: return "The construction changed. Try your action again."
    case .placementLimitReached: return "This homestead has reached its current construction limit."
    case .identifierExhausted, .invalidState: return "This construction could not be updated. Your previous layout is preserved."
    case .unknownPlacement: return "That construction is no longer here."
    case .nothingToUndo: return "There is no construction to undo yet."
    case .invalidPlacement: return "Choose a bounded location, rotation, and scale."
    case .overlappingSolid: return "That solid construction overlaps an existing one."
    case .noSelectedPlacement: return "Select one of your constructions first."
    case .noReachablePlacement: return "No player-built construction is within reach in front of you."
    case .wouldTrapPlayer: return "That edit would place solid construction around you."
    }
  }
}
