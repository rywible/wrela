import Foundation
import SimulationCore
import XCTest

@testable import SanctuaryContent

final class CreatureDecisionIntegrationTests: XCTestCase {
  private func world(root: URL? = nil) throws -> SanctuaryWorld {
    let world = try SanctuaryWorld(root: root)
    world.camera.position = SIMD3(2, world.groundHeight(2, 23) + 1.72, 23)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    return world
  }

  func testAcceptedExternalDecisionReplaysWithoutInterpreter() throws {
    let world = try world()
    let before = try world.checkpoint()
    let ticket = try world.prepareCreatureInterpretation("hello", requestID: "recorded-1")
    XCTAssertEqual(try world.checkpoint(), before, "Preparing asynchronous input is transient")
    world.advance(1 / 60, running: false)
    let replayStart = try world.checkpoint()
    let proposal = try CreatureRequestProposal(ticket: ticket, request: .greeting, source: .onDeviceModel)
    let record = try world.acceptCreatureInterpretation(proposal, ticket: ticket)
    XCTAssertEqual(record.disposition, .applied)
    let after = try world.checkpoint()
    XCTAssertEqual(world.controller.state.decisionJournal.decisions, [record])
    let action = SimulationAction.externalDecision(try SimulationCoding.encode(record))
    let savedAction = try SimulationCoding.encode(action)
    try world.restore(replayStart)
    try world.apply(JSONDecoder().decode(SimulationAction.self, from: savedAction))
    XCTAssertEqual(try world.checkpoint(), after)
    XCTAssertThrowsError(try world.apply(action))
    XCTAssertEqual(try world.checkpoint(), after)
  }

  func testRejectedMeaningsRecordAndReplayWithoutAnimalActionOrCredit() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try world(root: root)
    let replayStart = try world.checkpoint()
    var records: [RecordedCreatureDecision] = []

    for (index, text, request): (Int, String, AnimalRequest) in [
      (0, "don't follow me", .follow),
      (1, "follow me but stay here", .follow),
      (2, "please fly to me", .come),
    ] {
      let populationBefore = world.controller.state.population
      let journeyBefore = world.controller.state.travel
      let ticket = try world.prepareCreatureInterpretation(text, requestID: "rejected-\(index)")
      let proposal = try CreatureRequestProposal(
        ticket: ticket, request: request, source: .onDeviceModel)
      let record = try world.acceptCreatureInterpretation(proposal, ticket: ticket)
      XCTAssertEqual(record.disposition, .rejected)
      XCTAssertEqual(world.controller.state.population, populationBefore)
      XCTAssertEqual(world.controller.state.travel, journeyBefore)
      records.append(record)
    }

    let after = try world.checkpoint()
    try world.controller.save()
    let reopened = try self.world(root: root)
    XCTAssertEqual(try reopened.checkpoint(), after)

