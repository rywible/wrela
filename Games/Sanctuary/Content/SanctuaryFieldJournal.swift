import Foundation

extension WildlifeSpecies {
  public var displayName: String {
    switch self {
    case .canopyGlider: return "Canopy Glider"
    case .cloudRay: return "Cloud Ray"
    default: return rawValue.prefix(1).uppercased() + rawValue.dropFirst()
    }
  }
}

extension SanctuaryWorld {
  /// Entries disclose known identities and places. Changing habitat activity is described
  /// only while the individual is visible, so the journal is not a remote wildlife tracker.
  public var fieldJournalText: String {
    let state = controller.state
    let journey = state.travel
    let places = SanctuaryGeography.landmarks.filter { journey.discoveredLandmarks.contains($0.id) }
    let animals = state.population.actors.filter { journey.observedAnimals.contains($0.id) }
    var entries = ["Places visited"]
    entries += places.isEmpty ? ["Watch the paths, shorelines and distant silhouettes."] : places.map(\.name)
    entries += ["", "Wildlife observed"]
    entries += animals.isEmpty ? ["Pause near wildlife to begin an observation."] : animals.map { actor in
      let place = SanctuaryGeography.landmarks.first { $0.id == actor.landmarkID }?.name ?? "the wilds"
      let rarity = state.population.discoveryCharacterization(for: actor.id)?.rarity == .rare
        ? " · a rare individual" : ""
      return "\(actor.species.displayName) · \(place)\(rarity)"
    }
    for fact in state.population.rareHabitatFacts(
      observedAnimalIDs: journey.observedAnimals, garden: state.garden ?? HabitatGarden()) {
      guard let actor = state.population.actor(id: fact.characterization.animalID), animalVisible(actor) else { continue }
      var activity: String?
      switch fact.state {
      case .observed: break
      case .approachingPreferredHabitat: activity = "Exploring the habitat you shaped."
      case .settledAtPreferredHabitat: activity = "Settled in the habitat you shaped."
      case .returningHome: activity = "Finding a way back to familiar ground."
      }
      if let structure = fact.structure {
        switch structure.state {
        case .building: activity = "Gathering material for a resting or working place."
        case .active: activity = "Its habitat work is established here."
        case .displaced: activity = "The changed ground has interrupted its habitat work."
        case .rebuilding: activity = "Rebuilding its place after the ground changed again."
        }
      }
      if let activity { entries += ["", "\(actor.species.displayName) · \(activity)"] }
    }
    return entries.joined(separator: "\n")
  }
}
