import Foundation
import simd

public enum SanctuaryVegetationResidencyError: Error, Equatable, Sendable {
  case invalidConfiguration
  case invalidCoordinate
  case invalidBudget
}

public enum SanctuaryVegetationResidencyTier: Int, CaseIterable, Codable, Sendable {
  case detailed = 0
  case community = 1
  case canopy = 2
}

public struct SanctuaryVegetationResidencyConfiguration: Equatable, Sendable {
  public let nearChunkRadius: Int
  public let middleChunkRadius: Int
  public let farChunkRadius: Int

  public init(
    nearChunkRadius: Int = 1, middleChunkRadius: Int = 2, farChunkRadius: Int = 4
  ) throws {
    guard nearChunkRadius >= 0, nearChunkRadius <= middleChunkRadius,
      middleChunkRadius <= farChunkRadius, farChunkRadius <= 4
    else { throw SanctuaryVegetationResidencyError.invalidConfiguration }
    self.nearChunkRadius = nearChunkRadius
    self.middleChunkRadius = middleChunkRadius
    self.farChunkRadius = farChunkRadius
  }

  public static let sanctuary = try! SanctuaryVegetationResidencyConfiguration()
  public var maximumResidentChunks: Int { square(farChunkRadius * 2 + 1) }
  public var maximumCachedChunks: Int { maximumResidentChunks * 2 }
  public var maximumDetailedRecords: Int {
    square(nearChunkRadius * 2 + 1) * SanctuaryBiomeVegetationPopulation.candidatesPerChunk
  }

  private func square(_ value: Int) -> Int { value * value }
}

public struct SanctuaryVegetationDesiredChunk: Equatable, Sendable {
  public let key: SanctuaryTerrainChunkKey
  public let tier: SanctuaryVegetationResidencyTier
}

public struct SanctuaryVegetationResidencyRequest: Equatable, Sendable {
  public let token: UInt64
  public let centerKey: SanctuaryTerrainChunkKey
  public let desiredChunks: [SanctuaryVegetationDesiredChunk]
  public let candidateCellCount: Int
  public let communityClimateSampleCount: Int
}

public struct SanctuaryVegetationWorkBudget: Equatable, Sendable {
  public let maximumChunks: Int
  public let maximumCandidateCells: Int

  public init(maximumChunks: Int, maximumCandidateCells: Int) throws {
    guard maximumChunks > 0, maximumCandidateCells > 0
    else { throw SanctuaryVegetationResidencyError.invalidBudget }
    self.maximumChunks = maximumChunks
    self.maximumCandidateCells = maximumCandidateCells
  }

  public static let oneChunk = try! SanctuaryVegetationWorkBudget(
    maximumChunks: 1,
    maximumCandidateCells: SanctuaryBiomeVegetationPopulation.candidatesPerChunk)
}

/// A scheduler-owned unit of pure CPU work. The token prevents a result from an obsolete player
/// position from entering the live residency cache.
public struct SanctuaryVegetationGenerationJob: Equatable, Sendable {
  public let requestToken: UInt64
  public let key: SanctuaryTerrainChunkKey
  public let requestedTier: SanctuaryVegetationResidencyTier
  public let candidateCellCount: Int
  public let communityClimateSampleCount: Int
}

/// One noncolliding 512 m canopy aggregate. `childIDs` is the complete identity mapping back to
/// the exact records; this representation never invents a second population.
public struct SanctuaryVegetationCanopySummary: Equatable, Sendable {
  public let id: String
  public let key: SanctuaryTerrainChunkKey
  public let bounds: SanctuaryMapBounds
  public let childCount: Int
  public let childIDs: [String]
  public let centroid: SIMD2<Float>
  public let maximumScale: Float
  public let speciesCounts: [SanctuaryVegetationSpecies: Int]
  public let canopyCoverageProxy: Float
  public let childDigest: UInt64

  public var providesCollision: Bool { false }
}

public struct SanctuaryVegetationCompiledChunk: Equatable, Sendable {
  public let job: SanctuaryVegetationGenerationJob
  public let exact: [SanctuaryVegetationRecord]
  public let parents: [SanctuaryVegetationParentSummary]
  public let canopy: SanctuaryVegetationCanopySummary

