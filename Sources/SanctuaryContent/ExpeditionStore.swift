import Foundation

/// Atomic primary save plus a last-valid backup. Generated meshes are never save data.
public struct ExpeditionStore {
  public let url: URL
  public init(url: URL) { self.url = url }
  public var backupURL: URL { url.deletingPathExtension().appendingPathExtension("previous.json") }
  private struct Document: Codable {
    var version = 1
    var creatureID = Expedition.creatureID
    var expedition: Expedition
  }
  private func decode(_ url: URL) throws -> Expedition {
    let data = try Data(contentsOf: url)
    guard data.count < 1_000_000 else { throw ExpeditionError.invalidSave }
    let version = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    guard version?["version"] as? Int == 1 else { throw ExpeditionError.unsupportedVersion }
    let document = try JSONDecoder().decode(Document.self, from: data)
    guard document.creatureID == Expedition.creatureID else { throw ExpeditionError.invalidSave }
    try document.expedition.validate()
    return document.expedition
  }
  public func load() throws -> (state: Expedition, recovered: Bool) {
    let exists = FileManager.default.fileExists(atPath: url.path)
    if !exists && !FileManager.default.fileExists(atPath: backupURL.path) {
      return (Expedition(), false)
    }
    do { return (try decode(url), false) } catch ExpeditionError.unsupportedVersion {
      throw ExpeditionError.unsupportedVersion
    } catch {
      if let saved = try? decode(backupURL) { return (saved, true) }
      throw ExpeditionError.unreadableSave
    }
  }
  public func save(_ state: Expedition) throws {
    try state.validate()
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    if FileManager.default.fileExists(atPath: url.path) {
      // Do not replace a newer schema or destroy the last recoverable backup.
      var validPrimary = false
      do {
        _ = try decode(url)
        validPrimary = true
      } catch ExpeditionError.unsupportedVersion { throw ExpeditionError.unsupportedVersion } catch
      { /* Preserve the recoverable backup when the primary is damaged. */  }
      if validPrimary { try Data(contentsOf: url).write(to: backupURL, options: .atomic) }
    }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(Document(expedition: state)).write(to: url, options: .atomic)
  }
}
