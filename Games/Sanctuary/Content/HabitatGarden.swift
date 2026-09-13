import Foundation
import simd

/// Deterministic, bounded nature-magic contributions owned by a Sanctuary save.
///
/// Garden water is deliberately an authored shallow-water target. It has no flow,
/// drainage, evaporation, catchment, or cross-patch volume accounting. Consumers
/// must use `waterSurfaceHeight(baseHeight:at:)` for the same local approximation.
public struct HabitatGarden: Codable, Equatable, Sendable {
  public static let maximumPatchCount = 128
  public static let minimumRadius: Float = 0.25
  public static let maximumRadius: Float = 16
  public static let maximumTerrainAmount: Float = 4
  public static let maximumTerrainOffset: Float = 12
  public static let maximumTerrainTargetHeight: Float = 16_000
  public static let maximumPlaySeconds: Double = 1_000_000_000

  public typealias PatchID = UInt64

  public enum Planting: String, Codable, CaseIterable, Sendable {
    case grove, reeds, flowers, shallowWater
  }

  public enum TerrainOperation: String, Codable, CaseIterable, Sendable {
    case raise, lower
    /// Blend toward an explicit world elevation; it does not infer a terrain sample.
    case smooth
  }

  /// World-space XZ location, measured in metres.
  public struct Location: Codable, Equatable, Sendable {
    public var x: Float
    public var z: Float
    public init(x: Float, z: Float) { self.x = x; self.z = z }
  }

  /// One active vegetation or water contribution. IDs are never reused.
  public struct Patch: Codable, Equatable, Sendable {
    public let id: PatchID
    public let planting: Planting
    public let center: Location
    public let radius: Float
    /// `nil` identifies a settled legacy contribution with no recorded play time.
    public let createdAtPlaySeconds: Double?
    public init(
      id: PatchID, planting: Planting, center: Location, radius: Float,
      createdAtPlaySeconds: Double? = nil
    ) {
      self.id = id; self.planting = planting; self.center = center; self.radius = radius
      self.createdAtPlaySeconds = createdAtPlaySeconds
    }
  }

  /// An active height contribution. `targetHeight` is used only by `.smooth`.
  public struct TerrainPatch: Codable, Equatable, Sendable {
    public let id: PatchID
    public let operation: TerrainOperation
    public let center: Location
    public let radius: Float
    public let amount: Float
    public let targetHeight: Float?
    /// `nil` identifies a settled legacy contribution with no recorded play time.
    public let createdAtPlaySeconds: Double?
    public init(
      id: PatchID, operation: TerrainOperation, center: Location, radius: Float,
      amount: Float, targetHeight: Float? = nil, createdAtPlaySeconds: Double? = nil
    ) {
      self.id = id; self.operation = operation; self.center = center; self.radius = radius
      self.amount = amount; self.targetHeight = targetHeight
      self.createdAtPlaySeconds = createdAtPlaySeconds
    }
  }

  /// Commands carry no renderer data and are suitable for recording/replay.
  public enum Command: Codable, Equatable, Sendable {
    case plant(Planting, at: Location, radius: Float)
    case sculpt(TerrainOperation, at: Location, radius: Float, amount: Float, targetHeight: Float?)
    /// Remove only this player's contribution and reveal overlapping contributions.
    case restore(PatchID)
    /// Remove the most recently created still-active contribution.
    case undo
  }

  /// Derived habitat facts at a point. Values are normalized except water depth.
  public struct Conditions: Codable, Equatable, Sendable {
    public var hasShallowWater: Bool
    public var shallowWaterDepth: Float
    public var shelter: Float
    public var flowering: Float
    public var groundcover: Float
    public init(
      hasShallowWater: Bool = false, shallowWaterDepth: Float = 0, shelter: Float = 0,
      flowering: Float = 0, groundcover: Float = 0
    ) {
      self.hasShallowWater = hasShallowWater; self.shallowWaterDepth = shallowWaterDepth
      self.shelter = shelter; self.flowering = flowering; self.groundcover = groundcover
    }
  }

  public private(set) var revision: UInt64
  /// Retained name and representation for saves written before terrain editing.
  public private(set) var patches: [Patch]
  public private(set) var terrainPatches: [TerrainPatch]
  private var nextPatchID: PatchID
  /// Creation order covers both collections, so undo never removes an overlap by value.
  private var activeOrder: [PatchID]

  public init() {
    revision = 0; patches = []; terrainPatches = []; nextPatchID = 1; activeOrder = []
  }

  public var contributionCount: Int { patches.count + terrainPatches.count }