    try world.restore(replayStart)
    for record in records {
      try world.apply(.externalDecision(try SimulationCoding.encode(record)))
    }
    XCTAssertEqual(try world.checkpoint(), after)
    let beforeDuplicate = try world.checkpoint()
    XCTAssertThrowsError(
      try world.apply(.externalDecision(try SimulationCoding.encode(records.last!))))
    XCTAssertEqual(try world.checkpoint(), beforeDuplicate)
  }

  func testRelationshipChangeMakesDelayedCompletionStaleAndAtomic() throws {
    let world = try world()
    let ticket = try world.prepareCreatureInterpretation("hello", requestID: "relationship-stale")
    let proposal = try CreatureRequestProposal(
      ticket: ticket, request: .greeting, source: .onDeviceModel)

    _ = try world.request("hello")
    let changed = try world.checkpoint()
    XCTAssertThrowsError(try world.acceptCreatureInterpretation(proposal, ticket: ticket)) { error in
      XCTAssertEqual(error as? CreatureDecisionJournalError, .staleWorldRevision)
    }
    XCTAssertEqual(try world.checkpoint(), changed)
  }

  func testDelayedCompletionCannotActOnVisibleTargetAfterPlayerLeavesRequestRange() throws {
    let world = try world()
    let ticket = try world.prepareCreatureInterpretation(
      "hello", requestID: "visible-but-too-far")
    let proposal = try CreatureRequestProposal(
      ticket: ticket, request: .greeting, source: .onDeviceModel)
    let actor = try XCTUnwrap(
      world.controller.state.population.actor(id: ticket.targetID))

    // Player motion is intentionally absent from the semantic revision: a
    // delayed result still has to pass the ordinary spatial action guard.
    let distant = actor.position + SIMD2<Float>(0, WildlifePopulation.requestRange + 2)
    world.camera.position = SIMD3(
      distant.x, world.groundHeight(distant.x, distant.y) + 1.72, distant.y)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    XCTAssertTrue(world.animalVisible(actor),
      "The regression must reach the range guard rather than fail visibility")
    let before = try world.checkpoint()

    XCTAssertThrowsError(
      try world.acceptCreatureInterpretation(proposal, ticket: ticket)
    ) { error in
      XCTAssertEqual(error as? WildlifePopulationError, .tooFar)
    }
    XCTAssertEqual(try world.checkpoint(), before,
      "A delayed out-of-range completion must not journal or mutate the animal")
  }

  func testAppliedPlayInvitationReopensThenDepartureInterruptsWithoutExperienceCredit() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try world(root: root)
    _ = try world.request("hello")
    for _ in 0..<AnimalRelationship.companyCompletionTicks {
      world.advance(1 / 60, running: false)
    }
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(actor.relationship.sharedExperiences.map(\.kind), [.company])
    world.camera.position = SIMD3(
      actor.position.x, world.groundHeight(actor.position.x, actor.position.y + 2) + 1.72,
      actor.position.y + 2)
    world.camera.yaw = 0
    world.syncExpeditionPlayer()
    XCTAssertEqual(world.nearbyAnimal?.id, actor.id,
      "The production visibility fixture must reacquire the same welcomed animal")

    let beforeInvitation = try XCTUnwrap(
      world.controller.state.population.actor(id: actor.id)?.relationship.trust)
    let ticket = try world.prepareCreatureInterpretation("play", requestID: "play-before-reopen")
    let proposal = try CreatureRequestProposal(
      ticket: ticket, request: .play, source: .onDeviceModel)
    let record = try world.acceptCreatureInterpretation(proposal, ticket: ticket)
    XCTAssertEqual(record.disposition, .applied)
    let invited = try XCTUnwrap(world.controller.state.population.actor(id: actor.id))
    XCTAssertEqual(invited.relationship.activeActivity?.kind, .play)
    XCTAssertEqual(invited.relationship.sharedExperiences.map(\.kind), [.company])
    XCTAssertEqual(invited.relationship.trust, beforeInvitation)

    try world.controller.save()
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.controller.state, world.controller.state)
    let current = try XCTUnwrap(reopened.controller.state.population.actor(id: actor.id))
    let departed = current.position + SIMD2<Float>(0, WildlifePopulation.requestRange + 4)
    reopened.camera.position = SIMD3(
      departed.x, reopened.groundHeight(departed.x, departed.y) + 1.72, departed.y)
    reopened.camera.yaw = 0
    reopened.syncExpeditionPlayer()
    reopened.advance(1 / 60, running: false)

    let interrupted = try XCTUnwrap(reopened.controller.state.population.actor(id: actor.id))
    XCTAssertNil(interrupted.relationship.activeActivity)
    XCTAssertEqual(interrupted.relationship.sharedExperiences.map(\.kind), [.company])
    XCTAssertEqual(interrupted.relationship.trust, beforeInvitation)
    try reopened.controller.save()
    let finalReopen = try SanctuaryWorld(root: root)
    XCTAssertEqual(finalReopen.controller.state, reopened.controller.state)
  }

  func testStaleSpatialAndPersistenceFailuresLeaveDecisionAndRelationshipUnchanged() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try world(root: root)
    let ticket = try world.prepareCreatureInterpretation("hello", requestID: "stale")
    let proposal = try CreatureRequestProposal(ticket: ticket, request: .greeting, source: .onDeviceModel)
    _ = try world.control("flowers")
    let edited = try world.checkpoint()
    XCTAssertThrowsError(try world.acceptCreatureInterpretation(proposal, ticket: ticket))
    XCTAssertEqual(try world.checkpoint(), edited)

    let nearby = try world.prepareCreatureInterpretation("hello", requestID: "out-of-view")
    let hiddenProposal = try CreatureRequestProposal(ticket: nearby, request: .greeting, source: .onDeviceModel)
    world.camera.yaw = .pi
    let hidden = try world.checkpoint()
    XCTAssertThrowsError(try world.acceptCreatureInterpretation(hiddenProposal, ticket: nearby))
    XCTAssertEqual(try world.checkpoint(), hidden)

    world.camera.yaw = 0
    let pending = try world.prepareCreatureInterpretation("hello", requestID: "disk-failure")
    let valid = try CreatureRequestProposal(ticket: pending, request: .greeting, source: .onDeviceModel)
    let saves = root.appendingPathComponent("saves")
    try FileManager.default.removeItem(at: saves)
    try Data("not a directory".utf8).write(to: saves)
    let beforeFailure = try world.checkpoint()
    XCTAssertThrowsError(try world.acceptCreatureInterpretation(valid, ticket: pending))
    XCTAssertEqual(try world.checkpoint(), beforeFailure)
  }
}
