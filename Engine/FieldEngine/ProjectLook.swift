import Foundation

extension SceneLook {
  package static var url: URL {
    ProjectContext.current.authoring.appendingPathComponent("ArtDirection.json")
  }
  package static func load() throws -> Self {
    let data = try Data(contentsOf: url)
    guard data.count < 262144,
      let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      let look = object["renderLook"]
    else { throw RuntimeError.message("Art direction requires renderLook") }
    let result = try JSONDecoder().decode(
      Self.self, from: JSONSerialization.data(withJSONObject: look))
    try result.validate()
    return result
  }
  package func publish() throws {
    try validate()
    let data = try Data(contentsOf: Self.url)
    guard var object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
      throw RuntimeError.message("Invalid art-direction document")
    }
    object["renderLook"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(self))
    try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
      .write(to: Self.url, options: .atomic)
  }
}