  /// Applies a revision-checked player command atomically.
  /// On every rejection, including stale input, `self` remains unchanged.
  @discardableResult
  public mutating func apply(
    _ command: Command, expectedRevision: UInt64, atPlaySeconds: Double? = nil
  ) throws -> PatchID {
    guard expectedRevision == revision else { throw HabitatGardenError.staleRevision }
    guard Self.isValidPlaySeconds(atPlaySeconds) else { throw HabitatGardenError.invalidTime }
    var candidate = self
    let affectedID: PatchID
    switch command {
    case let .plant(planting, center, radius):
      try candidate.ensureCapacity()
      let patch = Patch(
        id: candidate.nextPatchID, planting: planting, center: center, radius: radius,
        createdAtPlaySeconds: atPlaySeconds)
      try candidate.validate(patch: patch)
      affectedID = patch.id
      candidate.patches.append(patch); candidate.activeOrder.append(patch.id)
      try candidate.advanceIdentifier()
    case let .sculpt(operation, center, radius, amount, targetHeight):
      try candidate.ensureCapacity()
      let patch = TerrainPatch(
        id: candidate.nextPatchID, operation: operation, center: center, radius: radius,
        amount: amount, targetHeight: targetHeight, createdAtPlaySeconds: atPlaySeconds)
      try candidate.validate(patch: patch)
      affectedID = patch.id
      candidate.terrainPatches.append(patch); candidate.activeOrder.append(patch.id)
      try candidate.advanceIdentifier()
    case let .restore(id):
      guard candidate.removeContribution(id: id) else { throw HabitatGardenError.unknownPatch }
      affectedID = id
    case .undo:
      guard let id = candidate.activeOrder.last, candidate.removeContribution(id: id) else {
        throw HabitatGardenError.nothingToUndo
      }
      affectedID = id
    }
    guard candidate.revision < .max else { throw HabitatGardenError.identifierExhausted }
    candidate.revision += 1
    try candidate.validate()
    self = candidate
    return affectedID
  }

  /// Returns habitat facts at an XZ point in the game world, in metres.
  public func conditions(at location: Location) -> Conditions {
    guard Self.isValidCoordinate(location) else { return Conditions() }
    var result = Conditions()
    for patch in patches {
      let influence = Self.influence(at: location, center: patch.center, radius: patch.radius)
      guard influence > 0 else { continue }
      switch patch.planting {
      case .grove:
        result.shelter = max(result.shelter, influence)
        result.groundcover = max(result.groundcover, influence * 0.35)
      case .reeds:
        result.shelter = max(result.shelter, influence * 0.35)
        result.groundcover = max(result.groundcover, influence * 0.55)
      case .flowers:
        result.flowering = max(result.flowering, influence)
        result.groundcover = max(result.groundcover, influence * 0.8)
      case .shallowWater:
        result.hasShallowWater = true
        result.shallowWaterDepth = max(result.shallowWaterDepth, influence * 0.2)
      }
    }
    return result
  }

  /// The only terrain composition query for both mesh generation and collision.
  /// Terrain contributions in the authoritative saved edit order.
  public var orderedTerrainPatches: [TerrainPatch] {
    activeOrder.compactMap { id in terrainPatches.first { $0.id == id } }
  }

  /// Callers provide the unedited terrain height from their shared terrain source.
  public func surfaceHeight(baseHeight: Float, at location: Location) -> Float {
    guard baseHeight.isFinite, Self.isValidCoordinate(location) else { return baseHeight }
    var height = baseHeight
    for id in activeOrder {
      guard let patch = terrainPatches.first(where: { $0.id == id }) else { continue }
      let influence = Self.influence(at: location, center: patch.center, radius: patch.radius)
      guard influence > 0 else { continue }
      switch patch.operation {
      case .raise: height += patch.amount * influence
      case .lower: height -= patch.amount * influence
      case .smooth:
        guard let target = patch.targetHeight else { continue }
        height += (target - height) * (patch.amount * influence)
      }
      height = min(baseHeight + Self.maximumTerrainOffset, max(baseHeight - Self.maximumTerrainOffset, height))
    }
    return height
  }

  /// A local rendering/collision water approximation above the composed ground.
  public func waterSurfaceHeight(baseHeight: Float, at location: Location) -> Float? {
    let conditions = conditions(at: location)
    guard conditions.hasShallowWater else { return nil }
    return surfaceHeight(baseHeight: baseHeight, at: location) + conditions.shallowWaterDepth
  }

  public func validate() throws {
    let ids = patches.map(\.id) + terrainPatches.map(\.id)
    guard contributionCount <= Self.maximumPatchCount, nextPatchID > 0, revision < .max,
      Set(ids).count == ids.count, ids.allSatisfy({ $0 > 0 && $0 < nextPatchID }),
      activeOrder.count == ids.count, Set(activeOrder) == Set(ids)
    else { throw HabitatGardenError.invalidState }
    for patch in patches { try validate(patch: patch) }
    for patch in terrainPatches { try validate(patch: patch) }
  }

