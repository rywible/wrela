import FieldCore
import SimulationCore
import XCTest
import simd

@testable import SanctuaryContent

final class AlpineCreatureClearanceTests: XCTestCase {
  func testCloudstepFeatureKeepsLandmarkIdentityBesideHabitat() throws {
    let geography = SanctuaryGeography()
    let landmark = geography.landmark(for: .alpine)
    let plan = geography.plan(for: .init(containing: landmark.coordinate))
    let feature = try XCTUnwrap(plan.features.first { $0.id == landmark.id })

    XCTAssertEqual(landmark.coordinate, SIMD2<Float>(-420, 10_675))
    XCTAssertEqual(feature.kind, .stoneSpire)
    XCTAssertEqual(feature.scale, 5)
    XCTAssertEqual(feature.coordinate, landmark.coordinate + SIMD2<Float>(12, 0))
    XCTAssertTrue(feature.isDiscovery)
  }

  func testCloudstepperFocusAndGroundCapsuleClearLocalResolvedFeatures() throws {
    let world = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: "cloudstepper-001"))
    let elevation = world.elevation(for: actor)
    let keys = SanctuaryTerrainChunkKey.neighborhood(around: actor.position)
    let geography = SanctuaryGeography()
    let features = keys.flatMap { geography.plan(for: $0).features }.compactMap {
      SanctuaryLayout.resolveFeature($0, terrain: world.world.terrain)
    }

    let focus = V3(actor.position.x, elevation.focusY, actor.position.y)
    let capsuleSamples = [Float(0.3), 0.65].map {
      V3(actor.position.x, elevation.supportY + $0, actor.position.y)
    }
    for feature in features {
      guard let shape = SanctuaryLayout.collisionShape(for: feature) else { continue }
      XCTAssertGreaterThanOrEqual(
        shape.value(at: focus - feature.position), 0,
        "focus intersects \(feature.source.id)")
      for sample in capsuleSamples {
        XCTAssertGreaterThanOrEqual(
          shape.value(at: sample - feature.position), 0.35,
          "ground capsule intersects \(feature.source.id)")
      }
    }
  }

  func testExactAlpineFixtureHasVisibleCloudstepperWithStreamedCollision() throws {
    let world = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: "cloudstepper-001"))
    let start = actor.position + SIMD2<Float>(-4, 2)
    world.camera = PlayerCamera(
      position: V3(start.x, world.groundHeight(start.x, start.y) + 1.72, start.y),
      yaw: 0, pitch: -0.08)
    world.updateCollisionStreaming(at: start)

    XCTAssertTrue(world.animalVisible(actor))
  }
}
