import Foundation
import XCTest

@testable import SanctuaryContent

/// Persistence fixture for tests whose subject is travel or world editing, rather
/// than the relationship pacing itself. It changes only the persisted trust fact;
/// baked preferences remain the production-authored values.
enum SanctuaryRelationshipFixture {
  static func establish(_ id: String, in world: SanctuaryWorld) throws {
    try world.controller.editLiving { state in
      state.wildlife = try established(state.population, id: id)
    }
  }

  static func established(_ population: WildlifePopulation, id: String) throws -> WildlifePopulation {
    let source = try XCTUnwrap(population.actor(id: id))
    guard var document = try JSONSerialization.jsonObject(with: JSONEncoder().encode(population))
      as? [String: Any]
    else { throw ExpeditionError.invalidSave }
    guard var actors = document["actors"] as? [[String: Any]],
      let index = actors.firstIndex(where: { $0["id"] as? String == id }),
      var relationship = actors[index]["relationship"] as? [String: Any]
    else { throw ExpeditionError.invalidSave }

    relationship["trust"] = 1.0
    actors[index]["relationship"] = relationship
    document["actors"] = actors
    let data = try JSONSerialization.data(withJSONObject: document)
    let established = try JSONDecoder().decode(WildlifePopulation.self, from: data)
    guard established.actor(id: id)?.relationship.preferences == source.relationship.preferences
    else { throw ExpeditionError.invalidSave }
    return established
  }
}
