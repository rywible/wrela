import Foundation
import simd

public enum AnimalTemperament: String, Codable, CaseIterable, Sendable {
  case cautious, curious, proud, wild
}

public enum AnimalRequest: String, Codable, CaseIterable, Sendable {
  case greeting, follow, wait, come, play
}

/// Authored fallback for typed interaction. This is deliberately a small phrase
/// recognizer, not a language model or a source of simulation facts.
public enum AnimalRequestParser {
  public static func parse(_ text: String) -> AnimalRequest? {
    let words = normalizedWords(text)
    guard !words.isEmpty, !containsNegation(words), !containsUnsupportedAction(words) else {
      return nil
    }
    let requests = mentionedRequests(in: words)
    guard requests.count == 1 else { return nil }

    var normalized = words.joined(separator: " ")
    if normalized.hasPrefix("please ") { normalized.removeFirst("please ".count) }
    if normalized.hasSuffix(" please") { normalized.removeLast(" please".count) }
    switch normalized {
    case "hello", "hello there", "hi", "greet", "greetings": return .greeting
    case "follow", "follow me", "walk with me": return .follow
    case "wait", "wait here", "stay", "stay here": return .wait
    case "come", "come here", "come closer": return .come
    case "play", "play with me", "let us play", "want to play", "do you want to play":
      return .play
    default:
      // Longer social wording is harmless when it starts with a greeting and
      // contains no second request or unsupported physical command.
      if requests == [.greeting], ["hello", "hi", "greetings"].contains(words[0]) {
        return .greeting
      }
      return nil
    }
  }

  /// A model proposal may only narrow text which the authored vocabulary reads
  /// as the same single request. This prevents a generated classification from
  /// turning negation, conflicting commands, or unknown actions into behavior.
  public static func permits(_ request: AnimalRequest, for text: String) -> Bool {
    parse(text) == request
  }

  private static func normalizedWords(_ text: String) -> [String] {
    String(text.lowercased().map { $0.isLetter || $0.isNumber ? $0 : " " })
      .split(whereSeparator: \Character.isWhitespace).map(String.init)
  }

  private static func containsPhrase(_ phrase: [String], in words: [String]) -> Bool {
    guard words.count >= phrase.count else { return false }
    return (0...(words.count - phrase.count)).contains { start in
      Array(words[start..<(start + phrase.count)]) == phrase
    }
  }

  private static func mentionedRequests(in words: [String]) -> Set<AnimalRequest> {
    var requests = Set<AnimalRequest>()
    if words.contains(where: { ["hello", "hi", "greet", "greetings"].contains($0) }) {
      requests.insert(.greeting)
    }
    if words.contains("follow") || containsPhrase(["walk", "with", "me"], in: words) {
      requests.insert(.follow)
    }
    if words.contains("wait") || words.contains("stay") { requests.insert(.wait) }
    if words.contains("come") { requests.insert(.come) }
    if words.contains("play") { requests.insert(.play) }
    return requests
  }

  private static func containsNegation(_ words: [String]) -> Bool {
    if words.contains(where: { ["no", "not", "never", "dont"].contains($0) }) {
      return true
    }
    return containsPhrase(["don", "t"], in: words)
  }

  private static func containsUnsupportedAction(_ words: [String]) -> Bool {
    let unsupported: Set<String> = [
      "attack", "bite", "build", "carry", "dig", "fetch", "fight", "fly", "guard",
      "hunt", "jump", "move", "pull", "push", "ride", "swim",
    ]
    return words.contains(where: unsupported.contains)
  }
}

public struct AnimalPreferences: Codable, Equatable, Sendable {
  public var temperament: AnimalTemperament
  public var favoriteRequest: AnimalRequest
  public var sociability: Float
  public var playfulness: Float
  public var companionWilling: Bool

  public func validate() throws {
    guard sociability.isFinite, playfulness.isFinite, (0...1).contains(sociability),
      (0...1).contains(playfulness)
    else { throw ExpeditionError.invalidSave }
  }
}

