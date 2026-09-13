import XCTest
import simd
@testable import FieldEngine

final class SkyCacheScheduleTests: XCTestCase {
  private func uniforms() -> Uniforms {
    Uniforms(viewProjection: matrix_identity_float4x4, lightVP: matrix_identity_float4x4,
      inverseVP: matrix_identity_float4x4, cameraTime: SIMD4(0, 1.7, 0, 20),
      sunExposure: SIMD4(0, 1, 0, 1), options: SIMD4(0, 1, 0, 0),
      sky: SIMD4(0.5, 1, 1, 4), environment: .zero)
  }

  func testPresentationInputsDoNotInvalidateCache() {
    let original = uniforms()
    var changed = original
    changed.cameraTime = SIMD4(100, 3, -40, 60)
    changed.viewProjection.columns.0.x = 2
    changed.sunExposure.w = 3
    changed.environment.y = 1
    changed.rigColor = SIMD4(0.1, 0.2, 0.1, 0)
    changed.lookGrade.x = 2
    var gate = SkyPublicationGate()
    gate.request(SkyCacheSignature(original))
    let generation = gate.desiredGeneration
    gate.request(SkyCacheSignature(changed))
    XCTAssertEqual(gate.desiredGeneration, generation)
  }

  func testDensityOwnershipSignatureExcludesSunButIncludesSeed() {
    let original = uniforms()
    var sun = original; sun.sunExposure.y = 0.1
    XCTAssertNotEqual(SkyCacheSignature(original), SkyCacheSignature(sun))
    XCTAssertEqual(SkyCacheSignature(original).density, SkyCacheSignature(sun).density)
    var seed = original; seed.sky.w = 5
    XCTAssertNotEqual(SkyCacheSignature(original).density, SkyCacheSignature(seed).density)
  }

  func testRapidRequestsPublishOnlyLatestSuccessfulCompleteGeneration() {
    var gate = SkyPublicationGate()
    var u = uniforms()
    gate.request(SkyCacheSignature(u)); let a = gate.desiredGeneration
    XCTAssertTrue(gate.complete(a, success: true, final: false))
    XCTAssertEqual(gate.activeGeneration, 0)
    u.sky.x = 0.7
    gate.request(SkyCacheSignature(u)); let b = gate.desiredGeneration
    u.sky.x = 0.9
    gate.request(SkyCacheSignature(u)); let c = gate.desiredGeneration
    XCTAssertFalse(gate.complete(a, success: true, final: true))
    XCTAssertFalse(gate.complete(b, success: true, final: true))
    XCTAssertEqual(gate.activeGeneration, 0)
    XCTAssertTrue(gate.complete(c, success: true, final: false))
    XCTAssertEqual(gate.publications, 0)
    XCTAssertTrue(gate.complete(c, success: true, final: true))
    XCTAssertEqual(gate.activeGeneration, c)
    XCTAssertEqual(gate.publications, 1)
    XCTAssertEqual(gate.retired, 2)
  }

  func testFailedStepPreservesExistingPublication() {
    var gate = SkyPublicationGate()
    var u = uniforms()
    gate.request(SkyCacheSignature(u)); let active = gate.desiredGeneration
    XCTAssertTrue(gate.complete(active, success: true, final: true))
    u.sky.z = 2
    gate.request(SkyCacheSignature(u))
    XCTAssertFalse(gate.complete(gate.desiredGeneration, success: false, final: true))
    XCTAssertEqual(gate.activeGeneration, active)
    XCTAssertEqual(gate.publications, 1)
  }

  func testExplicitRestoreInvalidatesEqualSignature() {
    var gate = SkyPublicationGate()
    let signature = SkyCacheSignature(uniforms())
    gate.request(signature); let old = gate.desiredGeneration
    gate.request(signature, invalidate: true)
    XCTAssertGreaterThan(gate.desiredGeneration, old)
    XCTAssertFalse(gate.complete(old, success: true, final: true))
  }

  func testPublishedLightingKeepsSceneCameraExposureAndWetnessLive() {
    let published = SkyCacheSignature(uniforms())
    var requested = uniforms()
    requested.sunExposure = SIMD4(-0.56, 0.15, -0.81, 2.5)
    requested.sky = SIMD4(0.8, 1.2, 3, 8)
    requested.options.y = 0.5
    requested.environment.x = 5
    requested.environment.y = 0.75
    requested.cameraTime = SIMD4(130, 4, -90, 42)
    requested.rigColor = SIMD4(0.1, 0.1, 0.1, 0)
    published.applyLighting(to: &requested)
    XCTAssertEqual(SkyCacheSignature(requested), published)
    XCTAssertEqual(requested.sunExposure.w, 2.5)
    XCTAssertEqual(requested.environment.y, 0.75)
    XCTAssertEqual(requested.cameraTime, SIMD4(130, 4, -90, 42))
    XCTAssertEqual(requested.rigColor.x, 0.1)
  }