  fileprivate init(
    job: SanctuaryVegetationGenerationJob, exact: [SanctuaryVegetationRecord],
    parents: [SanctuaryVegetationParentSummary], canopy: SanctuaryVegetationCanopySummary
  ) {
    self.job = job
    self.exact = exact
    self.parents = parents
    self.canopy = canopy
  }
}

public struct SanctuaryVegetationResidentChunk: Equatable, Sendable {
  public let key: SanctuaryTerrainChunkKey
  public let tier: SanctuaryVegetationResidencyTier
}

/// One coherent scene source. Every resident canonical chunk appears exactly once, at one tier.
/// Only detailed records provide collision; community and canopy summaries retain source IDs.
public struct SanctuaryVegetationResidencySnapshot: Equatable, Sendable {
  public let revision: UInt64
  public let requestToken: UInt64
  public let centerKey: SanctuaryTerrainChunkKey?
  public let chunks: [SanctuaryVegetationResidentChunk]
  public let detailedRecords: [SanctuaryVegetationRecord]
  public let communityParents: [SanctuaryVegetationParentSummary]
  public let farCanopies: [SanctuaryVegetationCanopySummary]
  public let isComplete: Bool

  public static let empty = SanctuaryVegetationResidencySnapshot(
    revision: 0, requestToken: 0, centerKey: nil, chunks: [], detailedRecords: [],
    communityParents: [], farCanopies: [], isComplete: false)
}

public enum SanctuaryVegetationPublication: Equatable, Sendable {
  case stale
  case cached
  case published(SanctuaryVegetationResidencySnapshot)
}

public enum SanctuaryVegetationLocalExclusion: String, Codable, Equatable, Sendable {
  case authoredShallowWater
  case construction
}

/// Identity-preserving local edit decision consumed before the project turns an exact record into
/// visual and collision transforms. Terrain sculpting changes support height in the project
/// resolver but does not exclude or regenerate the source record.
public struct SanctuaryVegetationLocalEditDecision: Equatable, Sendable {
  public let sourceID: String
  public let exclusion: SanctuaryVegetationLocalExclusion?
  public let gardenRevision: UInt64?
  public let constructionRevision: UInt64?

  public var isIncluded: Bool { exclusion == nil }
}

/// Pure view of bounded persisted edits. Population IDs and habitat caches remain static while an
/// authored shallow-water patch or construction footprint can suppress its local presentation and
/// collision together.
public struct SanctuaryVegetationLocalEditMask: Sendable {
  public let garden: HabitatGarden?
  public let construction: PersonalConstruction?

  public init(garden: HabitatGarden? = nil, construction: PersonalConstruction? = nil) {
    self.garden = garden
    self.construction = construction
  }

  public func decision(
    for record: SanctuaryVegetationRecord
  ) -> SanctuaryVegetationLocalEditDecision {
    let location = HabitatGarden.Location(x: record.coordinate.x, z: record.coordinate.y)
    let water = garden?.conditions(at: location).hasShallowWater == true
    let occupied = construction?.placements.contains {
      Self.contains(record.coordinate, placement: $0)
    } == true
    return SanctuaryVegetationLocalEditDecision(
      sourceID: record.id,
      exclusion: water ? .authoredShallowWater : occupied ? .construction : nil,
      gardenRevision: garden?.revision, constructionRevision: construction?.revision)
  }

  private static func contains(
    _ point: SIMD2<Float>, placement: PersonalConstruction.Placement
  ) -> Bool {
    let dx = point.x - placement.location.x
    let dz = point.y - placement.location.z
    let c = cos(placement.yawRadians), s = sin(placement.yawRadians)
    let localX = dx * c - dz * s
    let localZ = dx * s + dz * c
    let extent = placement.primitive.halfExtents * placement.scale
    return abs(localX) <= extent.x && abs(localZ) <= extent.z
  }
}

/// Bounded content-side planner/cache. Generation is explicitly separate from publication so the
/// existing chunk scheduler can compile jobs off the render thread. Complete tiers publish in
/// near-to-far order; the previous valid snapshot remains visible while a tier is incomplete.
public struct SanctuaryBiomeVegetationResidency: Sendable {
  public let population: SanctuaryBiomeVegetationPopulation
  public let configuration: SanctuaryVegetationResidencyConfiguration
  public private(set) var snapshot: SanctuaryVegetationResidencySnapshot = .empty
  public private(set) var currentRequest: SanctuaryVegetationResidencyRequest?

