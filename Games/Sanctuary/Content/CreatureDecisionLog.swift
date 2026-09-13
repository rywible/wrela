import Foundation

public enum CreatureDecisionJournalError: Error, Equatable, LocalizedError, Sendable {
  case invalidTicket
  case invalidProposal
  case invalidJournal
  case duplicateRequestID
  case mismatchedProposal
  case unsupportedRequest
  case staleWorldRevision

  public var errorDescription: String? {
    switch self {
    case .invalidTicket: return "The creature interpretation request is malformed."
    case .invalidProposal: return "The creature interpretation proposal is malformed."
    case .invalidJournal: return "The creature decision journal is malformed."
    case .duplicateRequestID: return "That creature interpretation request was already recorded."
    case .mismatchedProposal:
      return "The creature interpretation proposal does not match its request."
    case .unsupportedRequest:
      return "The proposed animal request was not allowed when interpretation began."
    case .staleWorldRevision:
      return "Relevant creature or world facts changed during interpretation."
    }
  }
}

/// Game-owned input captured before optional asynchronous interpretation. The
/// revision represents only facts relevant to this interaction, not the
/// population's per-tick revision.
public struct CreatureRequestTicket: Codable, Equatable, Sendable {
  public static let maximumTextLength = 160

  public let requestID: String
  public let targetID: String
  public let worldRevision: UInt64
  public let journalSequence: UInt64
  public let playerText: String
  public let allowedRequests: [AnimalRequest]

  public init(
    requestID: String, targetID: String, worldRevision: UInt64, journalSequence: UInt64,
    playerText: String, allowedRequests: [AnimalRequest]
  ) throws {
    self.requestID = requestID
    self.targetID = targetID
    self.worldRevision = worldRevision
    self.journalSequence = journalSequence
    self.playerText = playerText
    self.allowedRequests = allowedRequests
    try validate()
  }

  public func validate() throws {
    let trimmed = playerText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !requestID.isEmpty, requestID.utf8.count <= 128,
      !targetID.isEmpty, targetID.utf8.count <= 64,
      !trimmed.isEmpty, playerText.count <= Self.maximumTextLength,
      !allowedRequests.isEmpty,
      Set(allowedRequests.map(\.rawValue)).count == allowedRequests.count
    else { throw CreatureDecisionJournalError.invalidTicket }
  }
}

/// The source is descriptive replay metadata. Both cases remain untrusted
/// proposals until the game accepts them against the captured ticket.
public enum CreatureProposalSource: String, Codable, Equatable, Sendable {
  case authoredParser
  case onDeviceModel
}

/// The game's deterministic qualification of a typed proposal. Rejection is a
/// journal fact, but never an animal request or relationship encounter.
public enum CreatureDecisionDisposition: String, Codable, Equatable, Sendable {
  case applied, rejected
}

/// A typed interpretation result. Identifiers and revision must echo the
/// captured ticket; the interpreter never gets authority to choose them.
public struct CreatureRequestProposal: Codable, Equatable, Sendable {
  public let requestID: String
  public let targetID: String
  public let worldRevision: UInt64
  public let request: AnimalRequest
  public let source: CreatureProposalSource

  public init(
    requestID: String, targetID: String, worldRevision: UInt64, request: AnimalRequest,
    source: CreatureProposalSource
  ) throws {
    self.requestID = requestID
    self.targetID = targetID
    self.worldRevision = worldRevision
    self.request = request
    self.source = source
    try validate()
  }

  public init(
    ticket: CreatureRequestTicket, request: AnimalRequest, source: CreatureProposalSource
  ) throws {
    try self.init(
      requestID: ticket.requestID, targetID: ticket.targetID,
      worldRevision: ticket.worldRevision, request: request, source: source)
  }

  public func validate() throws {
    guard !requestID.isEmpty, requestID.utf8.count <= 128,
      !targetID.isEmpty, targetID.utf8.count <= 64
    else { throw CreatureDecisionJournalError.invalidProposal }
  }
}

/// An immutable game-qualified interpretation. Replay consumes this result at
/// its recorded simulation event and never asks an interpreter again.
public struct RecordedCreatureDecision: Codable, Equatable, Sendable {
  public let sequence: UInt64
  public let requestID: String
  public let targetID: String
  public let worldRevision: UInt64
  public let playerText: String
  public let allowedRequests: [AnimalRequest]
  public let request: AnimalRequest
  public let source: CreatureProposalSource
  /// Nil is reserved for decisions written before disposition was persisted;
  /// those retain their historical applied replay behavior.
  public let recordedDisposition: CreatureDecisionDisposition?
  public var disposition: CreatureDecisionDisposition { recordedDisposition ?? .applied }

