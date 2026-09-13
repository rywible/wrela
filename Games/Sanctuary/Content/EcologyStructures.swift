import Foundation
import simd

public enum EcologyStructureKind: String, CaseIterable, Codable, Hashable, Sendable {
  case dam
  case nest
  case restingBed
  case seedCache
}

public enum EcologyStructureState: String, Codable, Equatable, Sendable {
  case building
  case active
  case displaced
  case rebuilding
}

/// One persistent consequence of an animal's ordinary habitat work.
public struct EcologyStructure: Codable, Equatable, Sendable {
  public let id: String
  public let ownerID: String
  public let kind: EcologyStructureKind
  public private(set) var position: SIMD2<Float>
  public private(set) var yaw: Float
  public let radius: Float
  public private(set) var state: EcologyStructureState
  public private(set) var progress: Float
  public private(set) var revision: UInt64
  /// Contributions already present when construction began are part of the
  /// chosen site. Later intersecting edits can displace the structure.
  public private(set) var foundationContributionIDs: [HabitatGarden.PatchID]

  fileprivate init(
    id: String, ownerID: String, kind: EcologyStructureKind, position: SIMD2<Float>, yaw: Float,
    radius: Float, state: EcologyStructureState, progress: Float,
    foundationContributionIDs: [HabitatGarden.PatchID]
  ) {
    self.id = id
    self.ownerID = ownerID
    self.kind = kind
    self.position = position
    self.yaw = yaw
    self.radius = radius
    self.state = state
    self.progress = progress
    revision = 0
    self.foundationContributionIDs = foundationContributionIDs.sorted()
  }

  fileprivate mutating func setProgress(_ value: Float, completed: Bool) {
    guard state == .building || state == .rebuilding else { return }
    let next = min(1, max(0, value))
    let nextState: EcologyStructureState = completed && next >= 1 ? .active : state
    guard next != progress || nextState != state else { return }
    progress = next
    state = nextState
    revision += 1
  }

  fileprivate mutating func relocateBuilding(
    to position: SIMD2<Float>, yaw: Float,
    foundationContributionIDs: [HabitatGarden.PatchID]
  ) {
    guard state == .building else { return }
    self.position = position
    self.yaw = normalizedYaw(yaw)
    self.foundationContributionIDs = foundationContributionIDs.sorted()
    progress = 0
    revision += 1
  }

  fileprivate mutating func relocateForAuthoredHomeCorrection(
    to position: SIMD2<Float>, yaw: Float,
    foundationContributionIDs: [HabitatGarden.PatchID]
  ) {
    guard state == .displaced else { return }
    self.position = position
    self.yaw = normalizedYaw(yaw)
    self.foundationContributionIDs = foundationContributionIDs.sorted()
    state = .rebuilding
    progress = 0
    revision += 1
  }

  fileprivate mutating func setDisplaced(_ displaced: Bool) -> Bool {
    let next: EcologyStructureState
    if displaced {
      next = .displaced
    } else if state == .displaced {
      next = .rebuilding
    } else {
      return false
    }
    guard next != state else { return false }
    state = next
    progress = 0
    revision += 1
    return true
  }

  fileprivate func intersects(_ center: SIMD2<Float>, radius otherRadius: Float) -> Bool {
    distance(position, center) <= radius + otherRadius
  }

  fileprivate func validate() throws {
    guard !id.isEmpty, id.count <= 96, !ownerID.isEmpty, ownerID.count <= 64,
      position.x.isFinite, position.y.isFinite, SanctuaryGeography.bounds.contains(position),
      yaw.isFinite, abs(yaw) <= Float.pi * 2, radius.isFinite, (0.25...8).contains(radius),
      progress.isFinite, (0...1).contains(progress), revision < .max,
      foundationContributionIDs == foundationContributionIDs.sorted(),
      Set(foundationContributionIDs).count == foundationContributionIDs.count,
      state == .active ? progress == 1 : progress < 1
    else { throw WildlifePopulationError.invalidState }
  }
}

