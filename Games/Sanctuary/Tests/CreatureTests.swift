import FieldCore
import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class CreatureTests: XCTestCase {
  func trial(_ scenario: CreatureScenario, frames: Int, seed: UInt32 = 17) -> CreatureSimulation {
    var actor = CreatureSimulation(seed: seed)
    for i in 0..<frames { actor.step(scenario.stimulus(at: Float(i) / 60)) }
    return actor
  }
  func testCalmVisitorOcclusionAndRecovery() throws {
    let calm = trial(.calm, frames: 600)
    XCTAssertEqual(calm.state, "watching")
    XCTAssertEqual(calm.position, .zero)
    let hidden = trial(.occluded, frames: 600)
    XCTAssertEqual(hidden.fear, 0)
    XCTAssertFalse(hidden.events.contains { $0.state == "fleeing" })
    let startled = trial(.startle, frames: 240)
    XCTAssertTrue(startled.events.contains { $0.state == "fleeing" })
    XCTAssertGreaterThan(distance(startled.position, SIMD2(0, 3)), 3.3)
    let recovered = trial(.recovery, frames: 900)
    XCTAssertEqual(recovered.fear, 0)
    XCTAssertNotEqual(recovered.state, "fleeing")
    try recovered.validate()
  }
  func testFoodAndObstaclePath() throws {
    let forage = trial(.forage, frames: 600)
    XCTAssertLessThan(distance(forage.position, SIMD2(1.5, -1)), 0.31)
    var actor = CreatureSimulation()
    for i in 0..<1200 {
      let input = CreatureScenario.obstacle.stimulus(at: Float(i) / 60)
      actor.step(input)
      XCTAssertGreaterThanOrEqual(
        distance(actor.position, input.obstacle!), input.obstacleRadius + 0.37)
      try actor.validate()
    }
    XCTAssertTrue(actor.events.contains { $0.state == "avoiding" })
    XCTAssertLessThan(distance(actor.position, SIMD2(0, -2)), 0.31)
  }
  func testSeededReplayAndSaveDuringFlight() throws {
    var a = trial(.forage, frames: 110)
    XCTAssertTrue(a.hopping)
    var b = try JSONDecoder().decode(CreatureSimulation.self, from: JSONEncoder().encode(a))
    for i in 110..<600 {
      let input = CreatureScenario.recovery.stimulus(at: Float(i) / 60)
      a.step(input)
      b.step(input)
    }
    XCTAssertEqual(a, b)
    XCTAssertEqual(trial(.occluded, frames: 900), trial(.occluded, frames: 900))
    XCTAssertNotEqual(
      trial(.occluded, frames: 900, seed: 1).position,
      trial(.occluded, frames: 900, seed: 2).position)
  }
  func testStanceDoesNotTranslateAndNoGroundPenetration() {
    var actor = CreatureSimulation()
    let motion = CreatureMotion()
    for _ in 0..<600 {
      let before = actor.position
      actor.step(CreatureScenario.forage.stimulus(at: 0), motion: motion)
      if let p = actor.phase, p < 0.22 { XCTAssertEqual(actor.position, before) }
      for p: Float in [0, 0.1, 0.2, 0.3, 0.5, 0.72, 0.85, 1] {
        XCTAssertGreaterThanOrEqual(CreatureMotion.flight(p), -0.000001)
      }
    }
    for time: Float in [0, 0.5, 1, 4] {
      let poses = motion.poses(time: time, phase: nil)
      for foot in ["front-left", "front-right", "hind-left", "hind-right"] {
        XCTAssertEqual(poses[foot]!.offset, .zero)
        XCTAssertEqual(poses[foot]!.rotation, .zero)
      }
    }
  }
  func testAttachmentsIndependentOfPartOrderAndRejectCycles() throws {
    let body = PartJoint(id: "body", pivot: SIMD3(0, 1, 0))
    let head = PartJoint(id: "head", parent: "body", pivot: SIMD3(0, 2, 0))
    let eyes = PartJoint(id: "eyes", parent: "head")
    let joints = [body, head, eyes]
    try PartRig.validate(joints)
    let poses = [
      "body": JointPose(offset: SIMD3(0, 0.1, 0)), "head": JointPose(rotation: SIMD3(0, 35, 0)),
    ]
    let a = PartRig.matrices(joints, poses: poses)
    let b = PartRig.matrices(joints.reversed(), poses: poses)
    XCTAssertEqual(a["eyes"], a["head"])
    XCTAssertEqual(a["eyes"], b["eyes"])
    var cycle = body
    cycle.parent = "eyes"
    XCTAssertThrowsError(try PartRig.validate([cycle, head, eyes]))
    XCTAssertThrowsError(try PartRig.validate([body, body]))
    XCTAssertThrowsError(try PartRig.validate([head]))
    var invalid = body
    invalid.scale = .nan
    XCTAssertThrowsError(try PartRig.validate([invalid]))
  }
  func testLegacyExpeditionLoadsWithoutCreatureSnapshot() throws {
    let data = try JSONEncoder().encode(Expedition())
    var object = try JSONSerialization.jsonObject(with: data) as! [String: Any]
    object.removeValue(forKey: "creature")
    var state = try JSONDecoder().decode(
      Expedition.self, from: JSONSerialization.data(withJSONObject: object))
    XCTAssertEqual(state.creaturePosition, Expedition.den)
    state.advance(seconds: 1 / 60, movingQuickly: false)
    XCTAssertNotNil(state.creature)
    try state.validate()
  }
}

