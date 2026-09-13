import Foundation

/// Identity of the immutable terrain source, independent of save schema and creature seeds.
/// The legacy source is the only executable recipe until a generated-world compiler exists.
public struct SanctuaryTerrainRecipe: Codable, Equatable, Sendable {
  public let generatorID: String
  public let version: Int

  public static let legacy = Self(generatorID: "sanctuary-legacy-v3", version: 1)
  public var identity: String { "\(generatorID)@\(version)" }

  public init(generatorID: String, version: Int) {
    self.generatorID = generatorID
    self.version = version
  }

  /// Unsupported recipes use the store's existing forward-version refusal path: never
  /// silently substitute the legacy terrain or recover a different world from its backup.
  public func validate() throws {
    guard self == Self.legacy else { throw ExpeditionError.unsupportedVersion }
  }
}
