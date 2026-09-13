import XCTest
import simd

@testable import SanctuaryContent

final class CreatureElevationTests: XCTestCase {
  func testGameOwnedFocusOffsetsMatchAuthoredEyes() {
    let expected: [WildlifeSpecies: Float] = [
      .frostling: 0.80, .sunhare: 0.82, .brookweaver: 0.48, .reedwalker: 1.48,
      .moonhart: 1.65, .cloudstepper: 1.28, .dunefox: 0.72, .canopyGlider: 0.53,
      .saltback: 0.41, .tidepooler: 0.39, .cloudRay: 0.28,
    ]
    XCTAssertEqual(expected.count, WildlifeSpecies.allCases.count)
    for species in WildlifeSpecies.allCases {
      XCTAssertEqual(
        species.sanctuaryFocusOffset, expected[species]!, accuracy: 0.0001, species.rawValue)
    }
  }

  func testOceanCloudRayUsesWaterAsSupport() throws {
    let world = try SanctuaryWorld(seed: 17)
    let ray = try XCTUnwrap(world.controller.state.population.actor(id: "cloud-ray-001"))
    let water = try XCTUnwrap(world.world.terrain.water(at: ray.position))
    let elevation = world.elevation(for: ray)

    XCTAssertEqual(water.body, .ocean)
    XCTAssertEqual(elevation.supportY, water.surfaceHeight, accuracy: 0.0001)
    XCTAssertEqual(elevation.rootY, water.surfaceHeight + 4.8, accuracy: 0.0001)
    XCTAssertEqual(elevation.focusY, elevation.rootY + 0.28, accuracy: 0.0001)
    XCTAssertGreaterThan(elevation.rootY, water.surfaceHeight)
  }

