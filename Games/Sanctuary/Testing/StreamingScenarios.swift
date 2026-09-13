import FieldCore
import Foundation
import SanctuaryContent
import SimulationCore
import TestKit
import simd

/// Bounded streaming scenarios that SanctuaryTesting can register beside its ordinary routes.
/// Fixture staging is intentionally separate from the recorded boundary movement below.
public enum SanctuaryStreamingScenarios {
  public static let alpineFixture = "streaming-alpine-rider"
  public static let creekFixture = "streaming-creek-bank"

  /// Source-owned collision neighborhoods before and after crossing north over z = 10,752.
  /// SanctuaryWorld publishes these as a stable semantic observation for the CPU route; native
  /// captures separately retain the presentation's far/detail replacement evidence.
  public static let alpineBeforeChunkIDs = "-2:19,-1:19,0:19,-2:20,-1:20,0:20,-2:21,-1:21,0:21"
  public static let alpineAfterChunkIDs = "-2:20,-1:20,0:20,-2:21,-1:21,0:21,-2:22,-1:22,0:22"

  public static var fixtures: [String] { [alpineFixture, creekFixture] }

  public static var tests: [GameTest] {
    [
      GameTest("alpine-stream-boundary", fixture: alpineFixture, tags: ["streaming", "route", "render"]) {
        TestStep.expect("travelMode", "riding")
        TestStep.expect("streamedCollisionChunkIDs", alpineBeforeChunkIDs)
        TestStep.capture("alpine-boundary-before")
        // A real riding input crosses one 512 m detail boundary northward.
        TestStep.action(.move(V3(0, 0, 3)))
        TestStep.advance(1)
        TestStep.range("z", 10_753.5, 10_754.5)
        TestStep.expect("streamedCollisionChunkIDs", alpineAfterChunkIDs)
        TestStep.save("north-detail")
        TestStep.capture("alpine-boundary-after")
        // Return through the same boundary with the inverse production input.
        TestStep.action(.move(V3(0, 0, -3)))
        TestStep.advance(1)
        TestStep.range("z", 10_744.5, 10_745.5)
        TestStep.expect("streamedCollisionChunkIDs", alpineBeforeChunkIDs)
        TestStep.load("north-detail")
        TestStep.expect("travelMode", "riding")
        TestStep.expect("streamedCollisionChunkIDs", alpineAfterChunkIDs)
      },
      GameTest("creek-bank-sampling", fixture: creekFixture, tags: ["streaming", "route", "render"]) {
        TestStep.capture("creek-bank-before")
        TestStep.walk(x: 1_660, z: 2_170, maxTicks: 1200)
        TestStep.expect("nearestLandmarkID", "willow-creek")
        TestStep.expect("discoveredLandmarkIDs", "willow-creek")
        TestStep.save("bank-sample")
        TestStep.capture("creek-bank-after")
        TestStep.walk(x: 1_640, z: 2_170, maxTicks: 1200)
        TestStep.load("bank-sample")
        TestStep.expect("discoveredLandmarkIDs", "willow-creek")
      },
    ]
  }

  /// Applies only named fixture staging. It declares the rider familiar, then uses production
  /// control, movement,
  /// collision, and companion synchronization paths; the scenario records the boundary inputs.
  @discardableResult public static func configure(
    fixture: String, in world: SanctuaryWorld
  ) throws -> Bool {
    switch fixture {
    case alpineFixture:
      let rider = try requiredActor("cloudstepper-001", in: world)
      let trustStart = rider.position + SIMD2<Float>(-4, 2)
      world.camera = PlayerCamera(
        position: V3(
          trustStart.x, world.groundHeight(trustStart.x, trustStart.y) + 1.72, trustStart.y),
        yaw: 0, pitch: -0.08)
      world.syncExpeditionPlayer()
      try SanctuaryFixtureRelationships.declareFamiliar("cloudstepper-001", in: world)
      _ = try world.control("ride")

      // These fixture-only inputs bring the already-mounted companion to the south side of
      // the active chunk edge. They remain the same production riding/collision route.
      world.camera.yaw = .pi
      world.move(V3(0, 0, 20))
      world.move(V3(0, 0, 2))
      world.move(.zero)
      world.syncExpeditionPlayer()
      try world.validate()
      return true

    case creekFixture:
      let start = SIMD2<Float>(1_640, 2_170)
      let target = SIMD2<Float>(1_660, 2_170)
      try requireDryBank(start, target: target, in: world)
      world.camera = PlayerCamera(
        position: V3(start.x, world.groundHeight(start.x, start.y) + 1.72, start.y),
        yaw: .pi / 2, pitch: -0.08)
      world.syncExpeditionPlayer()
      world.move(.zero)
      try world.validate()
      return true

    default:
      return false
    }
  }

  private static func requiredActor(_ id: String, in world: SanctuaryWorld) throws -> WildlifeActor {
    guard let actor = world.controller.state.population.actor(id: id) else {
      throw SimulationFailure.invalid("Streaming fixture requires \(id)")
    }
    return actor
  }

  private static func requireDryBank(
    _ start: SIMD2<Float>, target: SIMD2<Float>, in world: SanctuaryWorld
  ) throws {
    for point in [start, target] {
      let ground = world.groundHeight(point.x, point.y)
      guard (world.localWaterHeight(point.x, point.y) ?? ground) <= ground + 0.5 else {
        throw SimulationFailure.invalid(
          "Creek bank fixture reaches water at (\(point.x), \(point.y)); choose a production dry bank")
      }
    }
  }
}
