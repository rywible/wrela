import Foundation
import SanctuaryContent
import SimulationCore

/// Declared initial relationships for traversal fixtures. These are fixture facts, never a
/// substitute for an authored request, visibility, control, collision, or movement path.
public enum SanctuaryFixtureRelationships {
  /// Makes one already-authored companion familiar while retaining its identity, authored
  /// preferences, and interaction memory. Capability eligibility still comes from that
  /// actor's production species/profile and preference data.
  @discardableResult public static func declareFamiliar(
    _ id: String, in world: SanctuaryWorld
  ) throws -> WildlifeActor {
    let original = try requiredActor(id, in: world.controller.state.population)
    guard original.relationship.preferences.companionWilling else {
      throw SimulationFailure.invalid("Fixture companion \(id) is not authored as willing")
    }

    var document = try JSONSerialization.jsonObject(
      with: JSONEncoder().encode(world.controller.state.population)) as? [String: Any] ?? [:]
    guard var actors = document["actors"] as? [[String: Any]],
      let index = actors.firstIndex(where: { $0["id"] as? String == id }),
      var relationship = actors[index]["relationship"] as? [String: Any],
      relationship["id"] as? String == id
    else {
      throw SimulationFailure.invalid("Fixture relationship could not find \(id)")
    }

    // Keep every encoded relationship fact except the explicitly declared familiarity.
    relationship["trust"] = 1.0
    actors[index]["relationship"] = relationship
    document["actors"] = actors
    let population = try JSONDecoder().decode(
      WildlifePopulation.self,
      from: JSONSerialization.data(withJSONObject: document))
    let familiar = try requiredActor(id, in: population)
    guard familiar.relationship.id == original.relationship.id,
      familiar.relationship.preferences == original.relationship.preferences,
      familiar.relationship.encounters == original.relationship.encounters,
      familiar.relationship.trust == 1
    else {
      throw SimulationFailure.invalid("Fixture relationship changed authored facts for \(id)")
    }
    try world.controller.editLiving { state in state.wildlife = population }
    return try requiredActor(id, in: world.controller.state.population)
  }

  /// Declares the retained Frostling rescue relationship for legacy migration fixtures. It keeps
  /// its existing identity, preferences, and encounter memory; carrying and return remain the
  /// ordinary legacy interaction and walking paths exercised by the scenarios.
  public static func declareLegacyFamiliar(in world: SanctuaryWorld) throws {
    world.syncExpeditionPlayer()
    let original = world.controller.state.relationship
    var memory = try JSONSerialization.jsonObject(
      with: world.controller.checkpoint()) as? [String: Any] ?? [:]
    guard var state = memory["state"] as? [String: Any],
      var relationship = state["relationship"] as? [String: Any],
      relationship["id"] as? String == original.id
    else {
      throw SimulationFailure.invalid("Legacy fixture relationship is unavailable")
    }
    relationship["trust"] = 1.0
    state["relationship"] = relationship
    // `trust` is the legacy compatibility mirror encoded beside `relationship`.
    state["trust"] = 1.0
    memory["state"] = state
    try world.controller.restore(
      JSONSerialization.data(withJSONObject: memory, options: [.sortedKeys]))

    let familiar = world.controller.state.relationship
    guard familiar.id == original.id,
      familiar.preferences == original.preferences,
      familiar.encounters == original.encounters,
      familiar.trust == 1
    else {
      throw SimulationFailure.invalid("Legacy fixture relationship changed authored facts")
    }
  }

  private static func requiredActor(
    _ id: String, in population: WildlifePopulation
  ) throws -> WildlifeActor {
    guard let actor = population.actor(id: id) else {
      throw SimulationFailure.invalid("Fixture requires authored animal \(id)")
    }
    return actor
  }
}