  func testCloudstepperFocusMatchesAuthoredEyeHeight() throws {
    let world = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: "cloudstepper-001"))
    let elevation = world.elevation(for: actor)
    XCTAssertEqual(elevation.supportY, world.groundHeight(actor.position.x, actor.position.y), accuracy: 0.0001)
    XCTAssertEqual(elevation.rootY, elevation.supportY, accuracy: 0.0001)
    XCTAssertEqual(elevation.focusY, elevation.rootY + 1.28, accuracy: 0.0001)
  }

  func testOptionalCoordinateUsesTheSameSpeciesRules() throws {
    let world = try SanctuaryWorld(seed: 17)
    let ray = try XCTUnwrap(world.controller.state.population.actor(id: "cloud-ray-001"))
    let point = SIMD2<Float>(13_825, -12_425)
    let elevation = world.elevation(for: ray, at: point)
    let water = try XCTUnwrap(world.world.terrain.water(at: point))
    XCTAssertEqual(elevation.supportY, water.surfaceHeight, accuracy: 0.0001)
    XCTAssertEqual(elevation.rootY - elevation.supportY, 4.8, accuracy: 0.0001)
  }

  func testCanopyGliderKeepsItsAuthoredHoverOffsetOverOceanAndLand() throws {
    let world = try SanctuaryWorld(seed: 17)
    let glider = try XCTUnwrap(world.controller.state.population.actor(id: "canopy-glider-001"))
    let ocean = SIMD2<Float>(13_825, -12_425)
    let water = try XCTUnwrap(world.localWaterHeight(ocean.x, ocean.y))
    let oceanElevation = world.elevation(for: glider, at: ocean)
    XCTAssertGreaterThan(water, world.groundHeight(ocean.x, ocean.y) + 0.5)
    XCTAssertEqual(oceanElevation.supportY, water, accuracy: 0.0001)
    XCTAssertEqual(oceanElevation.rootY - oceanElevation.supportY, 1.35, accuracy: 0.0001)

    let landElevation = world.elevation(for: glider)
    XCTAssertEqual(
      landElevation.supportY, world.groundHeight(glider.position.x, glider.position.y), accuracy: 0.0001)
    XCTAssertEqual(landElevation.rootY - landElevation.supportY, 1.35, accuracy: 0.0001)
  }

  func testCanopyGliderUsesComposedPlayerWaterWhileGroundSpeciesStayGrounded() throws {
    let world = try SanctuaryWorld(seed: 17)
    let point = SIMD2<Float>(0, 20)
    try world.controller.editLiving { state in
      _ = try state.applyNature(.plant(.shallowWater, at: .init(x: point.x, z: point.y), radius: 3))
    }
    let glider = try XCTUnwrap(world.controller.state.population.actor(id: "canopy-glider-001"))
    let stepper = try XCTUnwrap(world.controller.state.population.actor(id: "cloudstepper-001"))
    let ground = world.groundHeight(point.x, point.y)
    let water = try XCTUnwrap(world.localWaterHeight(point.x, point.y))
    XCTAssertGreaterThan(water, ground)
    XCTAssertEqual(world.elevation(for: glider, at: point).supportY, water, accuracy: 0.0001)
    XCTAssertEqual(world.elevation(for: stepper, at: point).supportY, ground, accuracy: 0.0001)
    XCTAssertEqual(world.elevation(for: stepper, at: point).rootY, ground, accuracy: 0.0001)
  }

  func testSunhareRejectsAuthoredWaterAndUsesSavedBridgeSupport() throws {
    let world = try SanctuaryWorld(seed: 17)
    let point = SIMD2<Float>(100, 100)
    let sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: point.x, z: point.y), radius: 2))
    }
    XCTAssertFalse(world.creatureCanTraverse(sunhare, point, point))

    let water = try XCTUnwrap(world.localWaterHeight(point.x, point.y))
    _ = try world.commitPlacement(
      .place(.bridge, at: .init(x: point.x, y: water, z: point.y), yawRadians: 0, scale: 1),
      expectedRevision: world.controller.state.buildings.revision)
    let bridge = try XCTUnwrap(world.constructionFacts.first {
      $0.kind == .walkable && $0.placementID == 1
    })
    let elevation = world.elevation(for: sunhare, at: point)
    XCTAssertEqual(elevation.supportY, bridge.top, accuracy: 0.0001)
    XCTAssertGreaterThan(elevation.supportY, world.groundHeight(point.x, point.y))
    XCTAssertTrue(world.creatureCanTraverse(
      sunhare, point - SIMD2<Float>(1.5, 0), point + SIMD2<Float>(1.5, 0)))
  }

  func testBridgeRailsRejectAnimalCrossingWhileTheCenterlineRemainsWalkable() throws {
    let world = try SanctuaryWorld(seed: 17)
    let center = SIMD2<Float>(100, 100)
    let sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    _ = try world.commitPlacement(
      .place(.bridge, at: .init(x: center.x, y: world.groundHeight(center.x, center.y), z: center.y),
        yawRadians: 0, scale: 0.5), expectedRevision: 0)

    XCTAssertTrue(world.creatureCanTraverse(
      sunhare, center - SIMD2<Float>(0.75, 0), center + SIMD2<Float>(0.75, 0)))
    XCTAssertFalse(world.creatureCanTraverse(
      sunhare, SIMD2(center.x - 0.75, center.y + 0.36), SIMD2(center.x + 0.75, center.y + 0.36)))
  }

  func testWetHabitatSpeciesRetainsAuthoredShallowWaterAccess() throws {
    let world = try SanctuaryWorld(seed: 17)
    let point = SIMD2<Float>(100, 100)
    let brookweaver = try XCTUnwrap(
      world.controller.state.population.actor(id: "brookweaver-001"))
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: point.x, z: point.y), radius: 2))
    }
    XCTAssertTrue(world.creatureCanTraverse(brookweaver, point, point))
  }

  func testAvoidingSpeciesCanLeaveAnExistingWetPositionButCannotCrossItLaterally() throws {
    let world = try SanctuaryWorld(seed: 17)
    let point = SIMD2<Float>(100, 100)
    let sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: point.x, z: point.y), radius: 0.25))
    }
    XCTAssertFalse(world.creatureCanTraverse(
      sunhare, point - SIMD2<Float>(0.05, 0), point + SIMD2<Float>(0.05, 0)))
    XCTAssertTrue(world.creatureCanTraverse(sunhare, point, point + SIMD2<Float>(0.3, 0)))
  }
}