public struct AnimalEncounter: Codable, Equatable, Sendable {
  public enum Outcome: String, Codable, Sendable { case accepted, refused }
  public var tick: Int
  public var request: AnimalRequest
  public var outcome: Outcome
}

/// A shared experience has to unfold in the ordinary simulation before it can
/// become relationship history. The request is only an invitation to begin it.
public enum AnimalSharedActivityKind: String, Codable, CaseIterable, Sendable {
  case company, play, exploration
}

public struct AnimalSharedActivityProgress: Codable, Equatable, Sendable {
  public let kind: AnimalSharedActivityKind
  public let originatingRequest: AnimalRequest
  public let startedTick: Int
  public private(set) var activeTicks: Int
  public private(set) var distanceTravelled: Float
  public private(set) var lastPosition: SIMD2<Float>

  fileprivate init(
    kind: AnimalSharedActivityKind, originatingRequest: AnimalRequest, startedTick: Int,
    position: SIMD2<Float>
  ) {
    self.kind = kind
    self.originatingRequest = originatingRequest
    self.startedTick = startedTick
    activeTicks = 0
    distanceTravelled = 0
    lastPosition = position
  }

  fileprivate mutating func advance(tick: Int, position: SIMD2<Float>) {
    activeTicks += max(0, tick - (startedTick + activeTicks))
    distanceTravelled += distance(lastPosition, position)
    lastPosition = position
  }

  fileprivate mutating func rebase(at position: SIMD2<Float>) {
    lastPosition = position
  }

  fileprivate func validate() throws {
    guard (0...1_000_000_000).contains(startedTick), activeTicks >= 0,
      activeTicks <= 1_000_000_000 - startedTick,
      distanceTravelled.isFinite, (0...20_000).contains(distanceTravelled),
      lastPosition.x.isFinite, lastPosition.y.isFinite,
      abs(lastPosition.x) < 20_000, abs(lastPosition.y) < 20_000
    else { throw ExpeditionError.invalidSave }
  }
}

public struct AnimalSharedExperience: Codable, Equatable, Sendable {
  public let kind: AnimalSharedActivityKind
  public let originatingRequest: AnimalRequest
  public let completedTick: Int
  public let activeTicks: Int
  public let distanceTravelled: Float

  fileprivate func validate() throws {
    guard (0...1_000_000_000).contains(completedTick), activeTicks > 0,
      activeTicks <= completedTick, distanceTravelled.isFinite,
      (0...20_000).contains(distanceTravelled)
    else { throw ExpeditionError.invalidSave }
  }
}

public enum AnimalReturnResponse: String, Codable, CaseIterable, Sendable {
  case acknowledged, approached
}

/// A bounded record that the same animal experienced a real absence and return.
/// It is an observed simulation fact, not dialogue or a population-wide claim.
public struct AnimalReturnEvent: Codable, Equatable, Sendable {
  public let departedTick: Int
  public let returnedTick: Int
  public let response: AnimalReturnResponse

  fileprivate func validate() throws {
    guard (0...1_000_000_000).contains(departedTick),
      (departedTick...1_000_000_000).contains(returnedTick),
      returnedTick - departedTick >= AnimalRelationship.returnAbsenceTicks
    else { throw ExpeditionError.invalidSave }
  }
}

public enum WildlifeAssistanceKind: String, Codable, CaseIterable, Sendable {
  case habitatRestoration, boulderMovement
}

/// A game action supplies this only after its concrete outcome succeeds. Stable
/// outcome identity prevents retries from manufacturing relationship history.
public struct WildlifeAssistanceMemory: Codable, Equatable, Sendable {
  public let outcomeID: String
  public let kind: WildlifeAssistanceKind
  public let completedTick: Int

  fileprivate func validate() throws {
    guard !outcomeID.isEmpty, outcomeID.count <= 128,
      outcomeID == outcomeID.trimmingCharacters(in: .whitespacesAndNewlines),
      (0...1_000_000_000).contains(completedTick)
    else { throw ExpeditionError.invalidSave }
  }
}

