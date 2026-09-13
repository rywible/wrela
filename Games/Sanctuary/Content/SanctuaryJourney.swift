import Foundation
import simd

/// Facts earned during play, with no offline clock and no compulsory objective list.
public struct SanctuaryJourney: Codable, Equatable, Sendable {
  public enum TravelMode: String, Codable, Sendable { case walking, riding, flying }
  public private(set) var mode: TravelMode = .walking
  public private(set) var companionID: String?
  public private(set) var discoveredLandmarks: [String] = []
  public private(set) var observedAnimals: [String] = []
  public private(set) var readClues: [String] = []
  public private(set) var revision: UInt64 = 0
  public var flightHeight: Float = 12

  public init() {}

  public mutating func travel(_ mode: TravelMode, with companionID: String?) throws {
    guard mode == .walking ? companionID == nil : (companionID?.isEmpty == false),
      (companionID?.count ?? 0) <= 64, revision < UInt64.max - 1
    else { throw ExpeditionError.invalidSave }
    self.mode = mode
    self.companionID = companionID
    revision += 1
  }

  public mutating func discover(at position: SIMD2<Float>) {
    let geography = SanctuaryGeography()
    for site in SanctuaryGeography.landmarks
    where geography.isAtLandmark(site, position: position)
      && !discoveredLandmarks.contains(site.id) {
      discoveredLandmarks.append(site.id)
      discoveredLandmarks.sort()
      revision += 1
    }
  }

  public mutating func observe(animalID: String) {
    guard !animalID.isEmpty, animalID.count <= 64, !observedAnimals.contains(animalID),
      observedAnimals.count < 128 else { return }
    observedAnimals.append(animalID)
    observedAnimals.sort()
    revision += 1
  }

  @discardableResult public mutating func examineClue(at position: SIMD2<Float>) -> String? {
    let geography = SanctuaryGeography()
    guard let site = SanctuaryGeography.landmarks.filter({
      geography.isAtLandmark($0, position: position, centerRadius: 35)
    }).min(by: {
      simd_distance($0.coordinate, position) < simd_distance($1.coordinate, position)
    }) else { return nil }
    if !readClues.contains(site.id) { readClues.append(site.id); readClues.sort(); revision += 1 }
    return Self.clue(for: site.biome)
  }

  public static func clue(for biome: SanctuaryBiome) -> String {
    switch biome {
    case .woodland: return "Round prints cross the soft earth. A pale tuft rests against the bark."
    case .meadow: return "Grass bends in a wide resting hollow. Paired hoofprints lead through the flowers."
    case .creek: return "Freshly gnawed willow lies by the water. Small wet prints return to the bank."
    case .wetland: return "Reeds are woven into a sheltered nest. Long narrow tracks follow the shallows."
    case .lake: return "A smooth slide runs down the bank. Rings spread across the quiet water."
    case .alpine: return "A broad feather catches on stone. A great shadow sometimes crosses this ridge."
    case .desert: return "Tiny tracks loop between shaded stones. Something has tucked seed husks beneath a ledge."
    case .rainforest: return "Fruit skins collect beneath a branch. Leaves sway where the air is still."
    case .coast: return "Shells lie in a careful pile above the spray. Broad prints lead toward the bluff."
    case .tidepool: return "A small shell has shifted since the last ripple. Delicate marks circle the pool."
    case .ocean: return "A wide wake curves through the blue. A distant back rises, then slips beneath the light."
    }
  }

  public func validate() throws {
    let landmarks = Set(SanctuaryGeography.landmarks.map(\.id))
    guard revision <= 10_000_000_000, flightHeight.isFinite, (3...60).contains(flightHeight),
      Set(discoveredLandmarks).count == discoveredLandmarks.count,
      Set(readClues).count == readClues.count,
      discoveredLandmarks.allSatisfy(landmarks.contains), readClues.allSatisfy(landmarks.contains),
      observedAnimals.count <= 128, Set(observedAnimals).count == observedAnimals.count,
      observedAnimals.allSatisfy({ !$0.isEmpty && $0.count <= 64 }),
      mode == .walking ? companionID == nil : (companionID?.isEmpty == false)
    else { throw ExpeditionError.invalidSave }
  }
}
