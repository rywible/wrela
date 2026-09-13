import Foundation
import simd

public enum SanctuaryEnvironmentFieldError: Error, Equatable, Sendable {
  case invalidRecipe
  case invalidCoordinate
  case invalidWeather
  case invalidSurface
}

/// Authored constraints for a stable habitat-climate proxy. Wind points in the
/// direction air travels. These values are design inputs, not measured climate.
public struct SanctuaryEnvironmentRecipe: Equatable, Sendable {
  public let prevailingWindDirection: SIMD2<Float>
  public let upwindProbeCount: Int
  public let upwindProbeSpacingMetres: Float
  public let orographicReliefMetres: Float
  public let orographicMoistureStrength: Float
  public let seaLevelTemperatureProxy: Float
  public let lapsePerKilometre: Float

  public init(
    prevailingWindDirection: SIMD2<Float>, upwindProbeCount: Int = 6,
    upwindProbeSpacingMetres: Float = 320, orographicReliefMetres: Float = 420,
    orographicMoistureStrength: Float = 0.28, seaLevelTemperatureProxy: Float = 0.72,
    lapsePerKilometre: Float = 0.16
  ) throws {
    let magnitude = length(prevailingWindDirection)
    guard prevailingWindDirection.x.isFinite, prevailingWindDirection.y.isFinite,
      magnitude.isFinite, magnitude > 0.0001,
      (2...8).contains(upwindProbeCount),
      upwindProbeSpacingMetres.isFinite, (80...1_200).contains(upwindProbeSpacingMetres),
      orographicReliefMetres.isFinite, (50...2_000).contains(orographicReliefMetres),
      orographicMoistureStrength.isFinite, (0...0.5).contains(orographicMoistureStrength),
      seaLevelTemperatureProxy.isFinite, (0...1).contains(seaLevelTemperatureProxy),
      lapsePerKilometre.isFinite, (0...0.4).contains(lapsePerKilometre)
    else { throw SanctuaryEnvironmentFieldError.invalidRecipe }
    self.prevailingWindDirection = prevailingWindDirection / magnitude
    self.upwindProbeCount = upwindProbeCount
    self.upwindProbeSpacingMetres = upwindProbeSpacingMetres
    self.orographicReliefMetres = orographicReliefMetres
    self.orographicMoistureStrength = orographicMoistureStrength
    self.seaLevelTemperatureProxy = seaLevelTemperatureProxy
    self.lapsePerKilometre = lapsePerKilometre
  }

  /// Gentle southwest-to-northeast transport retains the current authored wet
  /// and dry regions while allowing real terrain to make local windward/lee differences.
  public static let sanctuary: SanctuaryEnvironmentRecipe = {
    // Constants above satisfy the public finite bounds by construction.
    try! SanctuaryEnvironmentRecipe(prevailingWindDirection: SIMD2(0.82, 0.57))
  }()
}

/// Immutable environmental facts at one world coordinate.
///
/// Elevation and signed water coverage are metres. `gradeRisePerRun` is clipped
/// to 0...4 m/m and `orographicExposure` is a signed -1...1 proxy. Moisture,
/// temperature, proximity, retention, wetness and soft-soil values are in 0...1.
/// Habitat values do not depend on live rain; surface wetness and soft soil do.
public struct SanctuaryEnvironmentSample: Equatable, Sendable {
  public let coordinate: SIMD2<Float>
  public let elevationMetres: Float
  public let gradeRisePerRun: Float
  public let habitatMoisture: Float
  public let habitatTemperature: Float
  public let orographicExposure: Float
  public let nearestWaterSignedCoverageMetres: Float
  public let waterProximity: Float
  public let soilRetention: Float
  public let surfaceWetness: Float
  public let softSoilSuitability: Float
  public let primaryBiome: SanctuaryBiome
  public let isWater: Bool
}

/// Stable habitat-scale values. This sample ignores live rain and may be cached
/// by callers whose terrain and environment recipe have not changed.
public struct SanctuaryHabitatClimateSample: Equatable, Sendable {
  public let coordinate: SIMD2<Float>
  public let elevationMetres: Float
  public let gradeRisePerRun: Float
  public let habitatMoisture: Float
  public let habitatTemperature: Float
  public let orographicExposure: Float
  public let primaryBiome: SanctuaryBiome
}

