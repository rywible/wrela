import Foundation
import simd

/// Stable, game-owned identifiers for Sanctuary's reachable environments.
/// These describe intended geography only; they do not create terrain, water, or render batches.
public enum SanctuaryBiome: String, CaseIterable, Codable, Sendable {
  case woodland, meadow, creek, wetland, lake, alpine, desert, rainforest, coast, tidepool, ocean
}

public enum SanctuaryTraversal: String, CaseIterable, Codable, Sendable {
  case walk, ride, fly
}

public enum SanctuaryGeographyError: Error, Equatable, Sendable {
  case invalidCoordinate
}

public struct SanctuaryMapBounds: Codable, Equatable, Sendable {
  public let minimum: SIMD2<Float>
  public let maximum: SIMD2<Float>

  public init(minimum: SIMD2<Float>, maximum: SIMD2<Float>) {
    self.minimum = minimum
    self.maximum = maximum
  }

  public func contains(_ point: SIMD2<Float>) -> Bool {
    point.x >= minimum.x && point.x <= maximum.x && point.y >= minimum.y && point.y <= maximum.y
  }

  public var center: SIMD2<Float> { (minimum + maximum) * 0.5 }
  public var size: SIMD2<Float> { maximum - minimum }

  public func clamped(_ point: SIMD2<Float>, inset: Float = 0) -> SIMD2<Float> {
    SIMD2(
      min(max(point.x, minimum.x + inset), maximum.x - inset),
      min(max(point.y, minimum.y + inset), maximum.y - inset))
  }

  /// Positive inside the finite world and negative outside it.
  public func signedDistanceToBoundary(_ point: SIMD2<Float>) -> Float {
    if contains(point) {
      return min(
        min(point.x - minimum.x, maximum.x - point.x),
        min(point.y - minimum.y, maximum.y - point.y))
    }
    let dx = max(max(minimum.x - point.x, 0), point.x - maximum.x)
    let dz = max(max(minimum.y - point.y, 0), point.y - maximum.y)
    return -length(SIMD2(dx, dz))
  }
}

public struct SanctuaryLandmark: Codable, Equatable, Sendable {
  public let id: String
  public let name: String
  public let coordinate: SIMD2<Float>
  public let biome: SanctuaryBiome

  public init(id: String, name: String, coordinate: SIMD2<Float>, biome: SanctuaryBiome) {
    self.id = id
    self.name = name
    self.coordinate = coordinate
    self.biome = biome
  }
}

/// A finite horizontal ellipse used by geography, terrain, and discovery rules.
public struct SanctuaryEllipse: Codable, Equatable, Sendable {
  public let center: SIMD2<Float>
  public let radii: SIMD2<Float>

  public init(center: SIMD2<Float>, radii: SIMD2<Float>) {
    self.center = center
    self.radii = radii
  }

  public func normalizedRadius(at point: SIMD2<Float>) -> Float {
    length((point - center) / radii)
  }

  public func contains(_ point: SIMD2<Float>) -> Bool {
    normalizedRadius(at: point) < 1
  }
}

/// A named edge in the finite world graph. Requirements are capabilities, never a render rule.
public struct SanctuaryRoute: Codable, Equatable, Sendable {
  public let id: String
  public let from: String
  public let to: String
  public let requiredTraversal: SanctuaryTraversal

  public init(id: String, from: String, to: String, requiredTraversal: SanctuaryTraversal) {
    self.id = id
    self.from = from
    self.to = to
    self.requiredTraversal = requiredTraversal
  }
}

/// Normalized, continuous habitat influence at a coordinate.
public struct SanctuaryBiomeSample: Equatable, Sendable {
  public let weights: [SanctuaryBiome: Float]
  public let primary: SanctuaryBiome
  /// Planned capabilities only. Runtime navigation must validate terrain, water and companions.
  public let proposedTraversal: Set<SanctuaryTraversal>

  public init(
    weights: [SanctuaryBiome: Float], primary: SanctuaryBiome,
    proposedTraversal: Set<SanctuaryTraversal>
  ) {
    self.weights = weights
    self.primary = primary
    self.proposedTraversal = proposedTraversal
  }
}

