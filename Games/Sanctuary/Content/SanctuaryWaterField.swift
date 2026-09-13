import Foundation
import simd

/// Identifies the source whose shoreline and surface were resolved at a world coordinate.
public enum SanctuaryWaterSource: Equatable, Sendable {
  case natural(SanctuaryWaterBody)
  case garden
}

/// Continuous source data used to clip water geometry and to derive production water queries.
///
/// `signedCoverage` is positive in water, zero at its shoreline, and negative on dry ground.
/// It is expressed in metres where the source has a true distance and in a conservative
/// metre-scaled approximation for periodic pool fields. The zero contour is exact in both cases.
public struct SanctuaryWaterFieldSample: Equatable, Sendable {
  public let source: SanctuaryWaterSource
  public let signedCoverage: Float
  public let bedHeight: Float
  public let surfaceHeight: Float
  public let depth: Float

  public init(
    source: SanctuaryWaterSource, signedCoverage: Float, bedHeight: Float,
    surfaceHeight: Float, depth: Float
  ) {
    self.source = source
    self.signedCoverage = signedCoverage
    self.bedHeight = bedHeight
    self.surfaceHeight = surfaceHeight
    self.depth = depth
  }

  public var body: SanctuaryWaterBody? {
    guard case let .natural(body) = source else { return nil }
    return body
  }

  public var isWet: Bool {
    signedCoverage > 0 && depth > 0 && surfaceHeight >= bedHeight
  }

  /// Compatibility representation for existing natural-water gameplay queries.
  public var waterSample: SanctuaryWaterSample? {
    guard let body, isWet else { return nil }
    return SanctuaryWaterSample(body: body, surfaceHeight: surfaceHeight, depth: depth)
  }
}

extension HabitatGarden {
  /// Signed union of every saved shallow-water planting.
  ///
  /// The surface and depth deliberately use the established garden queries, preserving legacy
  /// patch influence, terrain-edit ordering, and overlapping-patch behavior.
  public func waterField(
    baseHeight: Float, at location: Location
  ) -> SanctuaryWaterFieldSample? {
    guard baseHeight.isFinite, location.x.isFinite, location.z.isFinite,
      SanctuaryGeography.bounds.contains(SIMD2(location.x, location.z))
    else { return nil }

    var signedCoverage = -Float.infinity
    for patch in patches where patch.planting == .shallowWater {
      let delta = SIMD2(location.x - patch.center.x, location.z - patch.center.z)
      signedCoverage = max(signedCoverage, patch.radius - length(delta))
    }
    guard signedCoverage.isFinite else { return nil }
    let bed = surfaceHeight(baseHeight: baseHeight, at: location)
    let depth = conditions(at: location).shallowWaterDepth
    return SanctuaryWaterFieldSample(
      source: .garden, signedCoverage: signedCoverage, bedHeight: bed,
      surfaceHeight: bed + depth, depth: depth)
  }
}

extension Terrain {
  /// Width over which a natural water surface meets its terrain bed at a shoreline.
  public static let waterShoreTransitionWidth: Float = 2

