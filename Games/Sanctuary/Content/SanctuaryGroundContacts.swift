import FieldCore
import Foundation
import simd

/// A bounded, saved record of collision-resolved ground movement. The renderer
/// derives grass and soil response from these facts; it never creates contacts.
public struct SanctuaryGroundContacts: Codable, Equatable, Sendable {
  public static let maximumEvents = SurfaceInfluenceSnapshot.maximumEvents
  public static let playerSourceID = "sanctuary-player"
  public static let playerStrideMetres: Float = 0.65

  public private(set) var events: [GroundInfluenceEvent]
  public private(set) var nextID: UInt64
  public private(set) var revision: UInt64
  public private(set) var playerStrideProgress: Float
  public private(set) var nextPlayerFootIsLeft: Bool

  public init(
    events: [GroundInfluenceEvent] = [], nextID: UInt64 = 1, revision: UInt64 = 0,
    playerStrideProgress: Float = 0.325,
    nextPlayerFootIsLeft: Bool = true
  ) {
    self.events = events
    self.nextID = nextID
    self.revision = revision
    self.playerStrideProgress = playerStrideProgress
    self.nextPlayerFootIsLeft = nextPlayerFootIsLeft
  }

  public func snapshot(at playSeconds: Double) -> SurfaceInfluenceSnapshot {
    SurfaceInfluenceSnapshot(time: playSeconds, events: events)
  }

  /// Records an already accepted movement path. Neighboring substeps coalesce,
  /// but never across a teleport or farther than a short local sweep.
  public mutating func record(
    sourceID: String, kind: GroundInfluenceKind, start: SIMD3<Float>, end: SIMD3<Float>,
    radius: Float, displacement: Float, compression: Float, supportTolerance: Float = 0.45,
    at playSeconds: Double, recoverySeconds: Float
  ) {
    let horizontal = SIMD2<Float>(end.x - start.x, end.z - start.z)
    guard kind == .body, length_squared(horizontal) > 0.00000001,
      nextID < .max, revision < .max else { return }
    let heading = normalize(horizontal)
    let newEvent = GroundInfluenceEvent(
      id: nextID, sourceID: sourceID, kind: kind, start: start, end: end,
      radius: radius, displacement: displacement, compression: compression,
      supportTolerance: supportTolerance, heading: heading,
      startTime: playSeconds, endTime: playSeconds, recoverySeconds: recoverySeconds)
    // Reject invalid input before pruning or otherwise touching persisted history.
    guard (try? newEvent.validate()) != nil else { return }
    prune(at: playSeconds)

    if let index = events.indices.last,
      events[index].sourceID == sourceID,
      events[index].kind == kind,
      distance(events[index].end, start) <= 0.05,
      distance(events[index].start, end) <= 2,
      events[index].endTime <= playSeconds,
      playSeconds - events[index].endTime <= 0.25
    {
      let previous = events[index]
      let candidate = GroundInfluenceEvent(
        id: previous.id, sourceID: sourceID, kind: kind, start: previous.start, end: end,
        radius: radius, displacement: displacement, compression: compression,
        supportTolerance: supportTolerance, heading: heading,
        startTime: previous.startTime, endTime: playSeconds, recoverySeconds: recoverySeconds)
      guard (try? candidate.validate()) != nil else { return }
      events[index] = candidate
      revision += 1
      return
    }

    nextID += 1
    events.append(newEvent)
    if events.count > Self.maximumEvents {
      events.removeFirst(events.count - Self.maximumEvents)
    }
    revision += 1
  }

