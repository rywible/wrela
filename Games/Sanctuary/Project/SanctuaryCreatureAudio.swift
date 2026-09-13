import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// Presentation of real nearby individuals and their saved encounters. No speech,
/// simulated needs, or new world facts are inferred from a sound.
final class SanctuaryCreatureAudio {
  private var lastAge: Double?
  private var encounters: [String: AnimalEncounter] = [:]

  func cues(state: Expedition, camera: PlayerCamera,
    elevation: (WildlifeActor) -> SanctuaryCreatureElevation) -> [AudioCue] {
    let age = state.age
    let previous = lastAge
    lastAge = age
    let actors = state.population.actors
    let priorEncounters = encounters
    encounters = Dictionary(uniqueKeysWithValues: actors.compactMap { actor in
      actor.relationship.encounters.last.map { (actor.id, $0) }
    })
    // Loading/replaying a checkpoint never bursts all old calls into the room.
    guard let previous, age >= previous, age - previous < 2 else { return [] }
    let listener = SIMD2(camera.position.x, camera.position.z)
    var result: [AudioCue] = []
    for actor in actors.sorted(by: { distance($0.position, listener) < distance($1.position, listener) }) {
      let range = distance(actor.position, listener)
      guard range < 45 else { continue }
      let encounter = actor.relationship.encounters.last
      let response = encounter != nil && encounter != priorEncounters[actor.id]
      let offset = actor.id.utf8.reduce(UInt32(17)) { ($0 &* 31) &+ UInt32($1) } % 13
      let ambient = age > previous && Int((age + Double(offset)) / 17) > Int((previous + Double(offset)) / 17)
      guard response || ambient else { continue }
      let accepted = encounter?.outcome == .accepted
      let base = Self.frequency(actor.species) * (actor.lifeStage == .young ? 1.2 : 1)
      let end = base * (response ? (accepted ? 1.24 : 0.72) : 1.08)
      let tone = ProceduralTone(frequency: base, endFrequency: end,
        duration: base < 350 ? 0.7 : 0.32, overtone: base < 350 ? 0.28 : 0.12)
      result.append(AudioCue(kind: "tonal",
        position: V3(actor.position.x, elevation(actor).focusY, actor.position.y),
        gain: response ? 0.18 : 0.10, tone: tone))
      if result.count == 2 { break }
    }
    return result
  }

  private static func frequency(_ species: WildlifeSpecies) -> Float {
    switch species {
    case .frostling: return 880
    case .sunhare: return 740
    case .brookweaver: return 520
    case .reedwalker: return 620
    case .moonhart: return 280
    case .cloudstepper: return 190
    case .dunefox: return 680
    case .canopyGlider: return 1040
    case .saltback: return 160
    case .tidepooler: return 1180
    case .cloudRay: return 130
    }
  }
}
