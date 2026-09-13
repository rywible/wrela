import Foundation
import XCTest

@testable import SanctuaryContent

final class CreatureDecisionLogTests: XCTestCase {
  private func proposal(
    for ticket: CreatureRequestTicket, request: AnimalRequest = .greeting,
    source: CreatureProposalSource = .onDeviceModel
  ) throws -> CreatureRequestProposal {
    try CreatureRequestProposal(ticket: ticket, request: request, source: source)
  }

  private func assertJournalError<T>(
    _ expected: CreatureDecisionJournalError,
    _ operation: () throws -> T,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    XCTAssertThrowsError(try operation(), file: file, line: line) { error in
      XCTAssertEqual(error as? CreatureDecisionJournalError, expected, file: file, line: line)
    }
  }

  func testTicketCapturesBoundedAuthoredFactsAndJournalRoundTrips() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "request-001", targetID: "sunhare-001", worldRevision: 91,
      playerText: "Would you stay here for a moment?", allowedRequests: [.wait, .greeting])

    XCTAssertEqual(ticket.requestID, "request-001")
    XCTAssertEqual(ticket.targetID, "sunhare-001")
    XCTAssertEqual(ticket.worldRevision, 91)
    XCTAssertEqual(ticket.journalSequence, 0)
    XCTAssertEqual(ticket.allowedRequests, [.wait, .greeting])
    XCTAssertEqual(journal.sequence, 0)
    XCTAssertTrue(journal.decisions.isEmpty)

    let restored = try JSONDecoder().decode(
      CreatureDecisionJournal.self, from: JSONEncoder().encode(journal))
    XCTAssertEqual(restored, journal)
    try restored.validate()
  }

  func testAcceptRecordsImmutableTypedDecisionForReplay() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "request-001", targetID: "sunhare-001", worldRevision: 41,
      playerText: "come closer", allowedRequests: [.come])
    let decision = try journal.accept(
      proposal(for: ticket, request: .come), for: ticket, currentWorldRevision: 41)

    XCTAssertEqual(decision.sequence, 0)
    XCTAssertEqual(decision.requestID, ticket.requestID)
    XCTAssertEqual(decision.targetID, ticket.targetID)
    XCTAssertEqual(decision.worldRevision, ticket.worldRevision)
    XCTAssertEqual(decision.request, .come)
    XCTAssertEqual(decision.source, .onDeviceModel)
    XCTAssertEqual(decision.disposition, .applied)
    XCTAssertEqual(journal.sequence, 1)
    XCTAssertEqual(journal.recordedDecision(sequence: 0), decision)
    XCTAssertEqual(try decision.replayTicket(), ticket)
    XCTAssertEqual(try decision.replayProposal(), try proposal(for: ticket, request: .come))

    var replayJournal = CreatureDecisionJournal()
    let replayed = try replayJournal.accept(
      decision.replayProposal(), for: decision.replayTicket(), currentWorldRevision: 41)
    XCTAssertEqual(replayed, decision)
    XCTAssertEqual(replayJournal, journal)

    let restored = try JSONDecoder().decode(
      CreatureDecisionJournal.self, from: JSONEncoder().encode(journal))
    XCTAssertEqual(restored.recordedDecision(sequence: 0), decision)
  }

  func testSemanticRejectionIsRecordedAndReplaysWithoutBecomingAnAction() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "contradictory", targetID: "sunhare-001", worldRevision: 18,
      playerText: "follow me but stay here")
    let decision = try journal.accept(
      proposal(for: ticket, request: .follow), for: ticket, currentWorldRevision: 18)

    XCTAssertEqual(decision.disposition, .rejected)
    XCTAssertEqual(journal.sequence, 1)
    XCTAssertEqual(journal.decisions, [decision])
    let restored = try JSONDecoder().decode(
      CreatureDecisionJournal.self, from: JSONEncoder().encode(journal))
    XCTAssertEqual(restored, journal)

    var replay = CreatureDecisionJournal()
    XCTAssertEqual(
      try replay.acceptRecorded(decision, currentWorldRevision: 18), decision)
    XCTAssertEqual(replay, journal)
  }

  func testLegacyDecisionWithoutDispositionRetainsAppliedReplay() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "legacy", targetID: "sunhare-001", worldRevision: 3,
      playerText: "hello")
    _ = try journal.accept(
      proposal(for: ticket), for: ticket, currentWorldRevision: 3)
    var object = try XCTUnwrap(
      JSONSerialization.jsonObject(with: JSONEncoder().encode(journal)) as? [String: Any])
    var decisions = try XCTUnwrap(object["decisions"] as? [[String: Any]])
    decisions[0].removeValue(forKey: "recordedDisposition")
    object["decisions"] = decisions

    let legacy = try JSONDecoder().decode(
      CreatureDecisionJournal.self, from: JSONSerialization.data(withJSONObject: object))
    let decision = try XCTUnwrap(legacy.decisions.first)
    XCTAssertNil(decision.recordedDisposition)
    XCTAssertEqual(decision.disposition, .applied)
    var replay = CreatureDecisionJournal()
    XCTAssertEqual(try replay.acceptRecorded(decision, currentWorldRevision: 3), decision)
  }

  func testNegatedAndUnknownPhysicalTextCannotRecordAppliedDisposition() throws {
    for (text, request): (String, AnimalRequest) in [
      ("don't follow me", .follow), ("please fly to me", .come),
    ] {
      var journal = CreatureDecisionJournal()
      let ticket = try journal.makeTicket(
        requestID: request.rawValue, targetID: "sunhare-001", worldRevision: 9,
        playerText: text)
      let decision = try journal.accept(
        proposal(for: ticket, request: request), for: ticket, currentWorldRevision: 9)
      XCTAssertEqual(decision.disposition, .rejected, text)
    }
  }

  func testDuplicateRequestIDsAreRejectedWithoutMutation() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "same-id", targetID: "sunhare-001", worldRevision: 7,
      playerText: "hello")
    _ = try journal.accept(
      proposal(for: ticket, source: .authoredParser), for: ticket, currentWorldRevision: 7)
    let before = journal
    assertJournalError(.duplicateRequestID) {
      try journal.makeTicket(
        requestID: "same-id", targetID: "sunhare-001", worldRevision: 8,
        playerText: "hello")
    }
    XCTAssertEqual(journal, before)
  }

  func testMalformedAndDuplicateAllowedInputsAreRejected() throws {
    let journal = CreatureDecisionJournal()
    assertJournalError(.invalidTicket) {
      try journal.makeTicket(
        requestID: "", targetID: "sunhare-001", worldRevision: 1, playerText: "hello")
    }
    assertJournalError(.invalidTicket) {
      try journal.makeTicket(
        requestID: "request-001", targetID: "sunhare-001", worldRevision: 1,
        playerText: String(repeating: "x", count: CreatureRequestTicket.maximumTextLength + 1))
    }
    assertJournalError(.invalidTicket) {
      try journal.makeTicket(
        requestID: "request-001", targetID: "sunhare-001", worldRevision: 1,
        playerText: "hello", allowedRequests: [.greeting, .greeting])
    }
  }

  func testMismatchedUnsupportedAndStaleProposalsAreAtomic() throws {
    var journal = CreatureDecisionJournal()
    let ticket = try journal.makeTicket(
      requestID: "request-001", targetID: "sunhare-001", worldRevision: 12,
      playerText: "Would you wait?", allowedRequests: [.wait])
    let before = journal

    let wrongTarget = try CreatureRequestProposal(
      requestID: ticket.requestID, targetID: "dunefox-001",
      worldRevision: ticket.worldRevision, request: .wait, source: .onDeviceModel)
    assertJournalError(.mismatchedProposal) {
      try journal.accept(wrongTarget, for: ticket, currentWorldRevision: 12)
    }
    XCTAssertEqual(journal, before)

    let unsupported = try proposal(for: ticket, request: .follow)
    assertJournalError(.unsupportedRequest) {
      try journal.accept(unsupported, for: ticket, currentWorldRevision: 12)
    }
    XCTAssertEqual(journal, before)

    assertJournalError(.staleWorldRevision) {
      try journal.accept(
        try proposal(for: ticket, request: .wait), for: ticket, currentWorldRevision: 13)
    }
    XCTAssertEqual(journal, before)
  }

  func testOneAcceptedDecisionInvalidatesConcurrentTicketsAtTheOldSequence() throws {
    var journal = CreatureDecisionJournal()
    let first = try journal.makeTicket(
      requestID: "first", targetID: "sunhare-001", worldRevision: 100,
      playerText: "hello")
    let second = try journal.makeTicket(
      requestID: "second", targetID: "sunhare-002", worldRevision: 101,
      playerText: "hello")

    _ = try journal.accept(proposal(for: first), for: first, currentWorldRevision: 100)
    assertJournalError(.staleWorldRevision) {
      try journal.accept(try proposal(for: second), for: second, currentWorldRevision: 101)
    }
  }

  func testRecordedHistoryStaysBounded() throws {
    var history = CreatureDecisionJournal()
    for index in 0...CreatureDecisionJournal.maximumRecordedDecisions {
      let revision = UInt64(index)
      let ticket = try history.makeTicket(
        requestID: "recorded-\(index)", targetID: "sunhare-001", worldRevision: revision,
        playerText: "hello")
      _ = try history.accept(
        proposal(for: ticket, source: .authoredParser), for: ticket,
        currentWorldRevision: revision)
    }
    XCTAssertEqual(history.decisions.count, CreatureDecisionJournal.maximumRecordedDecisions)
    XCTAssertNil(history.recordedDecision(sequence: 0))
    XCTAssertEqual(history.decisions.first?.sequence, 1)
    XCTAssertEqual(
      history.decisions.last?.sequence,
      UInt64(CreatureDecisionJournal.maximumRecordedDecisions))
    try history.validate()
  }

  func testMalformedDecodedJournalIsRejected() throws {
    let malformed = """
      {
        "sequence": 1,
        "decisions": [{
          "sequence": 0,
          "requestID": "",
          "targetID": "sunhare-001",
          "worldRevision": 1,
          "playerText": "hello",
          "allowedRequests": ["greeting"],
          "request": "greeting",
          "source": "onDeviceModel"
        }]
      }
      """
    XCTAssertThrowsError(
      try JSONDecoder().decode(
        CreatureDecisionJournal.self, from: Data(malformed.utf8)))

    let missingHistory = #"{"sequence":1,"decisions":[]}"#
    XCTAssertThrowsError(
      try JSONDecoder().decode(
        CreatureDecisionJournal.self, from: Data(missingHistory.utf8)))
  }
}