extension CreatureTests {
  func testBlinkActuallyClosesGeometryAndKeepsPivot() throws {
    let motion = CreatureMotion()
    let frame = (0..<360).max {
      motion.blink(at: Float($0) / 60) < motion.blink(at: Float($1) / 60)
    }!
    let t = Float(frame) / 60
    XCTAssertGreaterThan(motion.blink(at: t), 0.98)
    let eye = PartJoint(id: "eyes", pivot: SIMD3(0, 0.8, -0.5))
    let matrix = PartRig.matrices([eye], poses: motion.poses(time: t, phase: nil))["eyes"]!
    let center = matrix * SIMD4<Float>(eye.pivot, 1)
    let upper = matrix * SIMD4<Float>(eye.pivot + SIMD3(0, 0.05, 0), 1)
    XCTAssertEqual(center, SIMD4<Float>(eye.pivot, 1))
    XCTAssertLessThan(upper.y - center.y, 0.004)
    XCTAssertEqual(motion.blink(at: 0), 0)
    var disabled = motion
    try disabled.set("blinkRate", 0)
    XCTAssertEqual(disabled.blink(at: t), 0)
  }
  func testAttentionHoldsThenChangesAndOldRecipeDefaults() throws {
    let motion = try JSONDecoder().decode(CreatureMotion.self, from: Data("{\"height\":0.2}".utf8))
    XCTAssertEqual(motion.height, 0.2)
    XCTAssertEqual(motion.blinkRate, 1)
    let a = motion.poses(time: 1, phase: nil)["head"]!.rotation
    let b = motion.poses(time: 2, phase: nil)["head"]!.rotation
    XCTAssertEqual(a, b, "A gaze should settle instead of perpetually oscillating")
    XCTAssertNotEqual(a, motion.poses(time: 5, phase: nil)["head"]!.rotation)
  }
  func testWorldTraversalConstraintAndGroundedTurning() {
    var actor = CreatureSimulation()
    for _ in 0..<300 {
      actor.step(CreatureScenario.forage.stimulus(at: 0), canTraverse: { _, _ in false })
    }
    XCTAssertEqual(actor.position, .zero)
    XCTAssertTrue(actor.events.contains { $0.state == "blocked" })
    actor = CreatureSimulation()
    for _ in 0..<90 {
      let yaw = actor.yaw
      actor.step(CreatureScenario.calm.stimulus(at: 0))
      if let phase = actor.phase, phase < 0.22 { XCTAssertEqual(actor.yaw, yaw) }
    }
    XCTAssertEqual(actor.position, .zero)
    XCTAssertGreaterThan(abs(actor.yaw), 3)
  }
}
