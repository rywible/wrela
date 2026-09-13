import Foundation
import simd

public enum BoulderArrangementError: Error, Equatable, Sendable {
  case staleRevision
  case unknownBoulder
  case invalidPush
  case invalidHelper
  case displacementLimit
  case arrangementLimit
  case nothingToUndo
  case invalidState
}

public struct BoulderDisplacement: Codable, Equatable, Sendable {
  public let featureID: String
  public fileprivate(set) var offset: SIMD2<Float>
  public fileprivate(set) var totalTravel: Float
  public fileprivate(set) var pushCount: Int
  public fileprivate(set) var lastHelperID: String
}

public struct BoulderPushRecord: Codable, Equatable, Sendable {
  public let featureID: String
  public let delta: SIMD2<Float>
  public let helperID: String
}

/// Sparse, persisted overrides for deterministic source boulders.
public struct BoulderArrangements: Codable, Equatable, Sendable {
  public static let maximumBoulders = 128
  public static let maximumHistory = 512
  public static let maximumPushDistance: Float = 2
  public static let maximumTotalDisplacement: Float = 32
  public static let minimumPushDistance: Float = 0.05

  public private(set) var revision: UInt64
  public private(set) var displacements: [BoulderDisplacement]
  public private(set) var history: [BoulderPushRecord]

  public init() {
    revision = 0
    displacements = []
    history = []
  }

  public enum Command: Equatable, Sendable {
    case push(featureID: String, direction: SIMD2<Float>, distance: Float, helperID: String)
    case undo
  }

  public func displacement(for featureID: String) -> BoulderDisplacement? {
    displacements.first { $0.featureID == featureID }
  }

  public func resolvedCoordinate(for feature: SanctuaryChunkFeature) -> SIMD2<Float> {
    feature.coordinate + (displacement(for: feature.id)?.offset ?? .zero)
  }

  public var sourceChunkKeys: Set<SanctuaryTerrainChunkKey> {
    Set(displacements.compactMap { SanctuaryGeography.chunkKey(forFeatureID: $0.featureID) })
  }

  @discardableResult
  public mutating func apply(
    _ command: Command, expectedRevision: UInt64,
    geography: SanctuaryGeography = SanctuaryGeography()
  ) throws -> String {
    guard expectedRevision == revision else { throw BoulderArrangementError.staleRevision }
    var candidate = self
    let affectedID: String
    switch command {
    case let .push(featureID, direction, distance, helperID):
      guard let feature = geography.feature(id: featureID), feature.kind == .boulder
      else { throw BoulderArrangementError.unknownBoulder }
      guard direction.x.isFinite, direction.y.isFinite, length_squared(direction) > 0.000001,
        distance.isFinite,
        (Self.minimumPushDistance...Self.maximumPushDistance).contains(distance)
      else { throw BoulderArrangementError.invalidPush }
      let trimmedHelper = helperID.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmedHelper.isEmpty, trimmedHelper.utf8.count <= 64
      else { throw BoulderArrangementError.invalidHelper }
      guard candidate.history.count < Self.maximumHistory else {
        throw BoulderArrangementError.arrangementLimit
      }
      let delta = normalize(direction) * distance
      if let index = candidate.displacements.firstIndex(where: { $0.featureID == featureID }) {
        let newOffset = candidate.displacements[index].offset + delta
        let newTravel = candidate.displacements[index].totalTravel + distance
        guard length(newOffset) <= Self.maximumTotalDisplacement, newTravel <= Self.maximumTotalDisplacement
        else { throw BoulderArrangementError.displacementLimit }
        guard SanctuaryGeography.bounds.contains(feature.coordinate + newOffset)
        else { throw BoulderArrangementError.displacementLimit }
        candidate.displacements[index].offset = newOffset
        candidate.displacements[index].totalTravel = newTravel
        candidate.displacements[index].pushCount += 1
        candidate.displacements[index].lastHelperID = trimmedHelper
      } else {
        guard candidate.displacements.count < Self.maximumBoulders
        else { throw BoulderArrangementError.arrangementLimit }
        guard SanctuaryGeography.bounds.contains(feature.coordinate + delta)
        else { throw BoulderArrangementError.displacementLimit }
        candidate.displacements.append(
          BoulderDisplacement(
            featureID: featureID, offset: delta, totalTravel: distance, pushCount: 1,
            lastHelperID: trimmedHelper))
      }
      candidate.history.append(
        BoulderPushRecord(featureID: featureID, delta: delta, helperID: trimmedHelper))
      affectedID = featureID
    case .undo:
      guard let record = candidate.history.popLast(),
        let index = candidate.displacements.firstIndex(where: { $0.featureID == record.featureID })
      else { throw BoulderArrangementError.nothingToUndo }
      affectedID = record.featureID
      candidate.displacements[index].offset -= record.delta
      candidate.displacements[index].totalTravel -= length(record.delta)
      candidate.displacements[index].pushCount -= 1
      if candidate.displacements[index].pushCount == 0 {
        candidate.displacements.remove(at: index)
      } else {
        candidate.displacements[index].lastHelperID = candidate.history.last {
          $0.featureID == record.featureID
        }!.helperID
      }
    }
    guard candidate.revision < .max else { throw BoulderArrangementError.invalidState }
    candidate.revision += 1
    candidate.displacements.sort { $0.featureID < $1.featureID }
    try candidate.validate(geography: geography)
    self = candidate
    return affectedID
  }

  public func validate(geography: SanctuaryGeography = SanctuaryGeography()) throws {
    guard revision < .max, displacements.count <= Self.maximumBoulders,
      history.count <= Self.maximumHistory,
      Set(displacements.map(\.featureID)).count == displacements.count,
      displacements.map(\.featureID) == displacements.map(\.featureID).sorted()
    else { throw BoulderArrangementError.invalidState }
    for displacement in displacements {
      guard let feature = geography.feature(id: displacement.featureID), feature.kind == .boulder,
        displacement.offset.x.isFinite, displacement.offset.y.isFinite,
        length(displacement.offset) <= Self.maximumTotalDisplacement,
        displacement.totalTravel.isFinite,
        displacement.totalTravel + 0.001 >= length(displacement.offset),
        displacement.totalTravel <= Self.maximumTotalDisplacement,
        displacement.pushCount > 0,
        displacement.lastHelperID.utf8.count <= 64,
        !displacement.lastHelperID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        SanctuaryGeography.bounds.contains(feature.coordinate + displacement.offset)
      else { throw BoulderArrangementError.invalidState }
      let records = history.filter { $0.featureID == displacement.featureID }
      guard records.count == displacement.pushCount,
        length(records.reduce(SIMD2<Float>.zero) { $0 + $1.delta } - displacement.offset) < 0.001,
        abs(records.reduce(0) { $0 + length($1.delta) } - displacement.totalTravel) < 0.001,
        records.last?.helperID == displacement.lastHelperID
      else { throw BoulderArrangementError.invalidState }
    }
    guard history.allSatisfy({ record in
      record.delta.x.isFinite && record.delta.y.isFinite
        && (Self.minimumPushDistance...Self.maximumPushDistance).contains(length(record.delta))
        && !record.helperID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        && record.helperID.utf8.count <= 64
        && displacements.contains { $0.featureID == record.featureID }
    }) else { throw BoulderArrangementError.invalidState }
  }

  private enum CodingKeys: String, CodingKey { case revision, displacements, history }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    revision = try container.decode(UInt64.self, forKey: .revision)
    displacements = try container.decode([BoulderDisplacement].self, forKey: .displacements)
    history = try container.decode([BoulderPushRecord].self, forKey: .history)
    try validate()
  }
}
