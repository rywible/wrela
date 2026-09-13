import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class AnimalRelationshipTests: XCTestCase {
  func testBakedPreferencesAreStableAndBounded() throws {
    let a = AnimalRelationship.baked(id: Expedition.creatureID, seed: 41)
    let b = AnimalRelationship.baked(id: Expedition.creatureID, seed: 41)
    XCTAssertEqual(a, b)
    XCTAssertNotEqual(a.preferences, AnimalRelationship.baked(id: "frostling-002", seed: 41).preferences)
    try a.validate()
  }

  func testAuthoredPhraseParserUsesClosedVocabulary() {
    XCTAssertEqual(AnimalRequestParser.parse("Hello there!"), .greeting)
    XCTAssertEqual(AnimalRequestParser.parse("Please follow me"), .follow)
    XCTAssertEqual(AnimalRequestParser.parse("follow me"), .follow)
    XCTAssertEqual(AnimalRequestParser.parse("Explain the northern ridge"), nil)
  }

  func testTrustPersistsWithoutDecayAndMemoryIsBounded() throws {
    var expedition = Expedition(seed: 17)
    expedition.player = Expedition.den + SIMD2<Float>(1, 0)
    for _ in 0..<300 { expedition.advance(seconds: 1 / 60, movingQuickly: false) }
    let earned = expedition.trust
    expedition.player = .zero
    for _ in 0..<600 { expedition.advance(seconds: 1 / 60, movingQuickly: true) }
    XCTAssertEqual(expedition.trust, earned)
    expedition.player = expedition.creaturePosition
    for _ in 0..<(AnimalRelationship.memoryLimit + 8) {
      _ = try expedition.addressCreature(.greeting, visible: true)
    }
    XCTAssertEqual(expedition.relationship.encounters.count, AnimalRelationship.memoryLimit)
    let restored = try JSONDecoder().decode(
      Expedition.self, from: JSONEncoder().encode(expedition))
    XCTAssertEqual(restored.relationship, expedition.relationship)
  }

  func testCalmPresenceStopsAtFamiliarityAndPreservesHigherTrust() {
    var receptive = AnimalRelationship.baked(
      id: "receptive-001", seed: 17, companionWilling: true)
    receptive.preferences = AnimalPreferences(
      temperament: .curious, favoriteRequest: .play, sociability: 1,
      playfulness: 1, companionWilling: true)
    receptive.observeCalmPresence(seconds: 120)
    XCTAssertEqual(receptive.trust, AnimalRelationship.familiarityCap, accuracy: 0.0001)
    XCTAssertFalse(receptive.isWilling(to: .play))
    XCTAssertFalse(receptive.isWilling(to: .follow))

    var established = AnimalRelationship.baked(
      id: "established-001", seed: 17, trust: 0.74, companionWilling: true)
    established.observeCalmPresence(seconds: 10_000)
    XCTAssertEqual(established.trust, 0.74)
  }

  func testImmediateRequestSpamRecordsResponsesWithoutAdditionalTrust() {
    var relationship = AnimalRelationship.baked(id: "patient-001", seed: 17)
    relationship.record(.greeting, accepted: true, tick: 120)
    let firstGain = relationship.trust
    XCTAssertGreaterThan(firstGain, 0)

    for _ in 0..<32 {
      relationship.record(.greeting, accepted: true, tick: 120)
    }
    relationship.record(.follow, accepted: false, tick: 120)
    XCTAssertEqual(relationship.trust, firstGain)
    XCTAssertEqual(relationship.encounters.count, AnimalRelationship.memoryLimit)
    XCTAssertEqual(relationship.encounters.last?.outcome, .refused)

    relationship.record(
      .greeting, accepted: true,
      tick: 120 + AnimalRelationship.trustRewardCooldownTicks)
    XCTAssertGreaterThan(relationship.trust, firstGain)
  }

  func testVariedAcceptedInteractionsOverSimulationTicksBuildTrustAndReplayExactly() throws {
    var relationship = AnimalRelationship.baked(
      id: "social-001", seed: 17, companionWilling: true)
    relationship.preferences = AnimalPreferences(
      temperament: .curious, favoriteRequest: .play, sociability: 1,
      playfulness: 1, companionWilling: true)
    relationship.observeCalmPresence(seconds: AnimalRelationship.familiaritySeconds)
    var tick = Int(AnimalRelationship.familiaritySeconds * 60)

    for request in [AnimalRequest.greeting, .wait, .play] {
      XCTAssertTrue(relationship.isWilling(to: request))
      relationship.record(request, accepted: true, tick: tick)
      tick += AnimalRelationship.trustRewardCooldownTicks
    }
    var replay = try JSONDecoder().decode(
      AnimalRelationship.self, from: JSONEncoder().encode(relationship))
    XCTAssertEqual(replay, relationship)

    for request in [AnimalRequest.come, .play, .come, .follow] {
      XCTAssertTrue(relationship.isWilling(to: request))
      relationship.record(request, accepted: true, tick: tick)
      replay.record(request, accepted: true, tick: tick)
      tick += AnimalRelationship.trustRewardCooldownTicks
      XCTAssertEqual(replay, relationship)
    }
    XCTAssertGreaterThan(relationship.trust, AnimalRelationship.familiarityCap)
    XCTAssertTrue(relationship.isWilling(to: .follow))
  }

  func testRequestValidationAndUnknownTextPreserveState() throws {
    let controller = try ExpeditionController(seed: 17)
    controller.updatePlayer(SIMD3<Float>(-3, 1.72, -83), yaw: 0, pitch: 0)
    let beforeUnknown = try controller.checkpoint()
    XCTAssertEqual(try controller.addressCreature("tell me a story", visible: true), "No request understood.")
    XCTAssertEqual(try controller.checkpoint(), beforeUnknown)
    let beforeHidden = try controller.checkpoint()
    XCTAssertThrowsError(try controller.addressCreature("hello", visible: false))
    XCTAssertEqual(try controller.checkpoint(), beforeHidden)
    XCTAssertThrowsError(
      try controller.addressCreature(String(repeating: "x", count: 161), visible: true))
    XCTAssertEqual(try controller.checkpoint(), beforeHidden)
  }

  func testFollowingCanLeaveOldLeashAndReplayThenExpiresWithoutTeleport() throws {
    let relationship = AnimalRelationship.baked(
      id: Expedition.creatureID, seed: 17, trust: 1, companionWilling: true)
    XCTAssertTrue(relationship.preferences.companionWilling)
    XCTAssertTrue(relationship.isWilling(to: .follow))

    let home = Expedition.den
    let visitor = home + SIMD2<Float>(7, 0)
    var actor = CreatureSimulation(home: home, seed: 17)
    actor.address(.follow, accepted: true, player: home + SIMD2<Float>(1, 0))
    var input = CreatureStimulus()
    input.player = visitor
    input.food = nil
    for _ in 0..<900 { actor.step(input) }
    XCTAssertGreaterThan(distance(actor.position, home), 3)

    var restored = try JSONDecoder().decode(
      CreatureSimulation.self, from: JSONEncoder().encode(actor))
    for _ in 0..<30 {
      actor.step(input)
      restored.step(input)
    }
    XCTAssertEqual(restored, actor)

    actor.address(.greeting, accepted: true, player: visitor)
    input.player = home
    for _ in 0..<180 { actor.step(input) }
    XCTAssertNil(actor.activeRequest)
    let distanceBeforeReturn = distance(actor.position, home)
    var previous = actor.position
    for _ in 0..<120 {
      actor.step(input)
      XCTAssertLessThanOrEqual(distance(actor.position, previous), CreatureMotion().stride + 0.0001)
      previous = actor.position
    }
    XCTAssertLessThan(distance(actor.position, home), distanceBeforeReturn)
  }

  func testAcceptedRequestDrivesBrainAndReleasePreservesActorHistory() throws {
    var expedition = Expedition(seed: 17)
    expedition.player = Expedition.den + SIMD2<Float>(1, 0)
    XCTAssertTrue(try expedition.addressCreature(.greeting, visible: true))
    XCTAssertEqual(expedition.creatureState.state, "greeting")
    XCTAssertNotEqual(expedition.creatureState.gaze, 0)
    for _ in 0..<300 { expedition.advance(seconds: 1 / 60, movingQuickly: false) }

    // This legacy release fixture now earns established trust through the same
    // accepted, cooldown-paced requests as play instead of passive presence.
    let sharedRequests: [AnimalRequest] = [
      .wait, .greeting, .come, .wait, .come, .play, .play, .come, .follow, .play,
      .follow, .come,
    ]
    for (index, request) in sharedRequests.enumerated() {
      if index > 0 {
        for _ in 0..<AnimalRelationship.trustRewardCooldownTicks {
          expedition.advance(seconds: 1 / 60, movingQuickly: false)
        }
      }
      XCTAssertTrue(try expedition.addressCreature(request, visible: true))
    }
    XCTAssertEqual(expedition.trust, 1, accuracy: 0.0001)
    let seed = expedition.creatureState.seed
    let tick = expedition.creatureState.tick
    expedition.interact()
    expedition.player = Expedition.home
    expedition.interact()
    XCTAssertEqual(expedition.creatureState.seed, seed)
    XCTAssertEqual(expedition.creatureState.tick, tick)
    XCTAssertEqual(expedition.creatureState.home, Expedition.home)
    XCTAssertFalse(expedition.relationship.encounters.isEmpty)
  }

  func testLegacyExpeditionDefaultsRelationshipFromStoredTrust() throws {
    var expedition = Expedition()
    expedition.player = Expedition.den
    for _ in 0..<120 { expedition.advance(seconds: 1 / 60, movingQuickly: false) }
    var object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(expedition)) as! [String: Any]
    object.removeValue(forKey: "relationship")
    let restored = try JSONDecoder().decode(
      Expedition.self, from: JSONSerialization.data(withJSONObject: object))
    XCTAssertEqual(restored.trust, expedition.trust)
    XCTAssertEqual(restored.relationship.id, Expedition.creatureID)
  }
}
