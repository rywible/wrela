import Foundation
import simd

public enum SanctuaryVegetationPopulationError: Error, Equatable, Sendable {
  case invalidBounds
  case queryTooLarge
}

public struct SanctuaryVegetationParentKey: Hashable, Codable, Sendable {
  public let x: Int
  public let z: Int

  public init(x: Int, z: Int) {
    self.x = x
    self.z = z
  }

  public var id: String { "vegetation-parent-v1:\(x):\(z)" }
}

/// One deterministic placement instruction shared by future visual and collision assembly.
public struct SanctuaryVegetationRecord: Equatable, Sendable {
  public let id: String
  public let parent: SanctuaryVegetationParentKey
  public let coordinate: SIMD2<Float>
  public let species: SanctuaryVegetationSpecies
  public let scaleSeed: UInt64
  public let scale: Float
  public let yaw: Float

  public init(
    id: String, parent: SanctuaryVegetationParentKey, coordinate: SIMD2<Float>,
    species: SanctuaryVegetationSpecies, scaleSeed: UInt64, scale: Float, yaw: Float
  ) {
    self.id = id
    self.parent = parent
    self.coordinate = coordinate
    self.species = species
    self.scaleSeed = scaleSeed
    self.scale = scale
    self.yaw = yaw
  }

  /// Groundcover has no solid trunk. Every other exact category may contribute collision after
  /// the project resolver applies the same saved-edit mask and transform as presentation.
  public var providesCollision: Bool { species != .reeds }
}

/// Coarse presentation metadata derived from exact children in one stable 128 m world cell.
/// It is never a collision source; collision must use `SanctuaryVegetationRecord` transforms.
public struct SanctuaryVegetationParentSummary: Equatable, Sendable {
  public let id: String
  public let key: SanctuaryVegetationParentKey
  public let bounds: SanctuaryMapBounds
  public let childCount: Int
  public let centroid: SIMD2<Float>
  public let maximumScale: Float
  public let speciesCounts: [SanctuaryVegetationSpecies: Int]
  /// Sorted exact source IDs represented by this noncolliding summary.
  public let childIDs: [String]
  public let childDigest: UInt64

  public var providesCollision: Bool { false }
}

public struct SanctuaryVegetationQuery: Equatable, Sendable {
  public let exact: [SanctuaryVegetationRecord]
  public let parents: [SanctuaryVegetationParentSummary]
}

/// Sparse deterministic vegetation source. Query bounds are cache windows, never identity.
/// A query enumerates complete intersecting parents so summaries are identical across overlaps.
public struct SanctuaryBiomeVegetationPopulation: Sendable {
  public static let candidateCellSize: Float = 32
  public static let parentCellSize: Float = 128
  public static let maximumCandidateCellsPerQuery = 4_096
  public static let maximumAcceptanceProbability: Float = 0.34
  public static let sourceVersion = 1
  public static let candidatesPerChunk = 256
  public static let communityParentsPerChunk = 16

  public let habitat: SanctuaryBiomeHabitatField
  public let seed: UInt64

  public init(habitat: SanctuaryBiomeHabitatField = .init(), seed: UInt64 = 82_317) {
    self.habitat = habitat
    self.seed = seed
  }

  /// Returns exact records inside the requested half-open bounds and complete summaries for every
  /// intersecting parent. At the finite world's maximum edge, the upper bound is inclusive.
  public func query(in requested: SanctuaryMapBounds) throws -> SanctuaryVegetationQuery {
    guard Self.valid(requested) else { throw SanctuaryVegetationPopulationError.invalidBounds }
    let world = SanctuaryGeography.bounds
    let lower = simd_max(requested.minimum, world.minimum)
    let upper = simd_min(requested.maximum, world.maximum)
    guard lower.x < upper.x, lower.y < upper.y else {
      return SanctuaryVegetationQuery(exact: [], parents: [])
    }

    let minParentX = Int(floor(lower.x / Self.parentCellSize))
    let minParentZ = Int(floor(lower.y / Self.parentCellSize))
    let maxParentX = Int(floor(upper.x.nextDown / Self.parentCellSize))
    let maxParentZ = Int(floor(upper.y.nextDown / Self.parentCellSize))
    let parentCount = (maxParentX - minParentX + 1) * (maxParentZ - minParentZ + 1)
    let cellsPerParent = Int(Self.parentCellSize / Self.candidateCellSize)
    guard parentCount > 0,
      parentCount * cellsPerParent * cellsPerParent <= Self.maximumCandidateCellsPerQuery
    else { throw SanctuaryVegetationPopulationError.queryTooLarge }

    var exact: [SanctuaryVegetationRecord] = []
    var parents: [SanctuaryVegetationParentSummary] = []
    for parentZ in minParentZ...maxParentZ {
      for parentX in minParentX...maxParentX {
        let key = SanctuaryVegetationParentKey(x: parentX, z: parentZ)
        let children = try records(in: key)
        if !children.isEmpty { parents.append(summary(for: key, children: children)) }
        exact += children.filter { Self.contains($0.coordinate, lower: lower, upper: upper) }
      }
    }
    exact.sort { $0.id < $1.id }
    parents.sort { $0.id < $1.id }
    return SanctuaryVegetationQuery(exact: exact, parents: parents)
  }