  fileprivate init(
    ticket: CreatureRequestTicket, proposal: CreatureRequestProposal,
    disposition: CreatureDecisionDisposition?
  ) {
    sequence = ticket.journalSequence
    requestID = ticket.requestID
    targetID = ticket.targetID
    worldRevision = ticket.worldRevision
    playerText = ticket.playerText
    allowedRequests = ticket.allowedRequests
    request = proposal.request
    source = proposal.source
    recordedDisposition = disposition
  }

  fileprivate func validate() throws {
    guard !requestID.isEmpty, requestID.utf8.count <= 128,
      !targetID.isEmpty, targetID.utf8.count <= 64,
      !playerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
      playerText.count <= CreatureRequestTicket.maximumTextLength,
      !allowedRequests.isEmpty,
      Set(allowedRequests.map(\.rawValue)).count == allowedRequests.count,
      allowedRequests.contains(request),
      recordedDisposition.map({ disposition in
        let permitted = AnimalRequestParser.permits(request, for: playerText)
        return disposition == (permitted ? .applied : .rejected)
      }) ?? true
    else { throw CreatureDecisionJournalError.invalidJournal }
  }

  /// Reconstructs the exact bounded input used to validate this result.
  /// Replay still checks current game facts before applying the action.
  public func replayTicket() throws -> CreatureRequestTicket {
    try CreatureRequestTicket(
      requestID: requestID, targetID: targetID, worldRevision: worldRevision,
      journalSequence: sequence, playerText: playerText, allowedRequests: allowedRequests)
  }

  public func replayProposal() throws -> CreatureRequestProposal {
    try CreatureRequestProposal(
      requestID: requestID, targetID: targetID, worldRevision: worldRevision,
      request: request, source: source)
  }
}

/// Bounded game-owned history for asynchronous interpretation. Tickets remain
/// ephemeral during inference. Qualified applied and semantic-rejection results
/// are persisted; malformed, duplicate and stale completions do not mutate it.
public struct CreatureDecisionJournal: Codable, Equatable, Sendable {
  public static let maximumRecordedDecisions = 64

  public private(set) var sequence: UInt64
  public private(set) var decisions: [RecordedCreatureDecision]

  public init() {
    sequence = 0
    decisions = []
  }

  /// Captures a request without changing persistent state. The coordinator owns
  /// the bounded in-flight task; the shared interpreter actor serializes model use.
  public func makeTicket(
    requestID: String, targetID: String, worldRevision: UInt64, playerText: String,
    allowedRequests: [AnimalRequest] = AnimalRequest.allCases
  ) throws -> CreatureRequestTicket {
    guard sequence < .max else { throw CreatureDecisionJournalError.invalidJournal }
    guard !contains(requestID: requestID) else {
      throw CreatureDecisionJournalError.duplicateRequestID
    }
    return try CreatureRequestTicket(
      requestID: requestID, targetID: targetID, worldRevision: worldRevision,
      journalSequence: sequence, playerText: playerText, allowedRequests: allowedRequests)
  }

  /// Qualifies a proposal for the current relevant facts. The population's
  /// current range, visibility, ability and per-tick revision remain separate
  /// checks in the same enclosing game transaction.
  @discardableResult public mutating func accept(
    _ proposal: CreatureRequestProposal, for ticket: CreatureRequestTicket,
    currentWorldRevision: UInt64
  ) throws -> RecordedCreatureDecision {
    try ticket.validate()
    try proposal.validate()
    if contains(requestID: ticket.requestID) {
      throw CreatureDecisionJournalError.duplicateRequestID
    }
    guard proposal.requestID == ticket.requestID,
      proposal.targetID == ticket.targetID,
      proposal.worldRevision == ticket.worldRevision
    else { throw CreatureDecisionJournalError.mismatchedProposal }
    guard ticket.allowedRequests.contains(proposal.request) else {
      throw CreatureDecisionJournalError.unsupportedRequest
    }
    guard sequence < .max, ticket.journalSequence == sequence,
      currentWorldRevision == ticket.worldRevision
    else { throw CreatureDecisionJournalError.staleWorldRevision }

    let disposition: CreatureDecisionDisposition =
      AnimalRequestParser.permits(proposal.request, for: ticket.playerText) ? .applied : .rejected
    return try record(
      proposal, for: ticket, currentWorldRevision: currentWorldRevision,
      disposition: disposition)
  }

