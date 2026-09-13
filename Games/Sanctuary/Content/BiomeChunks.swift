import FieldCore
import Foundation
import simd

public struct SanctuaryTerrainChunkKey: Hashable, Codable, Sendable {
  public static let size: Float = 512
  public let x: Int
  public let z: Int

  public init(x: Int, z: Int) { self.x = x; self.z = z }

  public init(containing coordinate: SIMD2<Float>) {
    x = Int(floor(coordinate.x / Self.size))
    z = Int(floor(coordinate.y / Self.size))
  }

  public var minimum: SIMD2<Float> { SIMD2(Float(x) * Self.size, Float(z) * Self.size) }
  public var maximum: SIMD2<Float> { minimum + SIMD2(repeating: Self.size) }
  public var center: SIMD2<Float> { minimum + SIMD2(repeating: Self.size * 0.5) }
  public var id: String { "\(x):\(z)" }

  public var intersectsWorld: Bool {
    maximum.x >= SanctuaryGeography.bounds.minimum.x
      && minimum.x <= SanctuaryGeography.bounds.maximum.x
      && maximum.y >= SanctuaryGeography.bounds.minimum.y
      && minimum.y <= SanctuaryGeography.bounds.maximum.y
  }

  public static func neighborhood(around coordinate: SIMD2<Float>, radius: Int = 1) -> [Self] {
    precondition((0...3).contains(radius))
    let origin = Self(containing: coordinate)
    return (-radius...radius).flatMap { dz in
      (-radius...radius).compactMap { dx in
        let key = Self(x: origin.x + dx, z: origin.z + dz)
        return key.intersectsWorld ? key : nil
      }
    }
  }
}

public struct SanctuaryTerrainCell: Hashable, Codable, Sendable {
  public let x: Int
  public let z: Int
  public init(x: Int, z: Int) { self.x = x; self.z = z }
}

/// Source-level tessellation policy shared by terrain presentation and its interpolation tests.
/// A normal cell is 8 m across. Terrain-magic cells use an 8 × 8 one-metre lattice; coarse
/// neighbors expose the same edge lattice so a center fan joins without T-junction cracks.
public enum SanctuaryTerrainTessellation {
  public static let baseResolution = 64
  public static let refinement = 8
  public static let cabinExtent: Float = 160
  public static let cabinResolution = 320

  public static func cellBounds(
    in key: SanctuaryTerrainChunkKey, cell: SanctuaryTerrainCell
  ) -> (minimum: SIMD2<Float>, maximum: SIMD2<Float>) {
    precondition((0..<baseResolution).contains(cell.x) && (0..<baseResolution).contains(cell.z))
    let step = SanctuaryTerrainChunkKey.size / Float(baseResolution)
    let minimum = key.minimum + SIMD2(Float(cell.x), Float(cell.z)) * step
    return (minimum, minimum + SIMD2(repeating: step))
  }

  public static func refinedCells(
    in key: SanctuaryTerrainChunkKey, garden: HabitatGarden?
  ) -> Set<SanctuaryTerrainCell> {
    guard let garden, !garden.terrainPatches.isEmpty else { return [] }
    let step = SanctuaryTerrainChunkKey.size / Float(baseResolution)
    var result: Set<SanctuaryTerrainCell> = []
    for patch in garden.terrainPatches {
      let center = SIMD2(patch.center.x, patch.center.z)
      let lower = (center - SIMD2(repeating: patch.radius) - key.minimum) / step
      let upper = (center + SIMD2(repeating: patch.radius) - key.minimum) / step
      let minX = max(0, Int(floor(lower.x)))
      let maxX = min(baseResolution - 1, Int(floor(upper.x)))
      let minZ = max(0, Int(floor(lower.y)))
      let maxZ = min(baseResolution - 1, Int(floor(upper.y)))
      guard minX <= maxX, minZ <= maxZ else { continue }
      for z in minZ...maxZ {
        for x in minX...maxX {
          let cell = SanctuaryTerrainCell(x: x, z: z)
          let bounds = cellBounds(in: key, cell: cell)
          let closest = simd_min(simd_max(center, bounds.minimum), bounds.maximum)
          if length_squared(center - closest) <= patch.radius * patch.radius {
            result.insert(cell)
          }
        }
      }
    }
    return result
  }
}

public enum SanctuaryFeatureKind: String, CaseIterable, Codable, Sendable {
  case broadleaf, conifer, willow, reedCluster, flowerPatch, cactus, palm
  case boulder, stoneSpire, driftwood, seaStack, waystone
}

/// A lightweight deterministic instruction. Geometry remains renderer-owned and is instanced.
public struct SanctuaryChunkFeature: Equatable, Sendable {
  public let id: String
  public let kind: SanctuaryFeatureKind
  public let coordinate: SIMD2<Float>
  public let scale: Float
  public let yaw: Float
  public let biome: SanctuaryBiome
  public let isDiscovery: Bool
}

public struct SanctuaryChunkPlan: Equatable, Sendable {
  public let key: SanctuaryTerrainChunkKey
  public let primaryBiome: SanctuaryBiome
  public let features: [SanctuaryChunkFeature]
  public let routeDistance: Float
}

extension SanctuaryGeography {
  /// Environmental silhouettes may stand beside their semantic map coordinate when the
  /// coordinate itself is a creature habitat or traversable gathering place.
  public func featureCoordinate(for landmark: SanctuaryLandmark) -> SIMD2<Float> {
    switch landmark.id {
    case "cloudstep": return landmark.coordinate + SIMD2(12, 0)
    // Keep the scale-five spire beside the gathering place. Its old root at the map
    // coordinate enclosed Saltback's home and blocked the ordinary approach sightline.
    case "saltwind-bluffs": return landmark.coordinate + SIMD2(-18, 0)
    default: return landmark.coordinate
    }
  }