  private func records(
    in parent: SanctuaryVegetationParentKey
  ) throws -> [SanctuaryVegetationRecord] {
    let cellsPerParent = Int(Self.parentCellSize / Self.candidateCellSize)
    let parentMinimum = SIMD2(Float(parent.x), Float(parent.z)) * Self.parentCellSize
    let climateCoordinate = SanctuaryGeography.bounds.clamped(
      parentMinimum + SIMD2<Float>(repeating: Self.parentCellSize * 0.5))
    let climate = try habitat.communityClimate(at: climateCoordinate)
    var records: [SanctuaryVegetationRecord] = []
    for localZ in 0..<cellsPerParent {
      for localX in 0..<cellsPerParent {
        let x = parent.x * cellsPerParent + localX
        let z = parent.z * cellsPerParent + localZ
        if let record = try record(
          candidateX: x, candidateZ: z, communityClimate: climate
        ) { records.append(record) }
      }
    }
    return records
  }

  private func record(
    candidateX x: Int, candidateZ z: Int,
    communityClimate: SanctuaryHabitatClimateSample
  ) throws -> SanctuaryVegetationRecord? {
    let placementSeed = hash(x: x, z: z, stream: 1)
    let jitterX = 0.2 + 0.6 * Self.unit(placementSeed)
    let jitterZ = 0.2 + 0.6 * Self.unit(Self.mix(placementSeed ^ 0xA511_E9B3))
    let coordinate = SIMD2(
      (Float(x) + jitterX) * Self.candidateCellSize,
      (Float(z) + jitterZ) * Self.candidateCellSize)
    guard SanctuaryGeography.bounds.contains(coordinate) else { return nil }
    let sample = try habitat.sample(at: coordinate, communityClimate: communityClimate)
    guard sample.permitsPlacement else { return nil }

    let weighted = SanctuaryVegetationSpecies.allCases.compactMap { species -> (SanctuaryVegetationSpecies, Float)? in
      let value = sample.suitability(for: species)
      return value > 0 ? (species, value) : nil
    }
    guard let strongest = weighted.map({ $0.1 }).max(), strongest > 0 else { return nil }
    let patchiness = clusterPotential(at: coordinate)
    let acceptance = min(
      Self.maximumAcceptanceProbability, strongest * (0.10 + 0.42 * patchiness))
    guard Self.unit(Self.mix(placementSeed ^ 0x76D2_94A5)) < acceptance else { return nil }

    let total = weighted.reduce(Float(0)) { $0 + $1.1 }
    var selection = Self.unit(Self.mix(placementSeed ^ 0xC6BC_2796)) * total
    var species = weighted.last!.0
    for candidate in weighted {
      selection -= candidate.1
      if selection <= 0 {
        species = candidate.0
        break
      }
    }
    let scaleSeed = Self.mix(placementSeed ^ 0xDB4F_0B91)
    let scaleRange: ClosedRange<Float>
    switch species {
    case .broadleaf, .willow: scaleRange = 0.82...1.28
    case .palm: scaleRange = 0.88...1.34
    case .conifer: scaleRange = 0.78...1.42
    case .cactus: scaleRange = 0.72...1.24
    case .reeds: scaleRange = 0.68...1.18
    }
    let scale = scaleRange.lowerBound
      + (scaleRange.upperBound - scaleRange.lowerBound) * Self.unit(scaleSeed)
    let parent = SanctuaryVegetationParentKey(
      x: Int(floor(coordinate.x / Self.parentCellSize)),
      z: Int(floor(coordinate.y / Self.parentCellSize)))
    return SanctuaryVegetationRecord(
      id: "vegetation-v\(Self.sourceVersion):\(x):\(z)", parent: parent,
      coordinate: coordinate, species: species, scaleSeed: scaleSeed, scale: scale,
      yaw: Self.unit(Self.mix(scaleSeed ^ 0x9E37_79B9)) * 2 * .pi)
  }