/// Local substrate and current surface response. This path needs no upwind
/// terrain probes when its caller already owns the resolved support and grade.
public struct SanctuarySurfaceEnvironmentSample: Equatable, Sendable {
  public let coordinate: SIMD2<Float>
  public let supportingHeightMetres: Float
  public let gradeRisePerRun: Float
  public let nearestWaterSignedCoverageMetres: Float
  public let waterProximity: Float
  public let soilRetention: Float
  public let surfaceWetness: Float
  public let softSoilSuitability: Float
  public let primaryBiome: SanctuaryBiome
  public let isWater: Bool
}

/// Pure Sanctuary-owned environmental source. The default query performs five
/// center height evaluations through `Terrain.sample`, six bounded upwind height
/// evaluations, and five water-field evaluations reusing the center bed. It does
/// not allocate a world grid or solve drainage/weather in a frame loop.
public struct SanctuaryEnvironmentField: @unchecked Sendable {
  public static let drySurfaceWeather = SanctuaryClimate.Sample(
    timeOfDay: 0.5, cloud: 0, rain: 0, wind: 0)
  private static let waterBodies: [SanctuaryWaterBody] = [
    .lake, .wetland, .creek, .tidepool, .ocean,
  ]
  public let recipe: SanctuaryEnvironmentRecipe
  public let terrain: Terrain
  public let geography: SanctuaryGeography

  public init(
    recipe: SanctuaryEnvironmentRecipe = .sanctuary,
    terrain: Terrain = .init(), geography: SanctuaryGeography = .init()
  ) {
    self.recipe = recipe
    self.terrain = terrain
    self.geography = geography
  }

  /// Exact terrain height call counts. `Terrain.sample` uses five center calls;
  /// static climate adds the configured upwind probes. A surface sample supplied
  /// with support and grade performs no terrain height calls.
  public var terrainHeightQueryCount: Int { 5 + recipe.upwindProbeCount }
  public var habitatClimateTerrainHeightQueryCount: Int { 5 + recipe.upwindProbeCount }
  public static let suppliedSurfaceTerrainHeightQueryCount = 0
  public static let waterFieldQueryCount = 5

  public func sample(
    at coordinate: SIMD2<Float>,
    weather: SanctuaryClimate.Sample = SanctuaryEnvironmentField.drySurfaceWeather
  ) throws -> SanctuaryEnvironmentSample {
    try validateCoordinate(coordinate)
    guard Self.valid(weather) else { throw SanctuaryEnvironmentFieldError.invalidWeather }
    let terrainSample = terrain.sample(coordinate.x, coordinate.y)
    let elevation = terrainSample.height
    let horizontalNormal = length(SIMD2(terrainSample.normal.x, terrainSample.normal.z))
    let grade = min(4, horizontalNormal / max(0.001, terrainSample.normal.y))
    let site = try siteContext(at: coordinate, supportingHeight: elevation, grade: grade)
    let climate = habitatClimate(from: site)
    let surface = surfaceEnvironment(from: site, weather: weather)

    return SanctuaryEnvironmentSample(
      coordinate: coordinate, elevationMetres: elevation, gradeRisePerRun: grade,
      habitatMoisture: climate.habitatMoisture,
      habitatTemperature: climate.habitatTemperature,
      orographicExposure: climate.orographicExposure,
      nearestWaterSignedCoverageMetres: surface.nearestWaterSignedCoverageMetres,
      waterProximity: surface.waterProximity, soilRetention: surface.soilRetention,
      surfaceWetness: surface.surfaceWetness,
      softSoilSuitability: surface.softSoilSuitability,
      primaryBiome: site.biome.primary, isWater: site.isWater)
  }