  /// Continuous field for one natural water body. The query remains defined on both sides of
  /// the shoreline so a renderer can interpolate the zero crossing without inventing geometry.
  public func waterField(
    for body: SanctuaryWaterBody, at coordinate: SIMD2<Float>, bedHeight: Float? = nil
  ) -> SanctuaryWaterFieldSample? {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      SanctuaryGeography.bounds.contains(coordinate)
    else { return nil }
    if let bedHeight, !bedHeight.isFinite { return nil }
    let bed = bedHeight ?? height(coordinate.x, coordinate.y)
    guard bed.isFinite else { return nil }

    let coverage: Float
    let targetDepth: Float
    switch body {
    case .lake:
      let footprint = SanctuaryGeography.moonlakeFootprint
      let footprintCoverage = (1 - footprint.normalizedRadius(at: coordinate))
        * min(footprint.radii.x, footprint.radii.y)
      let prescribedSurface = 2 + 1.2 * sin(coordinate.x * 0.00045)
      let clearance = prescribedSurface - bed
      coverage = min(footprintCoverage, clearance)
      targetDepth = max(0, clearance)

    case .wetland:
      let center = geography.landmark(for: .wetland).coordinate
      let delta = coordinate - center
      let radialCoverage = 780 - length(delta)
      let pattern = sin(delta.x * 0.011) + cos(delta.y * 0.013) - 1.18
      let patternGradientBound = sqrt(Float(0.011 * 0.011 + 0.013 * 0.013))
      coverage = min(radialCoverage, pattern / patternGradientBound)
      targetDepth = 0.28

    case .creek:
      let a = SIMD2<Float>(700, -180) * 3.5
      let b = SIMD2<Float>(480, 620) * 3.5
      let c = SIMD2<Float>(1_280, 1_120) * 3.5
      func signedSegmentCoverage(_ start: SIMD2<Float>, _ end: SIMD2<Float>) -> Float {
        let segment = end - start
        let t = min(
          1, max(0, dot(coordinate - start, segment) / max(0.001, length_squared(segment))))
        let nearest = start + segment * t
        let effectiveDistance = max(
          0, length(coordinate - nearest) - abs(15 * sin((nearest.x + nearest.y) * 0.009)))
        return 13 - effectiveDistance
      }
      coverage = max(signedSegmentCoverage(a, b), signedSegmentCoverage(b, c))
      targetDepth = 0.42

    case .tidepool:
      let center = geography.landmark(for: .tidepool).coordinate
      let delta = coordinate - center
      let radialCoverage = 720 - length(delta)
      let pattern = sin(delta.x * 0.018) * cos(delta.y * 0.016) - 0.68
      let patternGradientBound = sqrt(Float(0.018 * 0.018 + 0.016 * 0.016))
      let patternCoverage = pattern / patternGradientBound
      // Convert the vertical sea-level gate into a narrow horizontal-like transition measure.
      // The zero contour remains the established `bed >= oceanSurfaceHeight` condition while
      // ordinary dry-shelf pools retain their authored 0.34 m interior depth.
      let dryShelfCoverage = (bed - Self.oceanSurfaceHeight) * 8
      coverage = min(radialCoverage, min(patternCoverage, dryShelfCoverage))
      targetDepth = 0.34

    case .ocean:
      let center = geography.landmark(for: .ocean).coordinate
      let ellipseRadius = length((coordinate - center) / SIMD2<Float>(4_100, 4_600))
      let ellipseCoverage = (1 - ellipseRadius) * 4_100
      let southeastCoverage = min(coordinate.x - 11_800, -10_200 - coordinate.y)
      let footprintCoverage = max(ellipseCoverage, southeastCoverage)
      let clearance = Self.oceanSurfaceHeight - bed
      coverage = min(footprintCoverage, clearance)
      targetDepth = max(0, clearance)
    }

    let depth: Float
    switch body {
    case .creek, .wetland, .tidepool:
      depth = max(0, targetDepth * Self.shoreBlend(for: coverage))
    case .lake, .ocean:
      // These large bodies retain a level surface. Their signed field already incorporates
      // surface-to-bed clearance, so they become dry instead of passing beneath raised terrain.
      depth = max(0, targetDepth)
    }
    return SanctuaryWaterFieldSample(
      source: .natural(body), signedCoverage: coverage, bedHeight: bed,
      surfaceHeight: bed + depth, depth: depth)
  }

  /// Resolves natural water in its established body order, then chooses the higher visible
  /// surface between that winner and authored water. Authored water wins an exact-height tie.
  public func resolvedWaterField(
    at coordinate: SIMD2<Float>, garden: HabitatGarden? = nil
  ) -> SanctuaryWaterFieldSample? {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      SanctuaryGeography.bounds.contains(coordinate)
    else { return nil }
    let base = height(coordinate.x, coordinate.y)
    let location = HabitatGarden.Location(x: coordinate.x, z: coordinate.y)
    let composedBed = garden?.surfaceHeight(baseHeight: base, at: location) ?? base
    let authored = garden?.waterField(baseHeight: base, at: location)
    var natural: SanctuaryWaterFieldSample?
    for body in Self.waterBodyPrecedence {
      if let field = waterField(for: body, at: coordinate, bedHeight: composedBed), field.isWet {
        natural = field
        break
      }
    }
    guard authored?.isWet == true else { return natural }
    guard let natural else { return authored }
    return authored!.surfaceHeight >= natural.surfaceHeight ? authored : natural
  }

  static let waterBodyPrecedence: [SanctuaryWaterBody] = [
    .lake, .wetland, .creek, .tidepool, .ocean,
  ]

  private static func shoreBlend(for signedCoverage: Float) -> Float {
    let t = min(1, max(0, signedCoverage / waterShoreTransitionWidth))
    return t * t * (3 - 2 * t)
  }
}
