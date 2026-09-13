import FieldCore
import Foundation
import XCTest
import simd
@testable import SanctuaryContent

final class GroundSurfaceTests: XCTestCase {
  private let dry = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 0, rain: 0, wind: 0.2)
  private let rain = SanctuaryClimate.Sample(timeOfDay: 0.5, cloud: 1, rain: 1, wind: 0.7)

  private func contact(_ world: SanctuaryWorld, at p: SIMD2<Float>) -> GroundInfluenceEvent {
    let support = SIMD3(p.x, world.groundHeight(p.x, p.y), p.y)
    return GroundInfluenceEvent(id: 1, sourceID: SanctuaryGroundContacts.playerSourceID,
      kind: .foot, start: support, end: support, radius: 0.13,
      displacement: 0.1, compression: 0.5, startTime: 0, endTime: 0, recoverySeconds: 30)
  }

  private func bank(_ world: SanctuaryWorld) throws -> GroundInfluenceEvent {
    for x in stride(from: Float(1640), through: 1770, by: 2) {
      let event = contact(world, at: SIMD2(x, 2170))
      if world.groundTraceSupport(event: event, x: x, z: 2170, weather: dry) != nil {
        return event
      }
    }
    return try XCTUnwrap(nil as GroundInfluenceEvent?, "No eligible dry bank in this source fixture")
  }

  private func wetlandBank(
    _ world: SanctuaryWorld
  ) throws -> (GroundInfluenceEvent, SanctuaryGroundSurface, SanctuaryGroundSurface) {
    let center = SanctuaryGeography().landmark(for: .wetland).coordinate
    let directions: [SIMD2<Float>] = [
      SIMD2(1, 0), SIMD2(-1, 0), SIMD2(0, 1), SIMD2(0, -1),
      normalize(SIMD2(1, 1)), normalize(SIMD2(-1, 1)),
      normalize(SIMD2(1, -1)), normalize(SIMD2(-1, -1)),
    ]
    // Production wetland water is patchy and persistent ecology may add a pool at its
    // landmark. Search a finite 960 m neighborhood for an actual dry, wetland-affine bank.
    for radius in stride(from: Float(40), through: 960, by: 40) {
      for direction in directions {
        let point = center + direction * radius
        guard world.localWaterHeight(point.x, point.y) == nil,
          let biome = try? SanctuaryGeography().sample(at: point),
          (biome.weights[.wetland] ?? 0) >= 0.15
        else { continue }
        let event = contact(world, at: point)
        if let drySupport = world.groundTraceSupport(
          event: event, x: point.x, z: point.y, weather: dry),
          let rainSupport = world.groundTraceSupport(
            event: event, x: point.x, z: point.y, weather: rain)
        { return (event, drySupport, rainSupport) }
      }
    }
    return try XCTUnwrap(
      nil as (GroundInfluenceEvent, SanctuaryGroundSurface, SanctuaryGroundSurface)?,
      "No eligible dry wetland bank in the bounded production-source fixture")
  }

  func testSoilRejectsAirborneAndOutsideWorldContacts() throws {
    let world = try SanctuaryWorld()
    let event = try bank(world)
    XCTAssertNotNil(world.groundTraceSupport(
      event: event, x: event.end.x, z: event.end.z, weather: dry))
    var airborne = event
    airborne.start.y += 1; airborne.end.y += 1
    XCTAssertNil(world.groundTraceSupport(
      event: airborne, x: event.end.x, z: event.end.z, weather: dry))
    XCTAssertNil(world.groundTraceSupport(
      event: event, x: .nan, z: event.end.z, weather: dry))
    XCTAssertNil(world.groundTraceSupport(
      event: event, x: 16001, z: event.end.z, weather: dry))
  }

  func testNewWaterAndConstructionSuppressOldSoilTraces() throws {
    let world = try SanctuaryWorld()
    let event = try bank(world)
    let x = event.end.x, z = event.end.z
    try world.controller.editLiving { state in
      _ = try state.applyNature(.plant(.shallowWater, at: .init(x: x, z: z), radius: 2))
    }
    XCTAssertNil(world.groundTraceSupport(event: event, x: x, z: z, weather: dry))
    try world.controller.editLiving { state in
      _ = try state.applyNature(.undo)
      var building = state.buildings
      _ = try building.apply(.place(.deck, at: .init(x: x, y: event.end.y, z: z), yawRadians: 0, scale: 1), expectedRevision: building.revision)
      state.construction = building
    }
    XCTAssertNil(world.groundTraceSupport(event: event, x: x, z: z, weather: dry))
  }

  func testSharedSurfaceRainResponseAtCreekAndWetlandDryGround() throws {
    let world = try SanctuaryWorld()
    let creek = try bank(world)
    let creekDry = try XCTUnwrap(world.groundTraceSupport(
      event: creek, x: creek.end.x, z: creek.end.z, weather: dry))
    let creekRain = try XCTUnwrap(world.groundTraceSupport(
      event: creek, x: creek.end.x, z: creek.end.z, weather: rain))
    XCTAssertGreaterThan(creekRain.strength, creekDry.strength)

    let (_, wetlandDry, wetlandRain) = try wetlandBank(world)
    XCTAssertGreaterThan(wetlandRain.strength, wetlandDry.strength + 0.01)
  }

  func testDryCabinRejectsWhileAuthoredShallowWaterShoreRemainsEligible() throws {
    let world = try SanctuaryWorld()
    let cabinPoint = SIMD2<Float>(40, 40)
    let cabin = contact(world, at: cabinPoint)
    XCTAssertNil(world.groundTraceSupport(
      event: cabin, x: cabinPoint.x, z: cabinPoint.y, weather: dry))

    try world.controller.editLiving { state in
      _ = try state.applyNature(.plant(
        .shallowWater, at: .init(x: cabinPoint.x, z: cabinPoint.y), radius: 2))
    }
    let shorePoint = cabinPoint + SIMD2<Float>(3, 0)
    let shore = contact(world, at: shorePoint)
    let support = try XCTUnwrap(world.groundTraceSupport(
      event: shore, x: shorePoint.x, z: shorePoint.y, weather: dry))
    XCTAssertGreaterThanOrEqual(support.strength, 0.49)
  }
}
