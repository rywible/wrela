import Foundation

/// Persistent player choices for expressive editing; there are no material costs.
public struct SanctuaryToolSettings: Codable, Equatable, Sendable {
  public var brushRadius: Float = 3
  public var strength: Float = 1
  public var reach: Float = 5
  public var buildingScale: Float = 1
  public var buildingRotation: Float = 0
  public init() {}
  public func validate() throws {
    guard brushRadius.isFinite, (1...16).contains(brushRadius),
      strength.isFinite, (0.25...4).contains(strength),
      reach.isFinite, (3...16).contains(reach),
      buildingScale.isFinite, (0.5...3).contains(buildingScale),
      buildingRotation.isFinite, (-Float.pi...Float.pi).contains(buildingRotation)
    else { throw ExpeditionError.invalidSave }
  }
}