/// Persistent facts about one animal. Preferences are baked once and memories
/// contain only interactions that the production simulation actually applied.
public struct AnimalRelationship: Codable, Equatable, Sendable {
  public static let memoryLimit = 24
  public static let sharedExperienceLimit = 12
  public static let returnEventLimit = 8
  public static let assistanceMemoryLimit = 12
  /// Uninvited calm presence establishes recognition but cannot by itself
  /// unlock play or following, even for receptive authored preferences.
  public static let familiarityCap: Float = 0.1
  /// Provisional familiarity pacing: twelve calm seconds reach its cap.
  public static let familiaritySeconds: Float = 12
  /// Immediate request recognition is at most once per five seconds at 60 Hz.
  /// Every request remains responsive and is still recorded during this window.
  public static let trustRewardCooldownTicks = 300
  public static let companyCompletionTicks = 600
  public static let playCompletionTicks = 150
  public static let explorationCompletionTicks = 600
  /// One full authored stride is enough movement evidence when an invitation
  /// arrives during an already-running hop.
  public static let playCompletionDistance: Float = 0.6
  public static let explorationCompletionDistance: Float = 8
  public static let returnDepartureDistance: Float = 24
  public static let returnRecognitionDistance: Float = 8
  public static let returnAbsenceTicks = 600
  public static let returnResponseCooldownTicks = 3_600
  public var id: String
  public var preferences: AnimalPreferences
  public private(set) var trust: Float
  public private(set) var encounters: [AnimalEncounter]
  public private(set) var activeActivity: AnimalSharedActivityProgress?
  public private(set) var sharedExperiences: [AnimalSharedExperience]
  public private(set) var awaySinceTick: Int?
  public private(set) var returnEvents: [AnimalReturnEvent]
  public private(set) var mountedExplorationActive: Bool
  public private(set) var assistanceMemories: [WildlifeAssistanceMemory]

  private init(
    id: String, preferences: AnimalPreferences, trust: Float, encounters: [AnimalEncounter],
    activeActivity: AnimalSharedActivityProgress? = nil,
    sharedExperiences: [AnimalSharedExperience] = [], awaySinceTick: Int? = nil,
    returnEvents: [AnimalReturnEvent] = [], mountedExplorationActive: Bool = false,
    assistanceMemories: [WildlifeAssistanceMemory] = []
  ) {
    self.id = id
    self.preferences = preferences
    self.trust = trust
    self.encounters = encounters
    self.activeActivity = activeActivity
    self.sharedExperiences = sharedExperiences
    self.awaySinceTick = awaySinceTick
    self.returnEvents = returnEvents
    self.mountedExplorationActive = mountedExplorationActive
    self.assistanceMemories = assistanceMemories
  }

  public static func baked(
    id: String, seed: UInt32 = 17, trust: Float = 0, companionWilling: Bool? = nil
  )
    -> AnimalRelationship
  {
    var value: UInt32 = 2_166_136_261
    for byte in id.utf8 {
      value = (value ^ UInt32(byte)) &* 16_777_619
    }
    value = (value ^ seed) &* 16_777_619
    func next() -> UInt32 {
      value = value &* 1_664_525 &+ 1_013_904_223
      return value
    }
    let temperaments = AnimalTemperament.allCases
    let requests: [AnimalRequest] = [.greeting, .play, .come, .wait]
    let preferences = AnimalPreferences(
      temperament: temperaments[Int(next() % UInt32(temperaments.count))],
      favoriteRequest: requests[Int(next() % UInt32(requests.count))],
      sociability: 0.25 + Float(next() % 701) / 1000,
      playfulness: 0.2 + Float(next() % 751) / 1000,
      // Roughly one in five baked individuals is open to companionship.
      companionWilling: companionWilling ?? (next() % 5 == 2))
    return AnimalRelationship(
      id: id, preferences: preferences, trust: min(1, max(0, trust)), encounters: [])
  }

