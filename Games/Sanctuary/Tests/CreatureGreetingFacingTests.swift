import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class CreatureGreetingFacingTests: XCTestCase {
  private let home = SIMD2<Float>(2, 18)
  private let speaker = SIMD2<Float>(0, 22.8)

  private func stimulus(player: SIMD2<Float>) -> CreatureStimulus {
    var input = CreatureStimulus()
    input.player = player
    input.food = nil
    return input
  }

  private func facingError(_ actor: CreatureSimulation, toward point: SIMD2<Float>) -> Float {
    let desired = atan2(-(point.x - actor.position.x), -(point.y - actor.position.y))
    return abs(atan2(sin(desired - actor.yaw), cos(desired - actor.yaw)))
  }

  func testGreetingAndWaitFaceSpeakerThroughBoundedStationaryHops() throws {
    for request: AnimalRequest in [.greeting, .wait] {
      var actor = CreatureSimulation(home: home)
      actor.address(request, accepted: true, player: speaker)
      var turnStart: Float = actor.yaw
      var completedTurns = 0
      for _ in 0..<120 {
        let wasHopping = actor.hopping
        let previousYaw = actor.yaw
        actor.step(stimulus(player: speaker))
        XCTAssertEqual(actor.position, home)
        XCTAssertEqual(actor.start, home)
        XCTAssertEqual(actor.target, home)
        if !wasHopping && actor.hopping { turnStart = actor.yaw }
        if let phase = actor.phase, phase <= 0.22 {
          XCTAssertEqual(actor.yaw, previousYaw, "Grounded preparation cannot skate in yaw")
        }
        if wasHopping && !actor.hopping {
          let turn = abs(atan2(sin(actor.yaw - turnStart), cos(actor.yaw - turnStart)))
          XCTAssertLessThanOrEqual(turn, .pi / 2 + 0.00001)
          completedTurns += 1
        }
      }
      XCTAssertEqual(completedTurns, 2)
      XCTAssertLessThan(facingError(actor, toward: speaker), 0.001)
      XCTAssertEqual(actor.activeRequest, request)
      XCTAssertFalse(actor.hopping)
      try actor.validate()
    }
  }

  func testMidTurnSaveReplaysExactlyAndPreservesRequestExpiry() throws {
    for request: AnimalRequest in [.greeting, .wait] {
      var actor = CreatureSimulation(home: home)
      actor.address(request, accepted: true, player: speaker)
      let input = stimulus(player: speaker)
      for _ in 0..<25 { actor.step(input) }
      XCTAssertTrue(actor.hopping)
      var restored = try JSONDecoder().decode(CreatureSimulation.self, from: JSONEncoder().encode(actor))
      let duration = request == .wait ? 3_600 : 180
      for _ in 25..<(duration - 1) {
        actor.step(input)
        restored.step(input)
        XCTAssertEqual(actor, restored)
        XCTAssertEqual(actor.position, home)
      }
      XCTAssertEqual(actor.activeRequest, request)
      actor.step(input)
      restored.step(input)
      XCTAssertNil(actor.activeRequest)
      XCTAssertEqual(actor, restored)
      try restored.validate()
    }
  }

  func testWaitingTracksVisibleSpeakerButKeepsLastSeenAnchorWhenHidden() throws {
    var actor = CreatureSimulation(home: home)
    actor.address(.wait, accepted: true, player: speaker)
    for _ in 0..<120 { actor.step(stimulus(player: speaker)) }
    let facing = actor.yaw
    let otherSide = home + SIMD2<Float>(0, -4)
    var hidden = stimulus(player: otherSide)
    hidden.visible = false
    for _ in 0..<120 { actor.step(hidden) }
    XCTAssertEqual(actor.yaw, facing)
    XCTAssertEqual(actor.position, home)
    for _ in 0..<120 {
      actor.step(stimulus(player: otherSide))
      XCTAssertEqual(actor.position, home)
    }
    XCTAssertLessThan(facingError(actor, toward: otherSide), 0.001)
    XCTAssertEqual(actor.activeRequest, .wait)
    try actor.validate()
  }

  func testProductionPopulationGreetingTurnsNativeSunhareTowardSpeaker() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let greeting = try population.address(.greeting, targetID: id, player: speaker,
      expectedRevision: population.revision, isVisible: { $0.id == id })
    XCTAssertTrue(greeting.accepted)
    for _ in 0..<120 {
      try population.advance(seconds: 1 / 60, player: speaker, running: false,
        garden: HabitatGarden(), canTraverse: { _, _ in true }, isVisible: { $0.id == id })
    }
    let animal = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(animal.position, home)
    XCTAssertEqual(animal.posture, "greeting")
    XCTAssertLessThan(facingError(animal.simulation, toward: speaker), 0.001)
    try population.validate()
  }
}