  func testPublishedSunShadowUsesCurrentSceneCenter() {
    let sun = simd_normalize(SIMD3<Float>(-0.46, 0.59, -0.66))
    let oldCenter = SIMD3<Float>(0, 0, -20)
    let currentCenter = SIMD3<Float>(100, 12, -65)
    let shadow = OutdoorShadow(center: currentCenter, size: 65, near: 1, far: 250, distance: 110)
    let matrix = FrameComposer.outdoorShadowMatrix(shadow, sun: sun)
    let center = matrix * SIMD4(currentCenter, 1)
    XCTAssertLessThanOrEqual(abs(center.x), 0.5 / 1024 + 0.000001)
    XCTAssertLessThanOrEqual(abs(center.y), 0.5 / 1024 + 0.000001)
    // Light rays stay parallel to the published sun after the camera moves.
    let ray = matrix * SIMD4(sun, 0)
    XCTAssertEqual(ray.x, 0, accuracy: 0.000001)
    XCTAssertEqual(ray.y, 0, accuracy: 0.000001)
    let stale = matrix * SIMD4(oldCenter, 1)
    XCTAssertGreaterThan(abs(stale.x) + abs(stale.y), 0.1)
  }

  func testOrdinaryPauseFreezesWithoutAnExactRequest() {
    var policy = SkyCacheDeliveryPolicy()
    let u = uniforms()
    policy.observe(u, paused: false)
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .frozen)
    XCTAssertFalse(policy.pendingAuthoring)
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .frozen)
  }

  func testSkippedLiveFramesCannotTurnPauseIntoATimeEdit() {
    var policy = SkyCacheDeliveryPolicy()
    var u = uniforms()
    policy.observe(u, paused: false)
    // Source observation also occurs for frames that never acquire a drawable.
    u.cameraTime.w += 0.05
    policy.observe(u, paused: false)
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .frozen)
  }

  func testPausedTimeAndLightingEditsRemainPendingUntilEncoded() {
    var policy = SkyCacheDeliveryPolicy()
    var u = uniforms()
    policy.observe(u, paused: true)
    u.cameraTime.w += 1 / 60
    policy.observe(u, paused: true)
    // A dropped drawable must not consume the author's pending exact preview.
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .exactPaused)
    policy.encodedExact()
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .frozen)
    u.sunExposure.y = 0.3
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .exactPaused)
  }

  func testPausedCaptureIsExactButLiveCaptureKeepsLiveDelivery() {
    var policy = SkyCacheDeliveryPolicy()
    policy.observe(uniforms(), paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: true, invalidated: false), .exactPaused)
    policy.encodedExact()
    XCTAssertEqual(policy.delivery(paused: true, capture: true, invalidated: false), .exactPaused)
    XCTAssertEqual(policy.delivery(paused: false, capture: true, invalidated: false), .live)
  }

  func testPausedRestoreRequestsExactButLiveRestoreUsesScheduler() {
    let policy = SkyCacheDeliveryPolicy()
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: true), .exactPaused)
    XCTAssertEqual(policy.delivery(paused: false, capture: false, invalidated: true), .live)
  }

  func testPausedCameraExposureAndWetnessDoNotRebuildSky() {
    var policy = SkyCacheDeliveryPolicy()
    var u = uniforms()
    policy.observe(u, paused: true)
    u.cameraTime.x = 5; u.cameraTime.z = -10
    u.sunExposure.w = 2; u.environment.y = 1
    policy.observe(u, paused: true)
    XCTAssertEqual(policy.delivery(paused: true, capture: false, invalidated: false), .frozen)
  }

  func testResumeUsesBoundedDeliveryAfterUnencodedAuthoringChange() {
    var policy = SkyCacheDeliveryPolicy()
    var u = uniforms()
    policy.observe(u, paused: true)
    u.sky.w += 1
    policy.observe(u, paused: true)
    XCTAssertTrue(policy.pendingAuthoring)
    policy.observe(u, paused: false)
    XCTAssertFalse(policy.pendingAuthoring)
    XCTAssertEqual(policy.delivery(paused: false, capture: false, invalidated: false), .live)
  }

}
