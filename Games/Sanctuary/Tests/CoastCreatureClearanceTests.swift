import FieldCore
import SimulationCore
import XCTest
import simd

@testable import SanctuaryContent

final class CoastCreatureClearanceTests: XCTestCase {
  private let failedArtifactPosition = SIMD2<Float>(11_728.646, -7_173.585)

  func testSaltwindSpireKeepsIdentityBesideCreatureHabitat() throws {
    let geography = SanctuaryGeography()
    let landmark = geography.landmark(for: .coast)
    let plan = geography.plan(for: .init(containing: landmark.coordinate))
    let feature = try XCTUnwrap(plan.features.first { $0.id == landmark.id })

    XCTAssertEqual(landmark.coordinate, SIMD2<Float>(11_725, -7_175))
    XCTAssertEqual(feature.kind, .stoneSpire)
    XCTAssertEqual(feature.scale, 5)
    XCTAssertEqual(feature.coordinate, landmark.coordinate + SIMD2<Float>(-18, 0))
    XCTAssertEqual(
      SanctuaryTerrainChunkKey(containing: feature.coordinate),
      SanctuaryTerrainChunkKey(containing: landmark.coordinate))
    XCTAssertTrue(feature.isDiscovery)

    // The boulder arrangement resolver must not reinterpret the preserved landmark ID.
    let unchanged = try XCTUnwrap(SanctuaryLayout.resolveFeature(
      feature, terrain: Terrain(), boulders: BoulderArrangements()))
    XCTAssertEqual(SIMD2(unchanged.position.x, unchanged.position.z), feature.coordinate)
  }

  func testSaltbackHomeAndFailedArtifactPoseClearResolvedSpire() throws {
    let world = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: "saltback-001"))
    let landmark = SanctuaryGeography().landmark(for: .coast)
    let plan = SanctuaryGeography().plan(for: .init(containing: landmark.coordinate))
    let source = try XCTUnwrap(plan.features.first { $0.id == landmark.id })
    let feature = try XCTUnwrap(SanctuaryLayout.resolveFeature(
      source, terrain: world.world.terrain, garden: world.controller.state.garden,
      boulders: world.controller.state.boulders))
    let shape = try XCTUnwrap(SanctuaryLayout.collisionShape(for: feature))

    XCTAssertEqual(actor.position, landmark.coordinate + SIMD2<Float>(4, 2))
    for point in [actor.position, failedArtifactPosition] {
      let elevation = world.elevation(for: actor, at: point)
      let focus = V3(point.x, elevation.focusY, point.y)
      XCTAssertGreaterThanOrEqual(
        shape.value(at: focus - feature.position), 0,
        "Saltback focus intersects the Saltwind spire at \(point)")
      for height: Float in [0.3, 0.65] {
        let capsule = V3(point.x, elevation.supportY + height, point.y)
        XCTAssertGreaterThanOrEqual(
          shape.value(at: capsule - feature.position), 0.35,
          "Saltback ground capsule intersects the Saltwind spire at \(point)")
      }
    }
  }

  func testGroundedSaltwindFixtureCanSeeAndRideSaltbackWithProductionCollision() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.legacyInteractions = false
    let actorID = "saltback-001"
    try SanctuaryRelationshipFixture.establish(actorID, in: world)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: actorID))
    let player = actor.position + SIMD2<Float>(0, 4)
    world.camera = PlayerCamera(
      position: V3(player.x, world.groundHeight(player.x, player.y) + 1.72, player.y),
      yaw: 0, pitch: -0.08)
    world.updateCollisionStreaming(at: player)
    world.syncExpeditionPlayer()

    XCTAssertTrue(world.animalVisible(actor))
    XCTAssertEqual(world.nearbyAnimal?.id, actorID)
    _ = try world.control("ride")
    XCTAssertEqual(world.controller.state.travel.mode, .riding)
    XCTAssertEqual(world.controller.state.travel.companionID, actorID)
  }
}
