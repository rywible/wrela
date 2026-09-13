import SanctuaryContent
import XCTest

final class SanctuaryClimateTests: XCTestCase {
  func testSamplesAreFiniteAndBoundedAcrossSeedsAndDays() {
    for seed: UInt32 in [0, 17, 4_294_967_295] {
      for seconds in stride(from: -120.0, through: SanctuaryClimate.dayDuration * 3, by: 31.0) {
        let sample = SanctuaryClimate.sample(elapsedPlaySeconds: seconds, seed: seed)
        XCTAssertTrue(sample.timeOfDay.isFinite && sample.cloud.isFinite && sample.rain.isFinite && sample.wind.isFinite)
        XCTAssertTrue((0..<1).contains(sample.timeOfDay))
        XCTAssertTrue((0...1).contains(sample.cloud))
        XCTAssertTrue((0...1).contains(sample.rain))
        XCTAssertTrue((0...1).contains(sample.wind))
      }
    }
  }

  func testSameSavedElapsedTimeAndSeedReplayExactly() {
    let first = SanctuaryClimate.sample(elapsedPlaySeconds: 1_237.5, seed: 91)
    XCTAssertEqual(first, SanctuaryClimate.sample(elapsedPlaySeconds: 1_237.5, seed: 91))
    XCTAssertEqual(
      SanctuaryClimate.sample(elapsedPlaySeconds: 0, seed: 91),
      SanctuaryClimate.sample(elapsedPlaySeconds: SanctuaryClimate.dayDuration, seed: 91))
  }

  func testWeatherChangesSmoothlyAcrossDayWrap() {
    let before = SanctuaryClimate.sample(elapsedPlaySeconds: SanctuaryClimate.dayDuration - 1, seed: 17)
    let after = SanctuaryClimate.sample(elapsedPlaySeconds: 1, seed: 17)
    let directTimeDifference = abs(before.timeOfDay - after.timeOfDay)
    XCTAssertLessThan(min(directTimeDifference, 1 - directTimeDifference), 0.002)
    XCTAssertLessThan(abs(before.cloud - after.cloud), 0.01)
    XCTAssertLessThan(abs(before.rain - after.rain), 0.01)
    XCTAssertLessThan(abs(before.wind - after.wind), 0.01)
  }

  func testOpeningStartsInLateMorningWithoutReadingWallClock() {
    let sample = SanctuaryClimate.sample(elapsedPlaySeconds: 0, seed: 17)
    XCTAssertEqual(sample.timeOfDay, SanctuaryClimate.openingTimeOfDay)
  }
}
