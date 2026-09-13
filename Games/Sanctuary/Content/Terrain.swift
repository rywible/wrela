import FieldCore
import Foundation
import simd

public enum SanctuaryWaterBody: String, CaseIterable, Codable, Sendable {
  case creek, wetland, lake, tidepool, ocean
}

public struct SanctuaryWaterSample: Equatable, Sendable {
  public let body: SanctuaryWaterBody
  public let surfaceHeight: Float
  public let depth: Float

  public init(body: SanctuaryWaterBody, surfaceHeight: Float, depth: Float) {
    self.body = body
    self.surfaceHeight = surfaceHeight
    self.depth = depth
  }
}

public struct SanctuaryTerrainSample: Equatable, Sendable {
  public let height: Float
  public let normal: V3
  public let biome: SanctuaryBiome
  public let water: SanctuaryWaterSample?
  public let insideWorld: Bool

  public var isDryLand: Bool { water == nil }
  public var isWalkableSurface: Bool {
    insideWorld && water?.body != .lake && water?.body != .ocean && normal.y >= 0.58
  }
}

/// Procedural source for the complete finite world. Fine presentation is streamed in chunks;
/// this field itself is cheap, deterministic, and queryable at any world coordinate.
public struct Terrain: HeightField {
  public let geography = SanctuaryGeography()
  public let recipe: SanctuaryTerrainRecipe
  public static let oceanSurfaceHeight: Float = -4

  private static let elevations: [SanctuaryBiome: Float] = [
    .woodland: 2, .meadow: 1, .creek: -1, .wetland: -2, .lake: -5,
    .alpine: 135, .desert: 24, .rainforest: 18, .coast: 22, .tidepool: -3, .ocean: -24,
  ]
  private static let relief: [SanctuaryBiome: Float] = [
    .woodland: 4, .meadow: 1.7, .creek: 2, .wetland: 1, .lake: 1,
    .alpine: 34, .desert: 8, .rainforest: 11, .coast: 18, .tidepool: 2, .ocean: 3,
  ]

  public init() { recipe = .legacy }
  public init(recipe: SanctuaryTerrainRecipe) throws {
    try recipe.validate()
    self.recipe = recipe
  }
  public var semantics: FieldSemantics { .implicit }

  /// The winding old cabin path remains bit-for-bit compatible in the legacy garden.
  public func pathX(_ z: Float) -> Float { sin(z * 0.048) * 9 + sin(z * 0.105) * 2 }

  private func legacyHeight(_ x: Float, _ z: Float) -> Float {
    let base =
      0.8 * sin(x * 0.055 + z * 0.019) + 1.25 * sin(z * 0.045)
      + 0.42 * cos(x * 0.13 - z * 0.08)
    let flank = pow(abs(x) / 75, 2) * 15
    let hill = 8 * exp(-((x + 34) * (x + 34) / 700 + (z + 45) * (z + 45) / 1400))
    let ridge =
      17 * exp(-((z + 122) * (z + 122) / 450))
      * pow(0.55 + 0.45 * sin(x * 0.066 + 0.7), 2)
    return base + flank + hill + ridge
  }

