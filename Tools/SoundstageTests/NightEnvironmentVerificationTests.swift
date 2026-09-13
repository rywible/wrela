import XCTest
import simd
@testable import SoundstageKit

/// CPU definition and independent integration only. Metal parity is exercised by --check-renderer.
final class NightEnvironmentVerificationTests: XCTestCase {
  func testUnitRadianceDimsHorizonAndUsesClampedTransmission() {
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(0, 1, 0), transmission: 1), 1.15, accuracy: 0.000001)
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(1, 0, 0), transmission: 1), 0.1, accuracy: 0.000001)
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(0, -1, 0), transmission: 1), 0.18, accuracy: 0.000001)
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(0, 2, 0), transmission: -4), 0.2875, accuracy: 0.000001)
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(0, 1, 0), transmission: 4), 1.15, accuracy: 0.000001)
    XCTAssertEqual(NightEnvironmentVerification.unit(
      direction: SIMD3(0, 1, 0), transmission: .nan), 0.2875, accuracy: 0.000001)
  }

  func testFloorIsFiniteClampedAndDisabledIndoors() {
    XCTAssertEqual(NightEnvironmentVerification.coefficient(
      floor: .zero, outdoor: true), .zero)
    XCTAssertEqual(NightEnvironmentVerification.coefficient(
      floor: SIMD3(0.2, 0.1, 0.05), outdoor: false), .zero)
    let coefficient = NightEnvironmentVerification.coefficient(
      floor: SIMD3(-1, 0.1, 0.9), outdoor: true)
    XCTAssertEqual(coefficient.x, 0, accuracy: 0.000001)
    XCTAssertEqual(coefficient.y, 0.125, accuracy: 0.000001)
    XCTAssertEqual(coefficient.z, 0.3125, accuracy: 0.000001)
    let finite = NightEnvironmentVerification.coefficient(
      floor: SIMD3(.nan, .infinity, 0.04), outdoor: true)
    XCTAssertEqual(finite.x, 0)
    XCTAssertEqual(finite.y, 0)
    XCTAssertEqual(finite.z, 0.05, accuracy: 0.000001)
  }

  func testEvaluationKeepsUnitAlphaIndependentOfOutdoorAmplitude() {
    let outdoor = NightEnvironmentVerification.evaluate(.init(
      direction: SIMD3(0, 1, 0), transmission: 0.5,
      floor: SIMD3(0.2, 0.1, 0.05), outdoor: true))
    let indoor = NightEnvironmentVerification.evaluate(.init(
      direction: SIMD3(0, 1, 0), transmission: 0.5,
      floor: SIMD3(0.2, 0.1, 0.05), outdoor: false))
    XCTAssertEqual(outdoor.w, 0.71875, accuracy: 0.000001)
    XCTAssertEqual(indoor.w, outdoor.w, accuracy: 0.000001)
    XCTAssertEqual(indoor.x, 0)
    XCTAssertEqual(indoor.y, 0)
    XCTAssertEqual(indoor.z, 0)
    XCTAssertEqual(outdoor.x, 0.1796875, accuracy: 0.000001)
    XCTAssertEqual(outdoor.y, 0.08984375, accuracy: 0.000001)
    XCTAssertEqual(outdoor.z, 0.044921875, accuracy: 0.000001)
  }

  func testIndependentIntegrationPreservesUpDownEnergyAndRedistributesHorizontal() {
    let horizontal = Float(0.05 + 0.7 / Double.pi + 0.09)
    XCTAssertEqual(NightEnvironmentVerification.clearSkyIrradianceOverPi(
      normal: SIMD3(0, 1, 0)), 0.8, accuracy: 0.0001)
    XCTAssertEqual(NightEnvironmentVerification.clearSkyIrradianceOverPi(
      normal: SIMD3(0, -1, 0)), 0.18, accuracy: 0.0001)
    XCTAssertEqual(NightEnvironmentVerification.clearSkyIrradianceOverPi(
      normal: SIMD3(1, 0, 0)), horizontal, accuracy: 0.0001)
    XCTAssertEqual(NightEnvironmentVerification.clearSkyIrradianceOverPi(
      normal: .zero), 0)
    XCTAssertEqual(NightEnvironmentVerification.clearSkyIrradianceOverPi(
      normal: SIMD3(.nan, 0, 0)), 0)
  }
}
