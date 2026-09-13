import XCTest
import simd

@testable import SanctuaryContent

final class SanctuaryEnvironmentFieldTests: XCTestCase {
  private let dry = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 0.1, rain: 0, wind: 0.2)

  func testSameCoordinateRecipeAndWeatherReplayExactly() throws {
    let recipe = try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(3, 2), upwindProbeCount: 5,
      upwindProbeSpacingMetres: 280)
    let field = SanctuaryEnvironmentField(recipe: recipe)
    let point = SIMD2<Float>(8_145.25, -3_210.75)
    XCTAssertEqual(try field.sample(at: point, weather: dry),
      try field.sample(at: point, weather: dry))
    XCTAssertEqual(field.terrainHeightQueryCount, 10)
    XCTAssertEqual(field.habitatClimateTerrainHeightQueryCount, 10)
    XCTAssertEqual(SanctuaryEnvironmentField.suppliedSurfaceTerrainHeightQueryCount, 0)
    XCTAssertEqual(SanctuaryEnvironmentField.waterFieldQueryCount, 5)
  }

  func testFullSampleComposesExactClimateAndSuppliedSurfaceSources() throws {
    let field = SanctuaryEnvironmentField()
    let point = SIMD2<Float>(1_745.25, 2_145.75)
    let weather = SanctuaryClimate.Sample(timeOfDay: 0.7, cloud: 0.8, rain: 0.62, wind: 0.5)
    let full = try field.sample(at: point, weather: weather)
    let climate = try field.habitatClimate(at: point)
    let surface = try field.surfaceSample(
      at: point, supportingHeightMetres: full.elevationMetres,
      gradeRisePerRun: full.gradeRisePerRun, weather: weather)

    XCTAssertEqual(climate.coordinate, full.coordinate)
    XCTAssertEqual(climate.elevationMetres, full.elevationMetres)
    XCTAssertEqual(climate.gradeRisePerRun, full.gradeRisePerRun)
    XCTAssertEqual(climate.habitatMoisture, full.habitatMoisture)
    XCTAssertEqual(climate.habitatTemperature, full.habitatTemperature)
    XCTAssertEqual(climate.orographicExposure, full.orographicExposure)
    XCTAssertEqual(surface.coordinate, full.coordinate)
    XCTAssertEqual(surface.supportingHeightMetres, full.elevationMetres)
    XCTAssertEqual(surface.gradeRisePerRun, full.gradeRisePerRun)
    XCTAssertEqual(surface.nearestWaterSignedCoverageMetres,
      full.nearestWaterSignedCoverageMetres)
    XCTAssertEqual(surface.waterProximity, full.waterProximity)
    XCTAssertEqual(surface.soilRetention, full.soilRetention)
    XCTAssertEqual(surface.surfaceWetness, full.surfaceWetness)
    XCTAssertEqual(surface.softSoilSuitability, full.softSoilSuitability)
    XCTAssertEqual(surface.primaryBiome, full.primaryBiome)
    XCTAssertEqual(surface.isWater, full.isWater)
  }

  func testSuppliedSurfaceRejectsInvalidSupportGradeAndWeather() throws {
    let field = SanctuaryEnvironmentField()
    let point = SIMD2<Float>(40, 40)
    XCTAssertThrowsError(try field.surfaceSample(
      at: point, supportingHeightMetres: .nan, gradeRisePerRun: 0.2, weather: dry)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidSurface)
    }
    XCTAssertThrowsError(try field.surfaceSample(
      at: point, supportingHeightMetres: 0, gradeRisePerRun: -0.01, weather: dry)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidSurface)
    }
    XCTAssertThrowsError(try field.surfaceSample(
      at: point, supportingHeightMetres: 0, gradeRisePerRun: 4.01, weather: dry)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidSurface)
    }
    let invalid = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 0, rain: 1.01, wind: 0)
    XCTAssertThrowsError(try field.surfaceSample(
      at: point, supportingHeightMetres: 0, gradeRisePerRun: 0, weather: invalid)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidWeather)
    }
  }

  func testFiniteBoundedSamplesAndInvalidInputs() throws {
    let field = SanctuaryEnvironmentField()
    let points: [SIMD2<Float>] = [
      SanctuaryGeography.bounds.minimum, SanctuaryGeography.bounds.maximum, .zero,
      SIMD2(-420, 10_675), SIMD2(-8_575, 5_775), SIMD2(12_950, -9_520),
    ]
    for point in points {
      let sample = try field.sample(at: point, weather: dry)
      XCTAssertTrue(sample.elevationMetres.isFinite)
      XCTAssertTrue(sample.nearestWaterSignedCoverageMetres.isFinite)
      XCTAssertTrue((0...4).contains(sample.gradeRisePerRun))
      XCTAssertTrue((-1...1).contains(sample.orographicExposure))
      for value in [sample.habitatMoisture, sample.habitatTemperature,
        sample.waterProximity, sample.soilRetention, sample.surfaceWetness,
        sample.softSoilSuitability] {
        XCTAssertTrue(value.isFinite && (0...1).contains(value), "\(point): \(value)")
      }
    }
    XCTAssertThrowsError(try field.sample(at: SIMD2(.nan, 0), weather: dry)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidCoordinate)
    }
    XCTAssertThrowsError(try field.sample(at: SIMD2(16_001, 0), weather: dry)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidCoordinate)
    }
    let invalidWeather = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 0, rain: .nan, wind: 0)
    XCTAssertThrowsError(try field.sample(at: .zero, weather: invalidWeather)) {
      XCTAssertEqual($0 as? SanctuaryEnvironmentFieldError, .invalidWeather)
    }
    XCTAssertThrowsError(try SanctuaryEnvironmentRecipe(prevailingWindDirection: .zero))
    XCTAssertThrowsError(try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(1, 0), upwindProbeCount: 9))
  }

  func testWindReversalChangesWindwardAndLeeMoistureWithoutChangingTerrain() throws {
    let east = try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(1, 0), upwindProbeSpacingMetres: 320,
      orographicMoistureStrength: 0.38)
    let west = try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(-1, 0), upwindProbeSpacingMetres: 320,
      orographicMoistureStrength: 0.38)
    let eastward = SanctuaryEnvironmentField(recipe: east)
    let westward = SanctuaryEnvironmentField(recipe: west)
    let alpine = SanctuaryGeography().landmark(for: .alpine).coordinate
    let points = stride(from: 0, to: 12, by: 1).map { index -> SIMD2<Float> in
      let angle = Float(index) / 12 * 2 * .pi
      return alpine + SIMD2(cos(angle), sin(angle)) * 1_100
    }
    var moistureEffects: [Float] = []
    for point in points {
      let withEastWind = try eastward.sample(at: point, weather: dry)
      let withWestWind = try westward.sample(at: point, weather: dry)
      XCTAssertEqual(withEastWind.elevationMetres, withWestWind.elevationMetres)
      XCTAssertEqual(withEastWind.gradeRisePerRun, withWestWind.gradeRisePerRun)
      moistureEffects.append(withEastWind.habitatMoisture - withWestWind.habitatMoisture)
    }
    XCTAssertGreaterThan(moistureEffects.max() ?? 0, 0.01,
      "Eastward flow should moisten at least one windward alpine approach")
    XCTAssertLessThan(moistureEffects.min() ?? 0, -0.01,
      "Wind reversal should move the lee response to the opposing approach")
  }

  func testRainChangesLiveSurfaceButNotStableHabitat() throws {
    let field = SanctuaryEnvironmentField()
    let point = SanctuaryGeography().landmark(for: .desert).coordinate + SIMD2<Float>(420, -260)
    let clear = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 0.1, rain: 0, wind: 0.3)
    let rain = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 1, rain: 1, wind: 0.8)
    let before = try field.sample(at: point, weather: clear)
    let during = try field.sample(at: point, weather: rain)
    XCTAssertEqual(before.elevationMetres, during.elevationMetres)
    XCTAssertEqual(before.gradeRisePerRun, during.gradeRisePerRun)
    XCTAssertEqual(before.habitatMoisture, during.habitatMoisture)
    XCTAssertEqual(before.habitatTemperature, during.habitatTemperature)
    XCTAssertEqual(before.soilRetention, during.soilRetention)
    XCTAssertGreaterThan(during.surfaceWetness, before.surfaceWetness)
    if !before.isWater {
      XCTAssertGreaterThan(during.softSoilSuitability, before.softSoilSuitability)
    }
  }

  func testChunkBoundaryQueryIsContinuousAndIndependentOfTiling() throws {
    let field = SanctuaryEnvironmentField()
    let left = try field.sample(at: SIMD2(511.99, 1_024.01), weather: dry)
    let right = try field.sample(at: SIMD2(512.01, 1_023.99), weather: dry)
    XCTAssertLessThan(abs(left.elevationMetres - right.elevationMetres), 0.2)
    XCTAssertLessThan(abs(left.habitatMoisture - right.habitatMoisture), 0.01)
    XCTAssertLessThan(abs(left.habitatTemperature - right.habitatTemperature), 0.01)
    XCTAssertLessThan(abs(left.waterProximity - right.waterProximity), 0.01)
    XCTAssertLessThan(abs(left.soilRetention - right.soilRetention), 0.01)
  }

  func testBiomeAffinitiesRetainWetAndDryAuthoredIntent() throws {
    let field = SanctuaryEnvironmentField()
    let geography = SanctuaryGeography()
    let rainforest = try field.sample(
      at: geography.landmark(for: .rainforest).coordinate, weather: dry)
    let desert = try field.sample(at: geography.landmark(for: .desert).coordinate, weather: dry)
    let alpine = try field.sample(at: geography.landmark(for: .alpine).coordinate, weather: dry)
    XCTAssertGreaterThan(rainforest.habitatMoisture, desert.habitatMoisture)
    XCTAssertGreaterThan(rainforest.soilRetention, desert.soilRetention)
    // Both authored warm regions remain warmer than alpine. Their close ordering is allowed
    // to follow their real landmark elevations and overlapping biome affinities.
    XCTAssertGreaterThan(desert.habitatTemperature, alpine.habitatTemperature)
    XCTAssertGreaterThan(rainforest.habitatTemperature, alpine.habitatTemperature)

    let noLapseRecipe = try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(0.82, 0.57), lapsePerKilometre: 0)
    let strongLapseRecipe = try SanctuaryEnvironmentRecipe(
      prevailingWindDirection: SIMD2(0.82, 0.57), lapsePerKilometre: 0.30)
    let noLapse = try SanctuaryEnvironmentField(recipe: noLapseRecipe).sample(
      at: geography.landmark(for: .alpine).coordinate, weather: dry)
    let strongLapse = try SanctuaryEnvironmentField(recipe: strongLapseRecipe).sample(
      at: geography.landmark(for: .alpine).coordinate, weather: dry)
    XCTAssertEqual(noLapse.elevationMetres, strongLapse.elevationMetres)
    XCTAssertEqual(noLapse.gradeRisePerRun, strongLapse.gradeRisePerRun)
    XCTAssertLessThan(strongLapse.habitatTemperature, noLapse.habitatTemperature)

    let creek = geography.landmark(for: .creek).coordinate
    let water = try field.sample(at: creek, weather: dry)
    if water.isWater { XCTAssertEqual(water.softSoilSuitability, 0) }
  }
}