  private func clusterPotential(at coordinate: SIMD2<Float>) -> Float {
    let cellSize: Float = 256
    let originX = Int(floor(coordinate.x / cellSize))
    let originZ = Int(floor(coordinate.y / cellSize))
    var result: Float = 0
    for z in (originZ - 1)...(originZ + 1) {
      for x in (originX - 1)...(originX + 1) {
        let source = hash(x: x, z: z, stream: 19)
        let center = SIMD2(
          (Float(x) + 0.2 + 0.6 * Self.unit(source)) * cellSize,
          (Float(z) + 0.2 + 0.6 * Self.unit(Self.mix(source))) * cellSize)
        let radius = 58 + 48 * Self.unit(Self.mix(source ^ 0xD816_3841))
        let strength = 0.42 + 0.48 * Self.unit(Self.mix(source ^ 0xCB1A_B31F))
        result += strength * exp(-length_squared(coordinate - center) / (2 * radius * radius))
      }
    }
    return min(1, result)
  }

  private func summary(
    for key: SanctuaryVegetationParentKey, children: [SanctuaryVegetationRecord]
  ) -> SanctuaryVegetationParentSummary {
    let minimum = SIMD2(Float(key.x), Float(key.z)) * Self.parentCellSize
    let centroid = children.reduce(SIMD2<Float>(repeating: 0)) { $0 + $1.coordinate }
      / Float(children.count)
    var speciesCounts: [SanctuaryVegetationSpecies: Int] = [:]
    var digest: UInt64 = 0x9E37_79B9_7F4A_7C15
    let sortedChildren = children.sorted(by: { $0.id < $1.id })
    for child in sortedChildren {
      speciesCounts[child.species, default: 0] += 1
      digest = Self.mix(
        digest ^ Self.textHash(child.id) ^ Self.textHash(child.species.rawValue) ^ child.scaleSeed)
    }
    return SanctuaryVegetationParentSummary(
      id: key.id, key: key,
      bounds: SanctuaryMapBounds(
        minimum: minimum, maximum: minimum + SIMD2(repeating: Self.parentCellSize)),
      childCount: children.count, centroid: centroid,
      maximumScale: children.map(\.scale).max() ?? 0, speciesCounts: speciesCounts,
      childIDs: sortedChildren.map(\.id),
      childDigest: digest)
  }

  private func hash(x: Int, z: Int, stream: UInt64) -> UInt64 {
    var value = seed ^ Self.mix(UInt64(bitPattern: Int64(x)))
    value ^= Self.mix(UInt64(bitPattern: Int64(z)) ^ 0xD1B5_4A32_D192_ED03)
    return Self.mix(value ^ stream)
  }

  private static func mix(_ input: UInt64) -> UInt64 {
    var value = input &+ 0x9E37_79B9_7F4A_7C15
    value = (value ^ (value >> 30)) &* 0xBF58_476D_1CE4_E5B9
    value = (value ^ (value >> 27)) &* 0x94D0_49BB_1331_11EB
    return value ^ (value >> 31)
  }

  private static func unit(_ value: UInt64) -> Float {
    Float(value >> 40) / Float(1 << 24)
  }

  private static func textHash(_ text: String) -> UInt64 {
    text.utf8.reduce(0xCBF2_9CE4_8422_2325) { ($0 ^ UInt64($1)) &* 0x100_0000_01B3 }
  }

  private static func valid(_ bounds: SanctuaryMapBounds) -> Bool {
    bounds.minimum.x.isFinite && bounds.minimum.y.isFinite
      && bounds.maximum.x.isFinite && bounds.maximum.y.isFinite
      && bounds.minimum.x <= bounds.maximum.x && bounds.minimum.y <= bounds.maximum.y
  }

  private static func contains(
    _ point: SIMD2<Float>, lower: SIMD2<Float>, upper: SIMD2<Float>
  ) -> Bool {
    point.x >= lower.x && point.y >= lower.y
      && (point.x < upper.x || (upper.x == SanctuaryGeography.bounds.maximum.x && point.x <= upper.x))
      && (point.y < upper.y || (upper.y == SanctuaryGeography.bounds.maximum.y && point.y <= upper.y))
  }
}