  private var nextToken: UInt64 = 0
  private var snapshotRevision: UInt64 = 0
  private var desiredByKey: [SanctuaryTerrainChunkKey: SanctuaryVegetationResidencyTier] = [:]
  private var compiledByKey: [SanctuaryTerrainChunkKey: SanctuaryVegetationCompiledChunk] = [:]
  private var displayedByKey: [SanctuaryTerrainChunkKey: DisplayedChunk] = [:]
  private var scheduledKeys: Set<SanctuaryTerrainChunkKey> = []
  private var publishedThroughTier = -1

  public init(
    population: SanctuaryBiomeVegetationPopulation = .init(),
    configuration: SanctuaryVegetationResidencyConfiguration = .sanctuary
  ) {
    self.population = population
    self.configuration = configuration
  }

  public var cachedChunkCount: Int { compiledByKey.count }
  public var scheduledChunkCount: Int { scheduledKeys.count }

  /// Plans a new desired set without compiling or evicting displayed content. Cached overlapping
  /// chunks can re-tier immediately; missing chunks remain explicit scheduler jobs.
  @discardableResult
  public mutating func beginRequest(
    around coordinate: SIMD2<Float>
  ) throws -> SanctuaryVegetationResidencyRequest {
    guard coordinate.x.isFinite, coordinate.y.isFinite,
      SanctuaryGeography.bounds.contains(coordinate)
    else { throw SanctuaryVegetationResidencyError.invalidCoordinate }
    let center = SanctuaryTerrainChunkKey(containing: coordinate)
    if let currentRequest, currentRequest.centerKey == center { return currentRequest }

    nextToken &+= 1
    let desired = desiredChunks(around: center)
    desiredByKey = Dictionary(uniqueKeysWithValues: desired.map { ($0.key, $0.tier) })
    let request = SanctuaryVegetationResidencyRequest(
      token: nextToken, centerKey: center, desiredChunks: desired,
      candidateCellCount: desired.count * SanctuaryBiomeVegetationPopulation.candidatesPerChunk,
      communityClimateSampleCount:
        desired.count * SanctuaryBiomeVegetationPopulation.communityParentsPerChunk)
    currentRequest = request
    scheduledKeys.removeAll(keepingCapacity: true)
    publishedThroughTier = -1
    compiledByKey = compiledByKey.filter {
      desiredByKey[$0.key] != nil || displayedByKey[$0.key] != nil
    }
    _ = publishReadyTiers()
    precondition(compiledByKey.count <= configuration.maximumCachedChunks)
    return request
  }

  /// Marks a bounded nearest-first set of jobs as scheduled. If a job fails, call `abandon` so it
  /// becomes eligible again. A budget smaller than one 256-candidate chunk returns no work.
  public mutating func nextJobs(
    for requestToken: UInt64, budget: SanctuaryVegetationWorkBudget
  ) -> [SanctuaryVegetationGenerationJob] {
    guard let request = currentRequest, request.token == requestToken,
      budget.maximumCandidateCells >= SanctuaryBiomeVegetationPopulation.candidatesPerChunk
    else { return [] }
    let count = min(
      budget.maximumChunks,
      budget.maximumCandidateCells / SanctuaryBiomeVegetationPopulation.candidatesPerChunk)
    let missing = request.desiredChunks.filter {
      compiledByKey[$0.key] == nil && !scheduledKeys.contains($0.key)
    }.prefix(count)
    let jobs = missing.map {
      SanctuaryVegetationGenerationJob(
        requestToken: request.token, key: $0.key, requestedTier: $0.tier,
        candidateCellCount: SanctuaryBiomeVegetationPopulation.candidatesPerChunk,
        communityClimateSampleCount:
          SanctuaryBiomeVegetationPopulation.communityParentsPerChunk)
    }
    scheduledKeys.formUnion(jobs.map(\.key))
    return jobs
  }

  /// Pure deterministic work suitable for the existing off-thread chunk compiler.
  public func compile(
    _ job: SanctuaryVegetationGenerationJob
  ) throws -> SanctuaryVegetationCompiledChunk {
    let query = try population.query(in: Self.bounds(for: job.key))
    let canopy = Self.canopySummary(key: job.key, children: query.exact)
    return SanctuaryVegetationCompiledChunk(
      job: job, exact: query.exact, parents: query.parents, canopy: canopy)
  }