  private func smoothstep(_ a: Float, _ b: Float, _ x: Float) -> Float {
    let t = min(1, max(0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)
  }

  private func globalHeight(_ x: Float, _ z: Float) -> Float {
    let coordinate = SIMD2(x, z)
    let sample = try! geography.sample(at: coordinate)
    var elevation: Float = 0
    var relief: Float = 0
    for biome in SanctuaryBiome.allCases {
      let weight = sample.weights[biome] ?? 0
      elevation += weight * (Self.elevations[biome] ?? 0)
      relief += weight * (Self.relief[biome] ?? 1)
    }

    // Broad frequencies remain readable from the air; the high octave is intentionally modest
    // so a 512 m detail chunk does not need excessive tessellation.
    let broad = sin(x * 0.00073 + z * 0.00041) + 0.58 * sin(x * -0.00131 + z * 0.00107)
    let medium = 0.33 * sin(x * 0.0037 + z * -0.0029) + 0.18 * cos(x * 0.0071 + z * 0.0049)
    var result = elevation + relief * (0.54 * broad + medium)

    let alpine = geography.landmark(for: .alpine).coordinate
    let ad = coordinate - alpine
    let ar = length(ad)
    let alpineEnvelope = exp(-(ar * ar) / (2 * 4_000 * 4_000))
    // Cartesian phases stay continuous through the alpine center. Offset Gaussian summits form
    // a 600–850 m skyline around Cloudstep while the named coordinate itself remains a pass.
    let ridges = abs(sin(ad.x * 0.00135 + sin(ad.y * 0.00062) * 1.7))
    result += alpineEnvelope * (118 * pow(ridges, 1.7) + max(0, 72 * (1 - ar / 4_800)))
    let summits: [(SIMD2<Float>, Float, Float)] = [
      (SIMD2(720, 430), 520, 520),
      (SIMD2(-1_050, 820), 410, 620),
      (SIMD2(1_620, -720), 330, 720),
    ]
    for (offset, height, width) in summits {
      let delta = ad - offset
      result += height * exp(-length_squared(delta) / (2 * width * width))
    }

    let desert = geography.landmark(for: .desert).coordinate
    let dd = coordinate - desert
    let desertEnvelope = exp(-length_squared(dd) / (2 * 4_500 * 4_500))
    result += desertEnvelope * (6 * sin(dd.x * 0.006 + sin(dd.y * 0.0017) * 2.1))

    // Limit the amount a route may cut into the source relief. Blending hundreds of metres
    // toward the biome mean across a 115 m verge created artificial near-vertical walls at
    // mountain route endpoints. A smooth bounded correction retains the underlying summits
    // and limits the extra grade introduced by that same compact route corridor.
    let routeDistance = geography.distanceToRoute(at: coordinate)
    let localAverage = elevation + relief * 0.25 * broad
    result = Self.routeGradedHeight(result, toward: localAverage, distance: routeDistance)

    let lakeDistance = SanctuaryGeography.moonlakeFootprint.normalizedRadius(at: coordinate)
    if lakeDistance < 1 {
      let bed = -2.5 - 15 * (1 - lakeDistance)
      result = min(result, bed)
    }
    let ocean = geography.landmark(for: .ocean).coordinate
    let shelf = length((coordinate - ocean) / SIMD2<Float>(4_100, 4_600))
    if shelf < 1 || (coordinate.x > 11_800 && coordinate.y < -10_200) {
      let bed = -7 - 22 * max(0, 1 - shelf)
      result = min(result, bed)
    }

    // Lantern Tidepools sits within the broad ocean ellipse. Raise a finite intertidal shelf
    // after carving the ocean bed, then blend it back into that bed beyond the outer shore.
    // The authored secondary Tidepooler home lies along this shelf toward Saltwind Bluffs.
    let tide = geography.landmark(for: .tidepool).coordinate
    let td = coordinate - tide
    let shoreRadius = length(td / SIMD2<Float>(1_250, 1_900))
    if shoreRadius < 1.18 {
      let dryGrade = Self.oceanSurfaceHeight + 1.6
        - 1.35 * smoothstep(0.55, 0.98, shoreRadius)
        + 0.18 * sin(td.x * 0.012) * cos(td.y * 0.010)
      let raisedGround = max(result, dryGrade)
      if shoreRadius <= 0.98 {
        result = raisedGround
      } else {
        let oceanBlend = smoothstep(0.98, 1.18, shoreRadius)
        result = raisedGround * (1 - oceanBlend) + result * oceanBlend
      }
    }
    return result
  }

  /// A C1 compact route influence with a smooth 20 m correction cap. Its blend contributes
  /// at most 0.214 m/m of cross-route grade (0.82 * 20 * 1.5 / 115), in addition to the
  /// existing terrain's slope. Mountains remain steep where their source relief is steep.
  static func routeGradedHeight(_ height: Float, toward average: Float, distance: Float) -> Float {
    let t = min(1, max(0, (distance - 35) / 115))
    let influence = 1 - t * t * (3 - 2 * t)
    let correction = 20 * tanh((height - average) / 20)
    return height - correction * (0.82 * influence)
  }

  /// Exact legacy terrain in the established cabin garden, smoothly joined to continental relief.
  public func height(_ x: Float, _ z: Float) -> Float {
    let outsideX = max(0, abs(x) - 112)
    let outsideNorth = max(0, z - 78)
    let outsideSouth = max(0, -142 - z)
    let transitionDistance = length(SIMD2(outsideX, max(outsideNorth, outsideSouth)))
    let blend = smoothstep(0, 190, transitionDistance)
    let legacy = legacyHeight(x, z)
    if blend == 0 { return legacy }
    let global = globalHeight(x, z)
    if blend == 1 { return global }
    return legacy * (1 - blend) + global * blend
  }

  public func value(at p: V3) -> Float { p.y - height(p.x, p.z) }

  public func sample(_ x: Float, _ z: Float) -> (height: Float, normal: V3) {
    let h = height(x, z)
    let e: Float = 0.05
    let dx = height(x + e, z) - height(x - e, z)
    let dz = height(x, z + e) - height(x, z - e)
    return (h, normalize(V3(-dx, 2 * e, -dz)))
  }

  public func normal(_ x: Float, _ z: Float) -> V3 { sample(x, z).normal }

  /// Water is a queryable surface over a lower terrain bed. Streamed chunks add its real mesh.
  public func water(at coordinate: SIMD2<Float>) -> SanctuaryWaterSample? {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      SanctuaryGeography.bounds.contains(coordinate)
    else { return nil }
    let bed = height(coordinate.x, coordinate.y)
    for body in Self.waterBodyPrecedence {
      if let sample = waterField(for: body, at: coordinate, bedHeight: bed)?.waterSample {
        return sample
      }
    }
    return nil
  }

  public func surface(at coordinate: SIMD2<Float>) throws -> SanctuaryTerrainSample {
    let biome = try geography.sample(at: coordinate).primary
    let terrain = sample(coordinate.x, coordinate.y)
    return SanctuaryTerrainSample(
      height: terrain.height, normal: terrain.normal, biome: biome,
      water: water(at: coordinate), insideWorld: SanctuaryGeography.bounds.contains(coordinate))
  }
}