  public mutating func observeCalmPresence(seconds: Float) {
    guard seconds.isFinite, seconds > 0 else { return }
    guard trust < Self.familiarityCap else { return }
    trust = min(
      Self.familiarityCap,
      trust + seconds * Self.familiarityCap / Self.familiaritySeconds)
  }

  public func isWilling(to request: AnimalRequest) -> Bool {
    if request == .greeting { return true }
    var threshold: Float
    switch request {
    case .greeting: threshold = 0
    case .wait: threshold = 0.1
    case .come: threshold = 0.3
    case .play: threshold = max(0.35, 0.62 - preferences.playfulness * 0.3)
    case .follow: threshold = 0.78
    }
    switch preferences.temperament {
    case .cautious: threshold += 0.08
    case .curious: threshold -= 0.08
    case .proud: threshold += request == preferences.favoriteRequest ? -0.08 : 0.06
    case .wild: threshold += 0.14
    }
    // A completed welcomed visit is a concrete basis for trying play. The cap
    // remains individual: less playful animals ask for more familiarity.
    if request == .play, sharedExperiences.contains(where: { $0.kind == .company }) {
      threshold = min(threshold, 0.12 + (1 - preferences.playfulness) * 0.02)
    }
    threshold -= (preferences.sociability - 0.5) * 0.2
    if request == preferences.favoriteRequest { threshold -= 0.05 }
    if request == .follow && !preferences.companionWilling { return false }
    return trust >= min(1, max(0, threshold))
  }

  public mutating func record(_ request: AnimalRequest, accepted: Bool, tick: Int) {
    let rewardReady = appendEncounter(request, accepted: accepted, tick: tick)
    guard accepted, rewardReady else { return }
    awardLegacyRequestTrust(request)
  }

  /// Records an ordinary wildlife invitation. A greeting can add a small piece
  /// of recognition; every richer gain waits for a completed shared activity.
  mutating func recordInvitation(
    _ request: AnimalRequest, accepted: Bool, tick: Int, position: SIMD2<Float>
  ) {
    let rewardReady = appendEncounter(request, accepted: accepted, tick: tick)
    mountedExplorationActive = false
    activeActivity = nil
    if !accepted { awaySinceTick = nil }
    guard accepted else { return }
    if request == .greeting, rewardReady, trust < Self.familiarityCap {
      trust = min(
        Self.familiarityCap,
        trust + 0.025 * (request == preferences.favoriteRequest ? 1.25 : 1))
    }
    let kind: AnimalSharedActivityKind
    switch request {
    case .greeting, .wait, .come: kind = .company
    case .play: kind = .play
    case .follow: kind = .exploration
    }
    activeActivity = AnimalSharedActivityProgress(
      kind: kind, originatingRequest: request, startedTick: tick, position: position)
  }

  mutating func interruptSharedActivity() {
    mountedExplorationActive = false
    activeActivity = nil
  }

  /// The first synchronization establishes the mounted source position and
  /// earns no distance. Later validated segments can contribute only after at
  /// least one ordinary population tick has elapsed.
  mutating func recordMountedExplorationMovement(
    tick: Int, to: SIMD2<Float>
  ) {
    if !mountedExplorationActive {
      activeActivity = AnimalSharedActivityProgress(
        kind: .exploration, originatingRequest: .follow, startedTick: tick, position: to)
      mountedExplorationActive = true
      return
    }
    guard activeActivity?.kind == .exploration else { return }
    guard activeActivity!.activeTicks > 0 else {
      activeActivity!.rebase(at: to)
      return
    }
    advanceSharedActivity(tick: tick, position: to, qualifying: true)
  }

  mutating func advanceMountedExploration(tick: Int, position: SIMD2<Float>) {
    guard mountedExplorationActive, activeActivity?.kind == .exploration else { return }
    advanceSharedActivity(tick: tick, position: position, qualifying: true)
  }

  mutating func endMountedExploration() {
    guard mountedExplorationActive else { return }
    mountedExplorationActive = false
    activeActivity = nil
  }