public struct EcologyCollisionFact: Codable, Equatable, Sendable {
  public let structureID: String
  public let center: SIMD2<Float>
  public let radius: Float
  public let height: Float
}

/// A bounded gameplay approximation around a completed dam. This declares a
/// small local pool/rise; it is not drainage, flow, or volume-conserving hydrology.
public struct EcologyWaterFact: Codable, Equatable, Sendable {
  public let structureID: String
  public let center: SIMD2<Float>
  public let radius: Float
  public let upstreamPoolRadius: Float
  public let waterLevelRise: Float
}

public struct EcologyStructures: Codable, Equatable, Sendable {
  public static let maximumCount = 128
  public private(set) var revision: UInt64
  public private(set) var structures: [EcologyStructure]

  public init() {
    revision = 0
    structures = []
  }

  public func structure(id: String) -> EcologyStructure? {
    structures.first { $0.id == id }
  }

  public func structure(ownerID: String) -> EcologyStructure? {
    structures.first { $0.ownerID == ownerID }
  }

  public var collisionFacts: [EcologyCollisionFact] {
    structures.compactMap { structure in
      guard structure.kind == .dam, structure.state == .active else { return nil }
      return EcologyCollisionFact(
        structureID: structure.id, center: structure.position,
        radius: structure.radius * 0.72, height: 0.7)
    }
  }

  public var waterFacts: [EcologyWaterFact] {
    structures.compactMap { structure in
      guard structure.kind == .dam, structure.state == .active else { return nil }
      return EcologyWaterFact(
        structureID: structure.id, center: structure.position, radius: structure.radius,
        upstreamPoolRadius: structure.radius * 3, waterLevelRise: 0.28)
    }
  }

  /// Returns owners whose project was displaced or cleared to rebuild.
  mutating func reconcile(with garden: HabitatGarden) throws -> Set<String> {
    var candidate = self
    var resetOwners: Set<String> = []
    for index in candidate.structures.indices {
      let structure = candidate.structures[index]
      let newPlantingIntersects = garden.patches.contains { patch in
        !structure.foundationContributionIDs.contains(patch.id)
          && !structure.kind.isCompatible(with: patch.planting)
          && structure.intersects(
            SIMD2<Float>(patch.center.x, patch.center.z), radius: patch.radius)
      }
      let terrainIntersects = garden.terrainPatches.contains { patch in
        !structure.foundationContributionIDs.contains(patch.id)
          && structure.intersects(
            SIMD2<Float>(patch.center.x, patch.center.z), radius: patch.radius)
      }
      let obstructed = newPlantingIntersects || terrainIntersects
      if candidate.structures[index].setDisplaced(obstructed) {
        candidate.revision += 1
        resetOwners.insert(structure.ownerID)
      }
    }
    try candidate.validate()
    self = candidate
    return resetOwners
  }

  mutating func synchronize(
    ownerID: String, kind: EcologyStructureKind, position: SIMD2<Float>, yaw: Float,
    work: WildlifeHabitatWork, garden: HabitatGarden
  ) throws {
    var candidate = self
    if let index = candidate.structures.firstIndex(where: { $0.ownerID == ownerID }) {
      var before = candidate.structures[index]
      if before.state == .building, distance(before.position, position) > 0.5 {
        candidate.structures[index].relocateBuilding(
          to: position, yaw: yaw,
          foundationContributionIDs: contributionIDs(at: position, radius: kind.radius, in: garden))
        candidate.revision += 1
        before = candidate.structures[index]
      }
      candidate.structures[index].setProgress(work.progress, completed: work.completed)
      if candidate.structures[index] != before { candidate.revision += 1 }
    } else if work.progress > 0 {
      guard candidate.structures.count < Self.maximumCount else {
        throw WildlifePopulationError.invalidState
      }
      let radius = kind.radius
      let contributionIDs = contributionIDs(at: position, radius: radius, in: garden)
      let structure = EcologyStructure(
        id: "\(ownerID)-\(kind.rawValue)-001", ownerID: ownerID, kind: kind,
        position: position, yaw: normalizedYaw(yaw), radius: radius,
        state: work.completed ? .active : .building, progress: work.progress,
        foundationContributionIDs: contributionIDs)
      candidate.structures.append(structure)
      candidate.structures.sort { $0.id < $1.id }
      candidate.revision += 1
    }
    try candidate.validate()
    self = candidate
  }