  /// Emits immutable planted contacts from accepted path distance. Each event ID
  /// forever identifies one foot location, allowing render caches to key by ID.
  public mutating func recordPlayerFootsteps(
    along start: SIMD3<Float>, _ end: SIMD3<Float>, at playSeconds: Double,
    supportHeight: (SIMD2<Float>, Float) -> Float
  ) {
    let horizontal = SIMD2<Float>(end.x - start.x, end.z - start.z)
    var remaining = length(horizontal)
    guard remaining > 0.0001, remaining.isFinite else { return }
    let heading = horizontal / remaining
    var cursor = SIMD2<Float>(start.x, start.z)
    var cursorY = start.y
    let endXZ = SIMD2<Float>(end.x, end.z)

    while playerStrideProgress + remaining >= Self.playerStrideMetres {
      let needed = Self.playerStrideMetres - playerStrideProgress
      let fraction = needed / remaining
      let center = cursor + (endXZ - cursor) * fraction
      let centerY = cursorY + (end.y - cursorY) * fraction
      let side = SIMD2<Float>(-heading.y, heading.x) * (nextPlayerFootIsLeft ? 0.09 : -0.09)
      let unboundedFootprint = center + side
      let footprint = SIMD2<Float>(
        min(SanctuaryGeography.bounds.maximum.x,
          max(SanctuaryGeography.bounds.minimum.x, unboundedFootprint.x)),
        min(SanctuaryGeography.bounds.maximum.y,
          max(SanctuaryGeography.bounds.minimum.y, unboundedFootprint.y)))
      let y = supportHeight(footprint, centerY)
      let event = GroundInfluenceEvent(
        id: nextID, sourceID: Self.playerSourceID, kind: .foot,
        start: SIMD3(footprint.x, y, footprint.y), end: SIMD3(footprint.x, y, footprint.y),
        radius: 0.12, displacement: 0.025, compression: 0.82,
        supportTolerance: 0.22, heading: heading,
        startTime: playSeconds, endTime: playSeconds, recoverySeconds: 45)
      guard appendImmutable(event) else { return }
      nextPlayerFootIsLeft.toggle()
      playerStrideProgress = 0
      remaining -= needed
      cursor = center
      cursorY = centerY
      if remaining <= 0.0001 { return }
    }
    playerStrideProgress += remaining
  }

  private mutating func appendImmutable(_ event: GroundInfluenceEvent) -> Bool {
    guard event.id == nextID, nextID < .max, revision < .max,
      (try? event.validate()) != nil else { return false }
    prune(at: event.endTime)
    nextID += 1
    events.append(event)
    if events.count > Self.maximumEvents {
      events.removeFirst(events.count - Self.maximumEvents)
    }
    revision += 1
    return true
  }

  public mutating func prune(at playSeconds: Double) {
    guard playSeconds.isFinite, playSeconds >= 0 else { return }
    let before = events.count
    events.removeAll { $0.response(at: playSeconds) == 0 && playSeconds >= $0.endTime }
    if events.count != before, revision < .max { revision += 1 }
  }

  public func validate(at playSeconds: Double, validAnimalIDs: Set<String>) throws {
    try snapshot(at: playSeconds).validate()
    guard nextID > (events.map(\.id).max() ?? 0), nextID > 0, revision < .max,
      playerStrideProgress.isFinite,
      (0..<Self.playerStrideMetres).contains(playerStrideProgress),
      events.allSatisfy({ event in
        let start = SIMD2<Float>(event.start.x, event.start.z)
        let end = SIMD2<Float>(event.end.x, event.end.z)
        return (event.sourceID == Self.playerSourceID || validAnimalIDs.contains(event.sourceID))
          && SanctuaryGeography.bounds.contains(start) && SanctuaryGeography.bounds.contains(end)
          && abs(event.start.y) <= 1_000 && abs(event.end.y) <= 1_000
          && (event.kind != .foot || event.start == event.end)
      })
    else { throw ExpeditionError.invalidSave }
  }
}

extension SanctuaryWorld {
  public var surfaceInfluenceSnapshot: SurfaceInfluenceSnapshot {
    controller.state.recentGroundContacts.snapshot(at: controller.state.age)
  }