  /// Stable climate path. Default configuration performs eleven terrain height
  /// calls and five water-field queries; it is intended for cached habitat work.
  public func habitatClimate(
    at coordinate: SIMD2<Float>
  ) throws -> SanctuaryHabitatClimateSample {
    try validateCoordinate(coordinate)
    let terrainSample = terrain.sample(coordinate.x, coordinate.y)
    let horizontalNormal = length(SIMD2(terrainSample.normal.x, terrainSample.normal.z))
    let grade = min(4, horizontalNormal / max(0.001, terrainSample.normal.y))
    return habitatClimate(from: try siteContext(
      at: coordinate, supportingHeight: terrainSample.height, grade: grade))
  }

  /// Lightweight current-surface path for collision/support callers. It makes
  /// zero terrain-height calls and exactly five water-field queries.
  public func surfaceSample(
    at coordinate: SIMD2<Float>, supportingHeightMetres: Float,
    gradeRisePerRun: Float, weather: SanctuaryClimate.Sample
  ) throws -> SanctuarySurfaceEnvironmentSample {
    guard supportingHeightMetres.isFinite, abs(supportingHeightMetres) <= 1_000,
      gradeRisePerRun.isFinite, (0...4).contains(gradeRisePerRun)
    else { throw SanctuaryEnvironmentFieldError.invalidSurface }
    guard Self.valid(weather) else { throw SanctuaryEnvironmentFieldError.invalidWeather }
    return surfaceEnvironment(from: try siteContext(
      at: coordinate, supportingHeight: supportingHeightMetres, grade: gradeRisePerRun),
      weather: weather)
  }

  private struct SiteContext {
    let coordinate: SIMD2<Float>
    let elevation: Float
    let grade: Float
    let biome: SanctuaryBiomeSample
    let nearestWaterCoverage: Float
    let waterProximity: Float
    let biomeMoisture: Float
    let biomeTemperature: Float
    let soilRetention: Float
    let isWater: Bool
  }

  private func siteContext(
    at coordinate: SIMD2<Float>, supportingHeight: Float, grade: Float
  ) throws -> SiteContext {
    try validateCoordinate(coordinate)
    let biome = try geography.sample(at: coordinate)

    var nearestCoverage = -Float.greatestFiniteMagnitude
    var isWater = false
    for body in Self.waterBodies {
      guard let field = terrain.waterField(
        for: body, at: coordinate, bedHeight: supportingHeight) else { continue }
      nearestCoverage = max(nearestCoverage, field.signedCoverage)
      if !isWater, field.isWet { isWater = true }
    }
    let finiteCoverage = nearestCoverage.isFinite ? nearestCoverage : -32_000
    let waterProximity = exp(-max(0, -finiteCoverage) / 140)

    var biomeMoisture: Float = 0
    var biomeTemperature: Float = 0
    var biomeRetention: Float = 0
    for namedBiome in SanctuaryBiome.allCases {
      let weight = biome.weights[namedBiome] ?? 0
      biomeMoisture += weight * Self.moistureAffinity(namedBiome)
      biomeTemperature += weight * Self.temperatureAffinity(namedBiome)
      biomeRetention += weight * Self.retentionAffinity(namedBiome)
    }
    let flatRetention = 1 - Self.smoothstep(0.08, 0.75, grade)
    let soilRetention = Self.unit(
      biomeRetention * 0.58 + flatRetention * 0.27 + waterProximity * 0.15)
    return SiteContext(
      coordinate: coordinate, elevation: supportingHeight, grade: grade, biome: biome,
      nearestWaterCoverage: finiteCoverage, waterProximity: waterProximity,
      biomeMoisture: biomeMoisture, biomeTemperature: biomeTemperature,
      soilRetention: soilRetention, isWater: isWater)
  }