  /// Replays an already-qualified record. Legacy records without a disposition
  /// preserve their historical applied result; new records must still agree with
  /// today's deterministic qualification before any journal mutation.
  @discardableResult mutating func acceptRecorded(
    _ recorded: RecordedCreatureDecision, currentWorldRevision: UInt64
  ) throws -> RecordedCreatureDecision {
    try recorded.validate()
    let ticket = try recorded.replayTicket()
    let proposal = try recorded.replayProposal()
    let disposition: CreatureDecisionDisposition
    if let explicit = recorded.recordedDisposition {
      let current: CreatureDecisionDisposition =
        AnimalRequestParser.permits(proposal.request, for: ticket.playerText) ? .applied : .rejected
      guard current == explicit else { throw CreatureDecisionJournalError.invalidProposal }
      disposition = explicit
    } else {
      disposition = .applied
    }
    return try record(
      proposal, for: ticket, currentWorldRevision: currentWorldRevision,
      disposition: disposition, preserveLegacyDisposition: recorded.recordedDisposition == nil)
  }

  private mutating func record(
    _ proposal: CreatureRequestProposal, for ticket: CreatureRequestTicket,
    currentWorldRevision: UInt64, disposition: CreatureDecisionDisposition,
    preserveLegacyDisposition: Bool = false
  ) throws -> RecordedCreatureDecision {
    try ticket.validate()
    try proposal.validate()
    if contains(requestID: ticket.requestID) {
      throw CreatureDecisionJournalError.duplicateRequestID
    }
    guard proposal.requestID == ticket.requestID,
      proposal.targetID == ticket.targetID,
      proposal.worldRevision == ticket.worldRevision
    else { throw CreatureDecisionJournalError.mismatchedProposal }
    guard ticket.allowedRequests.contains(proposal.request) else {
      throw CreatureDecisionJournalError.unsupportedRequest
    }
    guard sequence < .max, ticket.journalSequence == sequence,
      currentWorldRevision == ticket.worldRevision
    else { throw CreatureDecisionJournalError.staleWorldRevision }

    let decision = RecordedCreatureDecision(
      ticket: ticket, proposal: proposal,
      disposition: preserveLegacyDisposition ? nil : disposition)
    var candidate = self
    candidate.decisions.append(decision)
    if candidate.decisions.count > Self.maximumRecordedDecisions {
      candidate.decisions.removeFirst(
        candidate.decisions.count - Self.maximumRecordedDecisions)
    }
    candidate.sequence += 1
    try candidate.validate()
    self = candidate
    return decision
  }

  /// Deterministic replay reads the accepted typed action directly instead of
  /// recreating the original parser or model request.
  public func recordedDecision(sequence requestedSequence: UInt64)
    -> RecordedCreatureDecision?
  {
    decisions.first { $0.sequence == requestedSequence }
  }

  public func validate() throws {
    let expectedCount =
      sequence < UInt64(Self.maximumRecordedDecisions)
      ? Int(sequence) : Self.maximumRecordedDecisions
    guard decisions.count == expectedCount,
      decisions.first?.sequence == (sequence == 0 ? nil : sequence - UInt64(expectedCount)),
      decisions.last?.sequence == (sequence == 0 ? nil : sequence - 1)
    else { throw CreatureDecisionJournalError.invalidJournal }
    var previousSequence: UInt64?
    for decision in decisions {
      try decision.validate()
      guard decision.sequence < sequence,
        previousSequence.map({ $0 < decision.sequence }) ?? true
      else { throw CreatureDecisionJournalError.invalidJournal }
      previousSequence = decision.sequence
    }
    let allIDs = decisions.map(\.requestID)
    guard Set(allIDs).count == allIDs.count else {
      throw CreatureDecisionJournalError.invalidJournal
    }
  }

  private func contains(requestID: String) -> Bool {
    decisions.contains { $0.requestID == requestID }
  }

  private enum CodingKeys: String, CodingKey { case sequence, decisions }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    sequence = try container.decode(UInt64.self, forKey: .sequence)
    decisions = try container.decode([RecordedCreatureDecision].self, forKey: .decisions)
    try validate()
  }
}