  /// Publishes only results for the current request. Complete tiers replace matching canonical
  /// chunks atomically; no exact child and summary for the same chunk coexist in a snapshot.
  public mutating func publish(
    _ result: SanctuaryVegetationCompiledChunk
  ) -> SanctuaryVegetationPublication {
    guard let request = currentRequest, result.job.requestToken == request.token,
      let desiredTier = desiredByKey[result.job.key],
      desiredTier == result.job.requestedTier
    else { return .stale }
    scheduledKeys.remove(result.job.key)
    compiledByKey[result.job.key] = result
    precondition(compiledByKey.count <= configuration.maximumCachedChunks)
    return publishReadyTiers()
  }

  public mutating func abandon(_ job: SanctuaryVegetationGenerationJob) {
    guard currentRequest?.token == job.requestToken else { return }
    scheduledKeys.remove(job.key)
  }

  /// Cancels only matching generation intent. The last published snapshot stays valid and any
  /// eventual result carrying the cancelled token is rejected without changing cache or revision.
  @discardableResult
  public mutating func cancel(requestToken: UInt64) -> Bool {
    guard currentRequest?.token == requestToken else { return false }
    currentRequest = nil
    desiredByKey.removeAll(keepingCapacity: true)
    scheduledKeys.removeAll(keepingCapacity: true)
    publishedThroughTier = -1
    compiledByKey = compiledByKey.filter { displayedByKey[$0.key] != nil }
    return true
  }

  private struct DisplayedChunk: Sendable {
    let tier: SanctuaryVegetationResidencyTier
    let compiled: SanctuaryVegetationCompiledChunk
  }

  private mutating func publishReadyTiers() -> SanctuaryVegetationPublication {
    guard let request = currentRequest else { return .cached }
    let previousTier = publishedThroughTier
    while publishedThroughTier < SanctuaryVegetationResidencyTier.canopy.rawValue {
      let next = publishedThroughTier + 1
      let tier = SanctuaryVegetationResidencyTier(rawValue: next)!
      let keys = request.desiredChunks.filter { $0.tier == tier }.map(\.key)
      guard keys.allSatisfy({ compiledByKey[$0] != nil }) else { break }
      for key in keys {
        displayedByKey[key] = DisplayedChunk(tier: tier, compiled: compiledByKey[key]!)
      }
      publishedThroughTier = next
    }
    guard publishedThroughTier != previousTier else { return .cached }

    trimObsoleteDisplayed(around: request.centerKey)
    let complete = publishedThroughTier == SanctuaryVegetationResidencyTier.canopy.rawValue
    if complete {
      displayedByKey = displayedByKey.filter { desiredByKey[$0.key] != nil }
      compiledByKey = compiledByKey.filter { desiredByKey[$0.key] != nil }
    }
    snapshotRevision &+= 1
    snapshot = makeSnapshot(request: request, complete: complete)
    return .published(snapshot)
  }

  /// As a new tier enters, retire the farthest obsolete chunks so displayed state never exceeds
  /// one full residency. Pending plus committed compiled caches therefore remain <= two sets.
  private mutating func trimObsoleteDisplayed(around center: SanctuaryTerrainChunkKey) {
    let excess = displayedByKey.count - configuration.maximumResidentChunks
    guard excess > 0 else { return }
    let removable = displayedByKey.keys.filter { desiredByKey[$0] == nil }.sorted {
      let ld = max(abs($0.x - center.x), abs($0.z - center.z))
      let rd = max(abs($1.x - center.x), abs($1.z - center.z))
      if ld != rd { return ld > rd }
      return $0.z == $1.z ? $0.x < $1.x : $0.z < $1.z
    }
    for key in removable.prefix(excess) {
      displayedByKey[key] = nil
      if desiredByKey[key] == nil { compiledByKey[key] = nil }
    }
  }