  private func habitatClimate(from site: SiteContext) -> SanctuaryHabitatClimateSample {

    var exposure: Float = 0
    var exposureWeight: Float = 0
    for index in 1...recipe.upwindProbeCount {
      let distance = Float(index) * recipe.upwindProbeSpacingMetres
      let probe = SanctuaryGeography.bounds.clamped(
        site.coordinate - recipe.prevailingWindDirection * distance)
      let upstreamHeight = terrain.height(probe.x, probe.y)
      let weight = 1 / Float(index)
      exposure += min(1, max(-1,
        (site.elevation - upstreamHeight) / recipe.orographicReliefMetres)) * weight
      exposureWeight += weight
    }
    let normalizedExposure = exposure / exposureWeight
    let habitatMoisture = Self.unit(
      site.biomeMoisture + normalizedExposure * recipe.orographicMoistureStrength
        + site.waterProximity * 0.10)
    let habitatTemperature = Self.unit(
      recipe.seaLevelTemperatureProxy * 0.55 + site.biomeTemperature * 0.45
        - max(0, site.elevation) / 1_000 * recipe.lapsePerKilometre)
    return SanctuaryHabitatClimateSample(
      coordinate: site.coordinate, elevationMetres: site.elevation,
      gradeRisePerRun: site.grade,
      habitatMoisture: habitatMoisture, habitatTemperature: habitatTemperature,
      orographicExposure: normalizedExposure, primaryBiome: site.biome.primary)
  }

  private func surfaceEnvironment(
    from site: SiteContext, weather: SanctuaryClimate.Sample
  ) -> SanctuarySurfaceEnvironmentSample {
    // Live rain affects only surface response. Nearby open water contributes
    // dampness without altering stable habitat climate at query time.
    let surfaceWetness = site.isWater
      ? 1 : Self.unit(max(site.waterProximity * 0.58,
        weather.rain * (0.42 + 0.58 * site.soilRetention)))
    let softSoil = site.isWater
      ? 0 : Self.unit(site.soilRetention * (0.30 + 0.70 * surfaceWetness))
    return SanctuarySurfaceEnvironmentSample(
      coordinate: site.coordinate, supportingHeightMetres: site.elevation,
      gradeRisePerRun: site.grade,
      nearestWaterSignedCoverageMetres: site.nearestWaterCoverage,
      waterProximity: site.waterProximity, soilRetention: site.soilRetention,
      surfaceWetness: surfaceWetness, softSoilSuitability: softSoil,
      primaryBiome: site.biome.primary, isWater: site.isWater)
  }

  private func validateCoordinate(_ coordinate: SIMD2<Float>) throws {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      SanctuaryGeography.bounds.contains(coordinate)
    else { throw SanctuaryEnvironmentFieldError.invalidCoordinate }
  }

  private static func valid(_ sample: SanctuaryClimate.Sample) -> Bool {
    sample.timeOfDay.isFinite && (0..<1).contains(sample.timeOfDay)
      && [sample.cloud, sample.rain, sample.wind].allSatisfy {
        $0.isFinite && (0...1).contains($0)
      }
  }

  private static func moistureAffinity(_ biome: SanctuaryBiome) -> Float {
    switch biome {
    case .woodland: return 0.62
    case .meadow: return 0.48
    case .creek: return 0.82
    case .wetland: return 0.94
    case .lake: return 0.78
    case .alpine: return 0.32
    case .desert: return 0.12
    case .rainforest: return 0.92
    case .coast: return 0.42
    case .tidepool: return 0.68
    case .ocean: return 0.72
    }
  }

  private static func temperatureAffinity(_ biome: SanctuaryBiome) -> Float {
    switch biome {
    case .woodland: return 0.57
    case .meadow: return 0.64
    case .creek: return 0.54
    case .wetland: return 0.59
    case .lake: return 0.50
    case .alpine: return 0.18
    case .desert: return 0.90
    case .rainforest: return 0.84
    case .coast: return 0.66
    case .tidepool: return 0.62
    case .ocean: return 0.58
    }
  }

  private static func retentionAffinity(_ biome: SanctuaryBiome) -> Float {
    switch biome {
    case .woodland: return 0.68
    case .meadow: return 0.58
    case .creek: return 0.82
    case .wetland: return 0.94
    case .lake: return 0.44
    case .alpine: return 0.20
    case .desert: return 0.10
    case .rainforest: return 0.88
    case .coast: return 0.28
    case .tidepool: return 0.52
    case .ocean: return 0.05
    }
  }

  private static func unit(_ value: Float) -> Float { min(1, max(0, value)) }
  private static func smoothstep(_ lower: Float, _ upper: Float, _ value: Float) -> Float {
    let t = unit((value - lower) / (upper - lower))
    return t * t * (3 - 2 * t)
  }
}