/// The fixed, finite geography contract for the flagship world.
///
/// The sites deliberately overlap with smooth radial influence, so simulation can ask for gradual
/// habitat transitions before terrain, water, ecology, or streaming are integrated.
public struct SanctuaryGeography: Sendable {
  public static let version = 2
  /// Provisional 32 × 32 km finite world. Local detail is authored in deterministic 512 m chunks;
  /// a coarse full-world mesh keeps major landforms visible beyond the active neighborhood.
  public static let bounds = SanctuaryMapBounds(
    minimum: SIMD2(-16_000, -16_000), maximum: SIMD2(16_000, 16_000))

  /// Provisional storybook names and placements; their stable IDs are the persistence contract.
  public static let landmarks: [SanctuaryLandmark] = [
    .init(id: "cabin-glade", name: "Cabin Glade", coordinate: SIMD2(0, 0), biome: .woodland),
    .init(id: "golden-meadow", name: "Golden Meadow", coordinate: SIMD2(2_450, -630), biome: .meadow),
    .init(id: "willow-creek", name: "Willow Creek", coordinate: SIMD2(1_680, 2_170), biome: .creek),
    .init(id: "reed-mirror", name: "Reed Mirror Wetland", coordinate: SIMD2(4_480, 3_920), biome: .wetland),
    .init(id: "moonlake", name: "Moonlake", coordinate: SIMD2(6_020, 6_300), biome: .lake),
    .init(id: "cloudstep", name: "Cloudstep Alpine", coordinate: SIMD2(-420, 10_675), biome: .alpine),
    .init(id: "sunglass-dunes", name: "Sunglass Dunes", coordinate: SIMD2(-8_575, 5_775), biome: .desert),
    .init(id: "verdant-canopy", name: "Verdant Canopy", coordinate: SIMD2(8_575, -2_975), biome: .rainforest),
    .init(id: "saltwind-bluffs", name: "Saltwind Bluffs", coordinate: SIMD2(11_725, -7_175), biome: .coast),
    .init(id: "lantern-tidepools", name: "Lantern Tidepools", coordinate: SIMD2(12_950, -9_520), biome: .tidepool),
    .init(id: "open-blue", name: "Open Blue", coordinate: SIMD2(13_825, -12_425), biome: .ocean),
  ]

  /// Shared source for Moonlake's terrain bed, water surface, and reachable shore discovery.
  public static let moonlakeFootprint = SanctuaryEllipse(
    center: SIMD2(6_020, 6_300), radii: SIMD2(1_050, 820))
  public static let moonlakeShoreBand: ClosedRange<Float> = 1...1.06

  /// Every landmark has a route to the cabin graph. The ocean is reached from shore by flight;
  /// its eventual boat/swimming policy belongs to the runtime traversal implementation.
  public static let routes: [SanctuaryRoute] = [
    .init(id: "cabin-meadow", from: "cabin-glade", to: "golden-meadow", requiredTraversal: .walk),
    .init(id: "meadow-creek", from: "golden-meadow", to: "willow-creek", requiredTraversal: .walk),
    .init(id: "creek-wetland", from: "willow-creek", to: "reed-mirror", requiredTraversal: .walk),
    .init(id: "wetland-lake", from: "reed-mirror", to: "moonlake", requiredTraversal: .walk),
    .init(id: "lake-alpine", from: "moonlake", to: "cloudstep", requiredTraversal: .ride),
    .init(id: "alpine-desert", from: "cloudstep", to: "sunglass-dunes", requiredTraversal: .ride),
    .init(id: "meadow-rainforest", from: "golden-meadow", to: "verdant-canopy", requiredTraversal: .ride),
    .init(id: "rainforest-coast", from: "verdant-canopy", to: "saltwind-bluffs", requiredTraversal: .ride),
    .init(id: "coast-tidepools", from: "saltwind-bluffs", to: "lantern-tidepools", requiredTraversal: .walk),
    .init(id: "tidepools-ocean", from: "lantern-tidepools", to: "open-blue", requiredTraversal: .fly),
  ]

