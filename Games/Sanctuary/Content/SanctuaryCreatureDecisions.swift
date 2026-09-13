import Foundation
import SimulationCore
import simd

extension SanctuaryWorld {
  /// Motion ticks do not invalidate a classification. Changes to willingness,
  /// target identity, capabilities, edits or accepted decisions do. Spatial
  /// checks are always repeated when the result arrives.
  public func creatureInterpretationRevision(for actor: WildlifeActor) -> UInt64 {
    let state = controller.state
    let lastEncounter = actor.relationship.encounters.last
    let activeActivity = actor.relationship.activeActivity
    let willingness = AnimalRequest.allCases.map {
      actor.relationship.isWilling(to: $0) ? "1" : "0"
    }.joined()
    var facts: [String] = [
      actor.id, actor.species.rawValue, actor.lifeStage.rawValue, willingness,
      String(actor.relationship.encounters.count), lastEncounter?.request.rawValue ?? "",
      lastEncounter?.outcome.rawValue ?? "", String(lastEncounter?.tick ?? -1),
      activeActivity?.kind.rawValue ?? "", activeActivity?.originatingRequest.rawValue ?? "",
      String(activeActivity?.startedTick ?? -1),
    ]
    facts.append(contentsOf: [
      String(actor.relationship.sharedExperiences.count),
      String(actor.relationship.returnEvents.count),
      String(actor.relationship.assistanceMemories.count),
      String(actor.companion.helpEligible), String(actor.companion.rideEligible),
      String(actor.companion.flyEligible), state.travel.mode.rawValue,
      state.travel.companionID ?? "", String(state.garden?.revision ?? 0),
      String(state.buildings.revision), String(state.movedBoulders.revision),
      String(state.decisionJournal.sequence),
    ])
    return facts.joined(separator: "|").utf8.reduce(UInt64(14_695_981_039_346_656_037)) {
      ($0 ^ UInt64($1)) &* 1_099_511_628_211
    }
  }

  /// Pending inference is transient. Saves contain accepted decisions, never a
  /// task that would need a model to resume after reopening.
  public func prepareCreatureInterpretation(_ text: String, requestID: String) throws
    -> CreatureRequestTicket
  {
    guard let actor = nearbyAnimal else { throw WildlifePopulationError.notVisible }
    let journal = controller.state.decisionJournal
    return try journal.makeTicket(requestID: requestID, targetID: actor.id,
      worldRevision: creatureInterpretationRevision(for: actor), playerText: text)
  }

  @discardableResult public func acceptCreatureInterpretation(
    _ proposal: CreatureRequestProposal, ticket: CreatureRequestTicket
  ) throws -> RecordedCreatureDecision {
    try ticket.validate()
    guard let actor = controller.state.population.actor(id: ticket.targetID),
      ticket.journalSequence == controller.state.decisionJournal.sequence
    else { throw CreatureDecisionJournalError.staleWorldRevision }
    var journal = controller.state.decisionJournal
    let captured = try journal.makeTicket(requestID: ticket.requestID, targetID: ticket.targetID,
      worldRevision: ticket.worldRevision, playerText: ticket.playerText,
      allowedRequests: ticket.allowedRequests)
    guard captured == ticket else { throw CreatureDecisionJournalError.invalidTicket }
    let decision = try journal.accept(proposal, for: ticket,
      currentWorldRevision: creatureInterpretationRevision(for: actor))
    try applyExternalDecision(SimulationCoding.encode(decision))
    return decision
  }

  /// Native completion and replay both commit this typed record through the
  /// ordinary game-owned target, perception, willingness and save transaction.
  public func applyExternalDecision(_ data: Data) throws {
    guard !data.isEmpty, data.count <= 16_384 else {
      throw SimulationFailure.invalid("External decision exceeds transport bounds")
    }
    let recorded = try JSONDecoder().decode(RecordedCreatureDecision.self, from: data)
    guard let actor = controller.state.population.actor(id: recorded.targetID),
      recorded.sequence == controller.state.decisionJournal.sequence else {
      throw CreatureDecisionJournalError.staleWorldRevision
    }
    let revision = creatureInterpretationRevision(for: actor)
    syncExpeditionPlayer()
    try controller.editLiving { state in
      var journal = state.decisionJournal
      let decision = try journal.acceptRecorded(recorded, currentWorldRevision: revision)
      guard decision == recorded else { throw CreatureDecisionJournalError.invalidProposal }
      state.creatureDecisions = journal
      if decision.disposition == .applied {
        var population = state.population
        let result = try population.address(decision.request, targetID: decision.targetID,
          player: state.player, expectedRevision: population.revision, isVisible: animalVisible)
        state.wildlife = population
        var journey = state.travel
        journey.observe(animalID: result.animalID)
        state.journey = journey
      }
    }
  }
}