  mutating func recordAssistance(
    outcomeID: String, kind: WildlifeAssistanceKind, tick: Int
  ) {
    assistanceMemories.append(
      WildlifeAssistanceMemory(outcomeID: outcomeID, kind: kind, completedTick: tick))
    if assistanceMemories.count > Self.assistanceMemoryLimit {
      assistanceMemories.removeFirst(assistanceMemories.count - Self.assistanceMemoryLimit)
    }
    trust = min(1, trust + 0.1 + preferences.sociability * 0.04)
  }

  /// Advances only qualifying in-world time. A broken activity is discarded;
  /// there is no offline catch-up and no relationship upkeep debt.
  mutating func advanceSharedActivity(
    tick: Int, position: SIMD2<Float>, qualifying: Bool
  ) {
    guard var progress = activeActivity else { return }
    guard qualifying, tick >= progress.startedTick + progress.activeTicks else {
      activeActivity = nil
      return
    }
    progress.advance(tick: tick, position: position)
    activeActivity = progress
    let complete: Bool
    switch progress.kind {
    case .company:
      complete = progress.activeTicks >= Self.companyCompletionTicks
    case .play:
      complete = progress.activeTicks >= Self.playCompletionTicks
        && progress.distanceTravelled >= Self.playCompletionDistance
    case .exploration:
      complete = progress.activeTicks >= Self.explorationCompletionTicks
        && progress.distanceTravelled >= Self.explorationCompletionDistance
    }
    guard complete else { return }
    sharedExperiences.append(
      AnimalSharedExperience(
        kind: progress.kind, originatingRequest: progress.originatingRequest,
        completedTick: tick, activeTicks: progress.activeTicks,
        distanceTravelled: progress.distanceTravelled))
    if sharedExperiences.count > Self.sharedExperienceLimit {
      sharedExperiences.removeFirst(sharedExperiences.count - Self.sharedExperienceLimit)
    }
    let gain: Float
    switch progress.kind {
    case .company: gain = 0.08 + preferences.sociability * 0.04
    case .play: gain = 0.12 + preferences.playfulness * 0.08
    case .exploration: gain = 0.16 + preferences.sociability * 0.06
    }
    let favored = progress.originatingRequest == preferences.favoriteRequest
    trust = min(1, trust + gain * (favored ? 1.25 : 1))
    activeActivity = nil
  }

  /// Tracks separation for an individual with completed company. A response is
  /// emitted only on a visible return after a sustained absence and cooldown.
  mutating func observePlayerReturn(
    tick: Int, distance: Float, visible: Bool, canRespond: Bool
  ) -> AnimalReturnResponse? {
    guard tick >= 0, tick <= 1_000_000_000, distance.isFinite, distance >= 0,
      sharedExperiences.contains(where: { $0.kind == .company })
    else {
      awaySinceTick = nil
      return nil
    }
    if distance >= Self.returnDepartureDistance {
      if awaySinceTick == nil { awaySinceTick = tick }
      return nil
    }
    guard visible, distance <= Self.returnRecognitionDistance else { return nil }
    guard let departedTick = awaySinceTick else { return nil }
    awaySinceTick = nil
    let refusalTick = encounters.filter { $0.outcome == .refused }.map(\.tick).max()
    let welcomedTick = sharedExperiences.filter { $0.kind == .company }.map(\.completedTick).max()!
    guard tick - departedTick >= Self.returnAbsenceTicks, canRespond,
      refusalTick.map({ $0 < welcomedTick }) ?? true,
      returnEvents.last.map({ tick - $0.returnedTick >= Self.returnResponseCooldownTicks }) ?? true
    else { return nil }
    let response: AnimalReturnResponse = prefersReturnApproach ? .approached : .acknowledged
    returnEvents.append(
      AnimalReturnEvent(departedTick: departedTick, returnedTick: tick, response: response))
    if returnEvents.count > Self.returnEventLimit {
      returnEvents.removeFirst(returnEvents.count - Self.returnEventLimit)
    }
    return response
  }