  public static func chunkKey(forFeatureID id: String) -> SanctuaryTerrainChunkKey? {
    guard let separator = id.lastIndex(of: "-") else { return nil }
    let keyText = id[..<separator]
    let components = keyText.split(separator: ":", omittingEmptySubsequences: false)
    guard components.count == 2, let x = Int(components[0]), let z = Int(components[1]),
      Int(id[id.index(after: separator)...]) != nil
    else { return nil }
    return SanctuaryTerrainChunkKey(x: x, z: z)
  }

  public func feature(id: String) -> SanctuaryChunkFeature? {
    guard let key = Self.chunkKey(forFeatureID: id), key.intersectsWorld else { return nil }
    return plan(for: key).features.first { $0.id == id }
  }

  /// Deterministic authored density for any streamed chunk. Ordinary regions contain dozens of
  /// readable forms; periodic waystones/large silhouettes make the distance between the eleven
  /// named destinations an explorable landscape rather than empty scale.
  public func plan(for key: SanctuaryTerrainChunkKey) -> SanctuaryChunkPlan {
    precondition(key.intersectsWorld)
    let centerSample = try! sample(at: key.center)
    let ux = UInt64(bitPattern: Int64(key.x))
    let uz = UInt64(bitPattern: Int64(key.z))
    var rng = SeededRandom(seed: 82_317 ^ (ux &* 0x9E37_79B9) ^ (uz &* 0x85EB_CA6B))
    let count: Int
    switch centerSample.primary {
    case .woodland, .rainforest: count = 52
    case .meadow, .creek, .wetland: count = 40
    case .alpine, .desert, .coast: count = 28
    case .lake, .tidepool, .ocean: count = 18
    }
    let discoveryIndex = (abs(key.x &* 31 &+ key.z &* 17) % 5 == 0) ? 0 : -1
    var features: [SanctuaryChunkFeature] = []
    for index in 0..<count {
      let coordinate = key.minimum + SIMD2(
        rng.range(18, SanctuaryTerrainChunkKey.size - 18),
        rng.range(18, SanctuaryTerrainChunkKey.size - 18))
      guard SanctuaryGeography.bounds.contains(coordinate) else { continue }
      let localBiome = (try? self.sample(at: coordinate).primary) ?? centerSample.primary
      let kind: SanctuaryFeatureKind
      if index == discoveryIndex {
        switch localBiome {
        case .alpine: kind = .stoneSpire
        case .ocean, .coast, .tidepool: kind = .seaStack
        default: kind = .waystone
        }
      } else {
        switch localBiome {
        case .woodland: kind = rng.next() < 0.78 ? .broadleaf : .boulder
        case .meadow: kind = rng.next() < 0.75 ? .flowerPatch : .broadleaf
        case .creek: kind = rng.next() < 0.62 ? .willow : .reedCluster
        case .wetland: kind = rng.next() < 0.78 ? .reedCluster : .willow
        case .lake: kind = rng.next() < 0.56 ? .boulder : .willow
        case .alpine: kind = rng.next() < 0.62 ? .conifer : .stoneSpire
        case .desert: kind = rng.next() < 0.66 ? .cactus : .boulder
        case .rainforest: kind = rng.next() < 0.72 ? .palm : .broadleaf
        case .coast: kind = rng.next() < 0.5 ? .driftwood : .stoneSpire
        case .tidepool: kind = rng.next() < 0.72 ? .boulder : .driftwood
        case .ocean: kind = .seaStack
        }
      }
      let scale: Float
      switch kind {
      case .seaStack: scale = rng.range(5, 16)
      case .stoneSpire: scale = rng.range(2, 7)
      case .waystone: scale = rng.range(1.4, 3.2)
      case .flowerPatch, .reedCluster: scale = rng.range(0.65, 1.35)
      default: scale = rng.range(0.8, 2.2)
      }
      features.append(
        SanctuaryChunkFeature(
          id: "\(key.id)-\(index)", kind: kind, coordinate: coordinate,
          scale: scale, yaw: rng.range(0, 2 * .pi), biome: localBiome,
          isDiscovery: index == discoveryIndex))
    }
    for landmark in Self.landmarks where landmark.id != "cabin-glade"
      && landmark.coordinate.x >= key.minimum.x && landmark.coordinate.x <= key.maximum.x
      && landmark.coordinate.y >= key.minimum.y && landmark.coordinate.y <= key.maximum.y
    {
      let coordinate = featureCoordinate(for: landmark)
      let kind: SanctuaryFeatureKind
      switch landmark.biome {
      case .meadow: kind = .flowerPatch
      case .creek, .wetland: kind = .willow
      case .lake, .tidepool, .ocean: kind = .seaStack
      case .alpine, .coast: kind = .stoneSpire
      case .desert: kind = .cactus
      case .rainforest: kind = .palm
      case .woodland: kind = .waystone
      }
      features.append(
        SanctuaryChunkFeature(
          id: landmark.id, kind: kind, coordinate: coordinate,
          scale: kind == .seaStack ? 8 : (kind == .stoneSpire ? 5 : 2),
          yaw: rng.range(0, 2 * .pi), biome: landmark.biome, isDiscovery: true))
    }
    return SanctuaryChunkPlan(
      key: key, primaryBiome: centerSample.primary, features: features,
      routeDistance: distanceToRoute(at: key.center))
  }
}