  /// Whole-world validation can supply the current saved play time. Legacy
  /// contributions remain valid because their timestamp is intentionally `nil`.
  public func validate(createdAtOrBefore playSeconds: Double) throws {
    guard Self.isValidPlaySeconds(playSeconds) else { throw HabitatGardenError.invalidState }
    try validate()
    for patch in patches where patch.createdAtPlaySeconds.map({ $0 > playSeconds }) ?? false {
      throw HabitatGardenError.invalidState
    }
    for patch in terrainPatches where patch.createdAtPlaySeconds.map({ $0 > playSeconds }) ?? false {
      throw HabitatGardenError.invalidState
    }
  }

  private mutating func ensureCapacity() throws {
    guard contributionCount < Self.maximumPatchCount else { throw HabitatGardenError.patchLimitReached }
  }
  private mutating func advanceIdentifier() throws {
    guard nextPatchID < .max else { throw HabitatGardenError.identifierExhausted }
    nextPatchID += 1
  }
  private mutating func removeContribution(id: PatchID) -> Bool {
    if let index = patches.firstIndex(where: { $0.id == id }) { patches.remove(at: index) }
    else if let index = terrainPatches.firstIndex(where: { $0.id == id }) { terrainPatches.remove(at: index) }
    else { return false }
    activeOrder.removeAll { $0 == id }
    return true
  }
  private func validate(patch: Patch) throws {
    guard Self.isValidCoordinate(patch.center), patch.radius.isFinite,
      (Self.minimumRadius...Self.maximumRadius).contains(patch.radius),
      Self.isValidPlaySeconds(patch.createdAtPlaySeconds)
    else { throw HabitatGardenError.invalidPatch }
  }
  private func validate(patch: TerrainPatch) throws {
    guard Self.isValidCoordinate(patch.center), patch.radius.isFinite, patch.amount.isFinite,
      (Self.minimumRadius...Self.maximumRadius).contains(patch.radius),
      (0...Self.maximumTerrainAmount).contains(patch.amount),
      (patch.operation == .smooth
        ? (patch.targetHeight?.isFinite == true && abs(patch.targetHeight!) <= Self.maximumTerrainTargetHeight)
        : patch.targetHeight == nil),
      Self.isValidPlaySeconds(patch.createdAtPlaySeconds)
    else { throw HabitatGardenError.invalidTerrainPatch }
  }
  private static func isValidCoordinate(_ location: Location) -> Bool {
    guard location.x.isFinite, location.z.isFinite else { return false }
    return SanctuaryGeography.bounds.contains(SIMD2(location.x, location.z))
  }
  private static func isValidPlaySeconds(_ value: Double?) -> Bool {
    guard let value else { return true }
    return value.isFinite && (0...Self.maximumPlaySeconds).contains(value)
  }
  private static func influence(at location: Location, center: Location, radius: Float) -> Float {
    let dx = location.x - center.x, dz = location.z - center.z
    let normalized = max(0, 1 - (dx * dx + dz * dz) / (radius * radius))
    return normalized * normalized * (3 - 2 * normalized)
  }

  private enum CodingKeys: String, CodingKey { case revision, patches, terrainPatches, nextPatchID, activeOrder }
  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    revision = try container.decodeIfPresent(UInt64.self, forKey: .revision) ?? 0
    patches = try container.decodeIfPresent([Patch].self, forKey: .patches) ?? []
    terrainPatches = try container.decodeIfPresent([TerrainPatch].self, forKey: .terrainPatches) ?? []
    let allIDs = patches.map(\.id) + terrainPatches.map(\.id)
    let maximumID = allIDs.max() ?? 0
    guard maximumID < .max else { throw HabitatGardenError.invalidState }
    nextPatchID = try container.decodeIfPresent(PatchID.self, forKey: .nextPatchID) ?? (maximumID + 1)
    activeOrder = try container.decodeIfPresent([PatchID].self, forKey: .activeOrder) ?? allIDs.sorted()
    try validate()
  }
}

public enum HabitatGardenError: LocalizedError, Equatable, Sendable {
  case staleRevision, patchLimitReached, identifierExhausted, unknownPatch, nothingToUndo
  case invalidPatch, invalidTerrainPatch, invalidTime, invalidState
  public var errorDescription: String? {
    switch self {
    case .staleRevision: return "The garden changed. Try your action again."
    case .patchLimitReached: return "This garden has reached its current nature-edit limit."
    case .identifierExhausted, .invalidState: return "This garden could not be updated. Your previous garden is preserved."
    case .unknownPatch: return "That nature contribution is no longer here."
    case .nothingToUndo: return "There are no nature contributions to undo yet."
    case .invalidPatch: return "Choose a planting within the reachable world."
    case .invalidTerrainPatch: return "Choose a bounded terrain edit with valid terrain settings."
    case .invalidTime: return "This nature action has an invalid play-time marker."
    }
  }
}