  private static let radii: [SanctuaryBiome: Float] = [
    .woodland: 4_725, .meadow: 4_025, .creek: 2_730, .wetland: 3_220, .lake: 4_375,
    .alpine: 6_300, .desert: 6_650, .rainforest: 5_950, .coast: 3_675,
    .tidepool: 2_170, .ocean: 6_650,
  ]

  public init() {}

  public func landmark(id: String) -> SanctuaryLandmark? {
    Self.landmarks.first { $0.id == id }
  }

  public func landmark(for biome: SanctuaryBiome) -> SanctuaryLandmark {
    Self.landmarks.first { $0.biome == biome }!
  }

  public func nearestLandmark(to coordinate: SIMD2<Float>) -> SanctuaryLandmark {
    Self.landmarks.min {
      length_squared($0.coordinate - coordinate) < length_squared($1.coordinate - coordinate)
    }!
  }

  /// Named sites use a close approach. Moonlake also accepts its narrow dry shore band because
  /// the landmark coordinate describes the deep-water center rather than a traversable point.
  public func isAtLandmark(
    _ landmark: SanctuaryLandmark, position: SIMD2<Float>, centerRadius: Float = 28
  ) -> Bool {
    guard centerRadius.isFinite, centerRadius >= 0,
      position.x.isFinite, position.y.isFinite
    else { return false }
    if distance(landmark.coordinate, position) < centerRadius { return true }
    guard landmark.biome == .lake else { return false }
    return Self.moonlakeShoreBand.contains(Self.moonlakeFootprint.normalizedRadius(at: position))
  }

  public func endpoints(for route: SanctuaryRoute) -> (SIMD2<Float>, SIMD2<Float>)? {
    guard let a = landmark(id: route.from), let b = landmark(id: route.to) else { return nil }
    return (a.coordinate, b.coordinate)
  }

  public func distanceToRoute(at coordinate: SIMD2<Float>) -> Float {
    Self.routes.compactMap { endpoints(for: $0) }.map { a, b in
      let segment = b - a
      let t = min(1, max(0, dot(coordinate - a, segment) / max(0.001, length_squared(segment))))
      return length(coordinate - (a + segment * t))
    }.min() ?? .infinity
  }

  /// Returns normalized, continuous Gaussian weights using a stable double-precision log sum.
  /// Finite queries up to one million metres outside the origin remain classifiable for tools;
  /// they report no proposed traversal beyond the finite map.
  public func sample(at coordinate: SIMD2<Float>) throws -> SanctuaryBiomeSample {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      abs(coordinate.x) <= 1_000_000, abs(coordinate.y) <= 1_000_000
    else { throw SanctuaryGeographyError.invalidCoordinate }

    var logWeights: [SanctuaryBiome: Double] = [:]
    for landmark in Self.landmarks {
      let radius = Self.radii[landmark.biome]!
      let dx = Double(coordinate.x - landmark.coordinate.x)
      let dz = Double(coordinate.y - landmark.coordinate.y)
      let normalizedDistanceSquared = (dx * dx + dz * dz) / Double(radius * radius)
      logWeights[landmark.biome] = -0.5 * normalizedDistanceSquared
    }
    let maximum = SanctuaryBiome.allCases.map { logWeights[$0]! }.max()!
    let total = SanctuaryBiome.allCases.reduce(0.0) { sum, biome in
      sum + exp(logWeights[biome]! - maximum)
    }
    var weights: [SanctuaryBiome: Float] = [:]
    var primary = SanctuaryBiome.allCases[0]
    var greatest = -Double.infinity
    for biome in SanctuaryBiome.allCases {
      let weight = exp(logWeights[biome]! - maximum) / total
      weights[biome] = Float(weight)
      if weight > greatest {
        greatest = weight
        primary = biome
      }
    }
    let proposedTraversal: Set<SanctuaryTraversal>
    if !Self.bounds.contains(coordinate) {
      proposedTraversal = []
    } else if primary == .lake || primary == .ocean {
      proposedTraversal = [.fly]
    } else {
      proposedTraversal = [.walk, .ride, .fly]
    }
    return SanctuaryBiomeSample(
      weights: weights, primary: primary,
      proposedTraversal: proposedTraversal)
  }
}