  func recordPlayerGroundSweeps(
    _ sweeps: [(SIMD3<Float>, SIMD3<Float>)], mode: SanctuaryJourney.TravelMode
  ) {
    guard !sweeps.isEmpty, mode != .flying else { return }
    controller.simulateLiving { state in
      let original = state.groundContacts ?? SanctuaryGroundContacts()
      var contacts = original
      let sourceID = mode == .riding
        ? (state.travel.companionID ?? SanctuaryGroundContacts.playerSourceID)
        : SanctuaryGroundContacts.playerSourceID
      if mode == .riding {
        guard let companionID = state.travel.companionID,
          let actor = state.population.actor(id: companionID),
          let end = sweeps.last?.1,
          distance(actor.position, SIMD2(end.x, end.z)) <= 0.05
        else { return }
      }
      let kind: GroundInfluenceKind = mode == .riding ? .body : .foot
      let radius: Float = mode == .riding ? 0.38 : 0.13
      let displacement: Float = mode == .riding ? 0.055 : 0.028
      let compression: Float = mode == .riding ? 0.9 : 0.72
      for sweep in sweeps {
        contacts.record(
          sourceID: sourceID, kind: .body, start: sweep.0, end: sweep.1,
          radius: radius, displacement: displacement, compression: compression,
          supportTolerance: 0.28, at: state.age,
          recoverySeconds: mode == .riding ? 18 : 2.5)
      }
      if kind == .foot {
        let garden = state.garden ?? HabitatGarden()
        let walkables = state.buildings.collisionFacts + SanctuaryHome.construction.collisionFacts
        for sweep in sweeps {
          contacts.recordPlayerFootsteps(along: sweep.0, sweep.1, at: state.age) {
            point, centerSupport in
            var support = garden.surfaceHeight(
              baseHeight: self.world.terrain.height(point.x, point.y),
              at: .init(x: point.x, z: point.y))
            for fact in walkables where fact.kind == .walkable
              && fact.contains(.init(x: point.x, y: fact.top, z: point.y))
              && fact.top <= centerSupport + 0.5 {
              support = max(support, fact.top)
            }
            return support
          }
        }
      }
      if contacts != original { state.groundContacts = contacts }
    }
  }

  func recordWildlifeGroundMovement(
    from before: WildlifePopulation, to after: WildlifePopulation, elapsedSeconds: Float
  ) {
    let origins = Dictionary(uniqueKeysWithValues: before.actors.map { ($0.id, $0) })
    let player = SIMD2<Float>(position.x, position.z)
    let garden = controller.state.garden ?? HabitatGarden()
    controller.simulateLiving { state in
      let original = state.groundContacts ?? SanctuaryGroundContacts()
      var contacts = original
      let contactTime = state.age + Double(elapsedSeconds)
      for actor in after.actors.sorted(by: { $0.id < $1.id }) {
        guard actor.species.sanctuaryHoverBaseline == 0,
          actor.id != state.travel.companionID,
          let origin = origins[actor.id], origin.position != actor.position,
          min(distance(origin.position, player), distance(actor.position, player)) <= 24
        else { continue }
        let startY = garden.surfaceHeight(
          baseHeight: world.terrain.height(origin.position.x, origin.position.y),
          at: .init(x: origin.position.x, z: origin.position.y))
        let endY = garden.surfaceHeight(
          baseHeight: world.terrain.height(actor.position.x, actor.position.y),
          at: .init(x: actor.position.x, z: actor.position.y))
        contacts.record(
          sourceID: actor.id, kind: .body,
          start: SIMD3(origin.position.x, startY, origin.position.y),
          end: SIMD3(actor.position.x, endY, actor.position.y),
          radius: min(0.5, max(0.12, 0.22 * actor.morphologyScale)),
          displacement: 0.035, compression: 0.62, supportTolerance: 0.35,
          at: contactTime, recoverySeconds: 24)
      }
      if contacts != original { state.groundContacts = contacts }
    }
  }
}
