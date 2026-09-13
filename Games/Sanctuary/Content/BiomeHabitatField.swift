import Foundation
import simd

/// Vegetation categories describe game-owned placement intent, independent of a render mesh.
public enum SanctuaryVegetationSpecies: String, CaseIterable, Codable, Hashable, Sendable {
  case broadleaf, willow, palm, conifer, cactus, reeds
}

/// Continuous environmental inputs plus exact placement exclusions at one world coordinate.
public struct SanctuaryBiomeHabitatSample: Equatable, Sendable {
  public let coordinate: SIMD2<Float>
  public let biomeWeights: [SanctuaryBiome: Float]
  public let habitatMoisture: Float
  public let habitatTemperature: Float
  public let orographicExposure: Float
  public let riparianPotential: Float
  public let terrainSuitability: Float
  public let routeSuitability: Float
  public let speciesSuitability: [SanctuaryVegetationSpecies: Float]
  public let isInsideWorld: Bool
  public let isWater: Bool
  public let isCabinReserved: Bool

  public var permitsPlacement: Bool {
    isInsideWorld && !isWater && !isCabinReserved
  }

  public func suitability(for species: SanctuaryVegetationSpecies) -> Float {
    speciesSuitability[species] ?? 0
  }
}

/// Pure game-owned habitat source. Presentation and collision consume placement records derived
/// from this field; neither may introduce a separate random population.
/// `Terrain` and `SanctuaryGeography` are immutable value sources. The unchecked marker records
/// that fact until the existing `HeightField` hierarchy adopts `Sendable` throughout.
public struct SanctuaryBiomeHabitatField: @unchecked Sendable {
  public let geography: SanctuaryGeography
  public let terrain: Terrain
  public let environment: SanctuaryEnvironmentField

  public init(
    geography: SanctuaryGeography = .init(), terrain: Terrain = .init(),
    environmentRecipe: SanctuaryEnvironmentRecipe = .sanctuary
  ) {
    self.geography = geography
    self.terrain = terrain
    environment = SanctuaryEnvironmentField(
      recipe: environmentRecipe, terrain: terrain, geography: geography)
  }

  public func sample(at coordinate: SIMD2<Float>) throws -> SanctuaryBiomeHabitatSample {
    let inside = SanctuaryGeography.bounds.contains(coordinate)
    guard inside else {
      let biomeSample = try geography.sample(at: coordinate)
      return SanctuaryBiomeHabitatSample(
        coordinate: coordinate, biomeWeights: biomeSample.weights,
        habitatMoisture: 0, habitatTemperature: 0, orographicExposure: 0,
        riparianPotential: 0,
        terrainSuitability: 0, routeSuitability: 0, speciesSuitability: [:],
        isInsideWorld: false, isWater: false, isCabinReserved: false)
    }
    return try sample(
      at: coordinate, communityClimate: environment.habitatClimate(at: coordinate))
  }

  /// Stable climate evaluated once for a vegetation community cache cell. Population generation
  /// shares this result across the sixteen candidates in one 128 m parent; live weather is absent.
  public func communityClimate(
    at coordinate: SIMD2<Float>
  ) throws -> SanctuaryHabitatClimateSample {
    try environment.habitatClimate(at: coordinate)
  }

  /// Candidate-local terrain, route and water eligibility combined with a cached static climate.
  /// The climate coordinate may be the center of the candidate's 128 m community parent.
  public func sample(
    at coordinate: SIMD2<Float>, communityClimate: SanctuaryHabitatClimateSample
  ) throws -> SanctuaryBiomeHabitatSample {
    let biomeSample = try geography.sample(at: coordinate)
    let inside = SanctuaryGeography.bounds.contains(coordinate)
    guard inside else {
      return SanctuaryBiomeHabitatSample(
        coordinate: coordinate, biomeWeights: biomeSample.weights,
        habitatMoisture: 0, habitatTemperature: 0, orographicExposure: 0,
        riparianPotential: 0,
        terrainSuitability: 0, routeSuitability: 0, speciesSuitability: [:],
        isInsideWorld: false, isWater: false, isCabinReserved: false)
    }

    let normalY = terrain.normal(coordinate.x, coordinate.y).y
    let terrainSuitability = Self.smoothstep(0.58, 0.94, normalY)
    let routeDistance = geography.distanceToRoute(at: coordinate)
    let routeSuitability = Self.smoothstep(8, 34, routeDistance)
    let creekCoverage = terrain.waterField(for: .creek, at: coordinate)?.signedCoverage
      ?? -Float.greatestFiniteMagnitude
    let dryCreekDistance = max(0, -creekCoverage)
    let riparianPotential = exp(
      -(dryCreekDistance * dryCreekDistance) / (2 * 72 * 72))

    func weight(_ biome: SanctuaryBiome) -> Float { biomeSample.weights[biome] ?? 0 }
    let woodland = Self.smoothstep(0.045, 0.28, weight(.woodland))
    let rainforest = Self.smoothstep(0.07, 0.43, weight(.rainforest))
    let creek = Self.smoothstep(0.035, 0.22, weight(.creek))
    let wetland = Self.smoothstep(0.045, 0.24, weight(.wetland))
    let alpine = Self.smoothstep(0.08, 0.56, weight(.alpine))
    let desert = Self.smoothstep(0.08, 0.68, weight(.desert))
    let ground = terrainSuitability * routeSuitability
    let moisture = communityClimate.habitatMoisture
    let temperature = communityClimate.habitatTemperature
    let temperateMoist = 0.72 + 0.28 * moisture
    let saturated = 0.66 + 0.34 * moisture
    let warmHumid = 0.58 + 0.22 * moisture + 0.20 * temperature
    let cold = 0.68 + 0.32 * (1 - temperature)
    let hotDry = 0.58 + 0.22 * (1 - moisture) + 0.20 * temperature
    let species: [SanctuaryVegetationSpecies: Float] = [
      .broadleaf: woodland * ground * temperateMoist,
      .willow: max(creek, wetland * 0.72) * riparianPotential * ground * saturated,
      .palm: rainforest * ground * warmHumid,
      .conifer: alpine * ground * cold,
      .cactus: desert * terrainSuitability * routeSuitability * hotDry,
      .reeds: max(wetland, creek * 0.68) * riparianPotential * ground * saturated,
    ]
    let cabinExtent = SanctuaryTerrainTessellation.cabinExtent
    let cabinReserved = abs(coordinate.x) <= cabinExtent && abs(coordinate.y) <= cabinExtent
    return SanctuaryBiomeHabitatSample(
      coordinate: coordinate, biomeWeights: biomeSample.weights,
      habitatMoisture: moisture, habitatTemperature: temperature,
      orographicExposure: communityClimate.orographicExposure,
      riparianPotential: riparianPotential, terrainSuitability: terrainSuitability,
      routeSuitability: routeSuitability, speciesSuitability: species,
      isInsideWorld: true, isWater: terrain.water(at: coordinate) != nil,
      isCabinReserved: cabinReserved)
  }

  private static func smoothstep(_ lower: Float, _ upper: Float, _ value: Float) -> Float {
    let t = min(1, max(0, (value - lower) / (upper - lower)))
    return t * t * (3 - 2 * t)
  }
}