  /// Deconstructs only the matching legacy owner's structure. The stable ID is
  /// retained while its owner walks to the corrected authored home.
  mutating func beginAuthoredHomeCorrection(
    ownerID: String, kind: EcologyStructureKind, legacyPosition: SIMD2<Float>
  ) throws {
    var candidate = self
    guard let index = candidate.structures.firstIndex(where: {
      $0.ownerID == ownerID && $0.kind == kind && distance($0.position, legacyPosition) <= 0.5
    }) else { return }
    if candidate.structures[index].setDisplaced(true) { candidate.revision += 1 }
    try candidate.validate()
    self = candidate
  }

  /// Starts a real rebuild at the corrected site after the ordinary animal
  /// brain reaches it. A later garden edit can still displace it normally.
  mutating func finishAuthoredHomeCorrection(
    ownerID: String, kind: EcologyStructureKind, position: SIMD2<Float>, yaw: Float,
    garden: HabitatGarden
  ) throws {
    var candidate = self
    guard let index = candidate.structures.firstIndex(where: {
      $0.ownerID == ownerID && $0.kind == kind && $0.state == .displaced
    }) else { return }
    candidate.structures[index].relocateForAuthoredHomeCorrection(
      to: position, yaw: yaw,
      foundationContributionIDs: contributionIDs(
        at: position, radius: kind.radius, in: garden))
    candidate.revision += 1
    try candidate.validate()
    self = candidate
  }

  public func validate() throws {
    guard structures.count <= Self.maximumCount, revision < .max,
      structures.map(\.id) == structures.map(\.id).sorted(),
      Set(structures.map(\.id)).count == structures.count,
      Set(structures.map(\.ownerID)).count == structures.count
    else { throw WildlifePopulationError.invalidState }
    for structure in structures { try structure.validate() }
  }

  func validate(ownerIDs: Set<String>) throws {
    try validate()
    guard structures.allSatisfy({ ownerIDs.contains($0.ownerID) }) else {
      throw WildlifePopulationError.invalidState
    }
  }

  private enum CodingKeys: String, CodingKey { case revision, structures }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    revision = try container.decode(UInt64.self, forKey: .revision)
    structures = try container.decode([EcologyStructure].self, forKey: .structures)
    try validate()
  }
}

extension EcologyStructureKind {
  fileprivate var radius: Float {
    switch self {
    case .dam: return 2.2
    case .nest: return 0.85
    case .restingBed: return 1.25
    case .seedCache: return 0.65
    }
  }

  /// Plantings that can physically remain within this structure's footprint.
  /// This is intentionally finite: unrelated later plantings still displace
  /// the structure and terrain edits always do. The sets cover the authored
  /// preferred habitats of the species that build each shared structure kind.
  fileprivate func isCompatible(with planting: HabitatGarden.Planting) -> Bool {
    switch self {
    case .dam:
      return planting == .shallowWater || planting == .reeds
    case .nest:
      return planting == .grove || planting == .reeds || planting == .shallowWater
    case .restingBed:
      return planting == .flowers || planting == .grove || planting == .reeds
    case .seedCache:
      return planting == .flowers || planting == .grove
    }
  }
}

private func normalizedYaw(_ yaw: Float) -> Float {
  atan2(sin(yaw), cos(yaw))
}

private func contributionIDs(
  at position: SIMD2<Float>, radius: Float, in garden: HabitatGarden
) -> [HabitatGarden.PatchID] {
  (
    garden.patches.compactMap { patch in
      distance(position, SIMD2<Float>(patch.center.x, patch.center.z)) <= radius + patch.radius
        ? patch.id : nil
    } + garden.terrainPatches.compactMap { patch in
      distance(position, SIMD2<Float>(patch.center.x, patch.center.z)) <= radius + patch.radius
        ? patch.id : nil
    }
  ).sorted()
}