  private func makeSnapshot(
    request: SanctuaryVegetationResidencyRequest, complete: Bool
  ) -> SanctuaryVegetationResidencySnapshot {
    let entries = displayedByKey.sorted {
      $0.key.z == $1.key.z ? $0.key.x < $1.key.x : $0.key.z < $1.key.z
    }
    var chunks: [SanctuaryVegetationResidentChunk] = []
    var detailed: [SanctuaryVegetationRecord] = []
    var parents: [SanctuaryVegetationParentSummary] = []
    var canopies: [SanctuaryVegetationCanopySummary] = []
    for (key, displayed) in entries {
      chunks.append(SanctuaryVegetationResidentChunk(key: key, tier: displayed.tier))
      switch displayed.tier {
      case .detailed: detailed += displayed.compiled.exact
      case .community: parents += displayed.compiled.parents
      case .canopy: canopies.append(displayed.compiled.canopy)
      }
    }
    detailed.sort { $0.id < $1.id }
    parents.sort { $0.id < $1.id }
    canopies.sort { $0.id < $1.id }
    return SanctuaryVegetationResidencySnapshot(
      revision: snapshotRevision, requestToken: request.token, centerKey: request.centerKey,
      chunks: chunks, detailedRecords: detailed, communityParents: parents,
      farCanopies: canopies, isComplete: complete)
  }

  private func desiredChunks(
    around center: SanctuaryTerrainChunkKey
  ) -> [SanctuaryVegetationDesiredChunk] {
    var result: [SanctuaryVegetationDesiredChunk] = []
    let radius = configuration.farChunkRadius
    for dz in -radius...radius {
      for dx in -radius...radius {
        let key = SanctuaryTerrainChunkKey(x: center.x + dx, z: center.z + dz)
        guard key.intersectsWorld else { continue }
        let distance = max(abs(dx), abs(dz))
        let tier: SanctuaryVegetationResidencyTier
        if distance <= configuration.nearChunkRadius { tier = .detailed }
        else if distance <= configuration.middleChunkRadius { tier = .community }
        else { tier = .canopy }
        result.append(SanctuaryVegetationDesiredChunk(key: key, tier: tier))
      }
    }
    return result.sorted {
      if $0.tier.rawValue != $1.tier.rawValue { return $0.tier.rawValue < $1.tier.rawValue }
      let ld = max(abs($0.key.x - center.x), abs($0.key.z - center.z))
      let rd = max(abs($1.key.x - center.x), abs($1.key.z - center.z))
      if ld != rd { return ld < rd }
      return $0.key.z == $1.key.z ? $0.key.x < $1.key.x : $0.key.z < $1.key.z
    }
  }

  private static func bounds(for key: SanctuaryTerrainChunkKey) -> SanctuaryMapBounds {
    SanctuaryMapBounds(
      minimum: simd_max(key.minimum, SanctuaryGeography.bounds.minimum),
      maximum: simd_min(key.maximum, SanctuaryGeography.bounds.maximum))
  }

  private static func canopySummary(
    key: SanctuaryTerrainChunkKey, children: [SanctuaryVegetationRecord]
  ) -> SanctuaryVegetationCanopySummary {
    let sorted = children.sorted { $0.id < $1.id }
    let centroid = sorted.isEmpty
      ? key.center
      : sorted.reduce(SIMD2<Float>(repeating: 0)) { $0 + $1.coordinate } / Float(sorted.count)
    var speciesCounts: [SanctuaryVegetationSpecies: Int] = [:]
    var coverageArea: Float = 0
    var digest: UInt64 = 0x9E37_79B9_7F4A_7C15
    for child in sorted {
      speciesCounts[child.species, default: 0] += 1
      let baseRadius: Float
      switch child.species {
      case .broadleaf, .willow: baseRadius = 3.0
      case .palm: baseRadius = 2.4
      case .conifer: baseRadius = 2.1
      case .cactus: baseRadius = 0.7
      case .reeds: baseRadius = 0.9
      }
      coverageArea += .pi * baseRadius * baseRadius * child.scale * child.scale
      digest = textHash(child.id, seed: digest ^ child.scaleSeed)
    }
    return SanctuaryVegetationCanopySummary(
      id: "vegetation-canopy-v\(SanctuaryBiomeVegetationPopulation.sourceVersion):\(key.id)",
      key: key, bounds: bounds(for: key), childCount: sorted.count,
      childIDs: sorted.map(\.id), centroid: centroid,
      maximumScale: sorted.map(\.scale).max() ?? 0, speciesCounts: speciesCounts,
      canopyCoverageProxy: min(1, coverageArea / (SanctuaryTerrainChunkKey.size
        * SanctuaryTerrainChunkKey.size)),
      childDigest: digest)
  }

  private static func textHash(_ text: String, seed: UInt64) -> UInt64 {
    text.utf8.reduce(seed) { ($0 ^ UInt64($1)) &* 0x100_0000_01B3 }
  }
}
