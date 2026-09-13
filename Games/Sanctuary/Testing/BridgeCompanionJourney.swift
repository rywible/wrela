import FieldCore
import SanctuaryContent
import SimulationCore
import TestKit
import simd

/// Native-replayable acceptance for one animal using the same saved bridge,
/// request adapter, population brain, collision, relationship, and store as play.
public enum SanctuaryBridgeCompanionJourney {
  public static let fixture = "experience-bridge-animal-company"

  public static var fixtures: [String] { [fixture] }

  private static let bridgeCenter = SIMD2<Float>(0.014510762, 21.128466)
  private static let animalStart = bridgeCenter - SIMD2<Float>(1.3, 0)
  private static let visitorStart = bridgeCenter + SIMD2<Float>(1.5, 0)

  public static var tests: [GameTest] {
    [
      GameTest(
        "player-bridge-animal-company", fixture: fixture,
        tags: ["experience", "living", "behavior", "construction", "persistence", "render"]
      ) {
        TestStep.advance(1)
        TestStep.expect("nearbyAnimalID", "frostling-001")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.action(.request("come here"))
        TestStep.expect("lastAnimalRequest", "come")
        TestStep.expect("lastAnimalResponse", "accepted")
        TestStep.expect("nearbyAnimalActivity", "company")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.capture("bridge-animal-invited")

        // The production brain covers the bridge centerline during this half.
        // An invitation and partial activity remain absent from relationship memory.
        TestStep.advance(300)
        TestStep.expect("nearbyAnimalID", "frostling-001")
        TestStep.expect("nearbyAnimalActivity", "company")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.save("bridge-company-midpoint")
        TestStep.capture("bridge-animal-crossed-midpoint")

        // Reopen at the exact midpoint, then supply the remaining real ticks.
        TestStep.reopen("bridge-company-midpoint")
        TestStep.expect("nearbyAnimalID", "frostling-001")
        TestStep.expect("nearbyAnimalActivity", "company")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.advance(300)
        TestStep.expect("nearbyAnimalID", "frostling-001")
        TestStep.expect("nearbyAnimalActivity", "")
        TestStep.expect("nearbyAnimalExperiences", "1")
        TestStep.save("bridge-company-completed")
        TestStep.capture("bridge-animal-company-completed")

        // Select the independent low-trust Sunhare through ordinary walking and
        // visibility. Leave the bridge along its centerline and round the east end;
        // a direct northward walk from the saved midpoint crosses the side rail.
        // Its refusal is persisted, but it earns no shared experience.
        TestStep.walk(x: 3.5, z: 21.128466, maxTicks: 600)
        TestStep.walk(x: 3.5, z: 14, maxTicks: 900)
        TestStep.walk(x: 2, z: 14, maxTicks: 900)
        // During the ordinary bridge exit ticks the Sunhare has moved east from
        // its authored home; face its actual deterministic simulation position.
        TestStep.action(.look(yaw: Float.pi / 2, pitch: -0.08))
        TestStep.expect("nearbyAnimalID", "sunhare-001")
        TestStep.action(.request("follow me"))
        TestStep.expect("lastAnimalRequest", "follow")
        TestStep.expect("lastAnimalResponse", "refused")
        TestStep.expect("nearbyAnimalActivity", "")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.save("bridge-independent-refusal")
        TestStep.reopen("bridge-independent-refusal")
        TestStep.expect("nearbyAnimalID", "sunhare-001")
        TestStep.expect("lastAnimalRequest", "follow")
        TestStep.expect("lastAnimalResponse", "refused")
        TestStep.expect("nearbyAnimalExperiences", "0")
        TestStep.capture("bridge-independent-refusal")
      },
    ]
  }

  /// Reuses the committed Cabin water/reeds/bridge fixture, then changes only
  /// Frostling's opening simulation location and declared familiarity through
  /// the production legacy-to-population migration path. All later behavior is
  /// driven by recorded world inputs.
  @discardableResult public static func configure(
    fixture name: String, in world: SanctuaryWorld
  ) throws -> Bool {
    guard name == fixture else { return false }
    guard try SanctuaryPlayerExperienceJourneys.configure(
      fixture: SanctuaryPlayerExperienceJourneys.cabinWaterBridgeFixture, in: world)
    else { throw SimulationFailure.invalid("Cabin water bridge fixture is unavailable") }

    let familiar = try SanctuaryFixtureRelationships.declareFamiliar(
      "frostling-001", in: world)
    let population = try WildlifePopulation.migrating(
      // `Expedition.population` derives the authored roster from this stable
      // production default when an older save has no explicit wildlife field.
      seed: 17,
      relationship: familiar.relationship,
      creature: CreatureSimulation(home: animalStart, seed: familiar.simulation.seed))
    try world.controller.editLiving { state in state.wildlife = population }

    let walkableSupport = world.controller.state.buildings.collisionFacts
      .filter { fact in
        fact.kind == .walkable
          && fact.contains(.init(x: visitorStart.x, y: fact.top, z: visitorStart.y))
      }
      .map(\.top).max()
    guard let support = walkableSupport else {
      throw SimulationFailure.invalid("Bridge companion fixture has no walkable visitor support")
    }
    world.camera = PlayerCamera(
      position: V3(visitorStart.x, support + 1.72, visitorStart.y),
      yaw: -Float.pi / 2, pitch: -0.08)
    world.syncExpeditionPlayer()
    try world.validate()
    return true
  }
}