  private var prefersReturnApproach: Bool {
    var threshold: Float
    switch preferences.temperament {
    case .curious: threshold = 0.12
    case .proud: threshold = 0.3
    case .cautious: threshold = 0.42
    case .wild: threshold = 0.55
    }
    threshold -= (preferences.sociability - 0.5) * 0.15
    if sharedExperiences.last(where: { $0.kind == .company })?.originatingRequest
      == preferences.favoriteRequest
    {
      threshold -= 0.05
    }
    return trust >= max(Self.familiarityCap, threshold)
  }

  @discardableResult private mutating func appendEncounter(
    _ request: AnimalRequest, accepted: Bool, tick: Int
  ) -> Bool {
    let lastInteractionTick = encounters.map(\.tick).max()
    let rewardReady = lastInteractionTick.map {
      tick >= $0 && tick - $0 >= Self.trustRewardCooldownTicks
    } ?? true
    encounters.append(
      AnimalEncounter(tick: tick, request: request, outcome: accepted ? .accepted : .refused))
    if encounters.count > Self.memoryLimit {
      encounters.removeFirst(encounters.count - Self.memoryLimit)
    }
    return rewardReady
  }

  /// Retains the legacy expedition's established request pacing while the
  /// living wildlife path uses completed activities through recordInvitation.
  private mutating func awardLegacyRequestTrust(_ request: AnimalRequest) {
    let favored = request == preferences.favoriteRequest
    let gain: Float
    switch request {
    case .greeting: gain = 0.025
    case .wait: gain = 0.05
    case .come: gain = 0.07
    case .play: gain = 0.12
    case .follow: gain = 0.1
    }
    trust = min(1, trust + gain * (favored ? 1.25 : 1))
  }

  public func validate() throws {
    try preferences.validate()
    try activeActivity?.validate()
    for experience in sharedExperiences { try experience.validate() }
    for event in returnEvents { try event.validate() }
    for assistance in assistanceMemories { try assistance.validate() }
    let returnSpacingIsValid = zip(returnEvents, returnEvents.dropFirst()).allSatisfy({ pair in
      pair.1.returnedTick - pair.0.returnedTick >= Self.returnResponseCooldownTicks
    })
    guard !id.isEmpty, id.count <= 64, trust.isFinite, (0...1).contains(trust),
      encounters.count <= Self.memoryLimit,
      sharedExperiences.count <= Self.sharedExperienceLimit,
      returnEvents.count <= Self.returnEventLimit,
      assistanceMemories.count <= Self.assistanceMemoryLimit,
      !mountedExplorationActive || activeActivity?.kind == .exploration || activeActivity == nil,
      awaySinceTick.map({ (0...1_000_000_000).contains($0) }) ?? true,
      encounters.allSatisfy({ $0.tick >= 0 && $0.tick <= 1_000_000_000 }),
      returnSpacingIsValid
    else { throw ExpeditionError.invalidSave }
  }

  private enum CodingKeys: String, CodingKey {
    case id, preferences, trust, encounters, activeActivity, sharedExperiences, awaySinceTick,
      returnEvents, mountedExplorationActive, assistanceMemories
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    id = try container.decode(String.self, forKey: .id)
    preferences = try container.decode(AnimalPreferences.self, forKey: .preferences)
    trust = try container.decode(Float.self, forKey: .trust)
    encounters = try container.decode([AnimalEncounter].self, forKey: .encounters)
    activeActivity = try container.decodeIfPresent(
      AnimalSharedActivityProgress.self, forKey: .activeActivity)
    sharedExperiences = try container.decodeIfPresent(
      [AnimalSharedExperience].self, forKey: .sharedExperiences) ?? []
    awaySinceTick = try container.decodeIfPresent(Int.self, forKey: .awaySinceTick)
    returnEvents = try container.decodeIfPresent([AnimalReturnEvent].self, forKey: .returnEvents) ?? []
    mountedExplorationActive =
      try container.decodeIfPresent(Bool.self, forKey: .mountedExplorationActive) ?? false
    assistanceMemories = try container.decodeIfPresent(
      [WildlifeAssistanceMemory].self, forKey: .assistanceMemories) ?? []
    try validate()
  }
}
