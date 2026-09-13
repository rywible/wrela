import FieldCore
import Foundation
import SanctuaryContent
import SimulationCore
import TestKit
import simd

public enum SanctuaryTesting {
  public static func project(workspace: URL) -> TestProject {
    let initialPopulationCount = Double(WildlifePopulation.initial().actors.count)
    let riderHome = WildlifePopulation.initial().actor(id: "moonhart-001")!.position
    let flierHome = WildlifePopulation.initial().actor(id: "canopy-glider-001")!.position
    let lakeMoonhart = WildlifePopulation.initial().actor(id: "moonhart-002")!.position
    let oceanRay = WildlifePopulation.initial().actor(id: "cloud-ray-001")!.position
    let riderStart = riderHome + SIMD2<Float>(0, 4)
    let flierStart = flierHome + SIMD2<Float>(0, 4)
    let geography = SanctuaryGeography()
    let explorationLandmarks = Dictionary(
      uniqueKeysWithValues: SanctuaryGeography.landmarks.map { ("explore-" + $0.biome.rawValue, $0) })
    struct WalkingExplorationRoute {
      let start: SIMD2<Float>
      let target: SIMD2<Float>
    }
    // Landmark props are solid source features. These approaches end at nearby
    // clearings (or the dry tidepool shore), still inside the 28 m discovery radius.
    let walkingRoutes: [SanctuaryBiome: WalkingExplorationRoute] = [
      .woodland: .init(start: SIMD2(0, 20), target: SIMD2(0, 0)),
      .meadow: .init(start: SIMD2(2_450, -610), target: SIMD2(2_450, -630)),
      .creek: .init(start: SIMD2(1_680, 2_190), target: SIMD2(1_680, 2_170)),
      .wetland: .init(start: SIMD2(4_480, 3_940), target: SIMD2(4_480, 3_920)),
      .desert: .init(start: SIMD2(-8_535, 5_775), target: SIMD2(-8_555, 5_775)),
      .rainforest: .init(start: SIMD2(8_615, -2_975), target: SIMD2(8_595, -2_975)),
      .coast: .init(start: SIMD2(11_771, -7_175), target: SIMD2(11_751, -7_175)),
      .tidepool: .init(start: SIMD2(12_976, -9_500), target: SIMD2(12_976, -9_520)),
    ]
    func validateWalkingRoute(
      _ route: WalkingExplorationRoute, fixture: String, in world: SanctuaryWorld
    ) throws {
      // This queries the same streamed terrain, water policy, and regional solids that the
      // production move applies. TestStep.walk below remains the movement under test.
      world.move(.zero)
      let count = max(1, Int(ceil(length(route.target - route.start) / 0.5)))
      for index in 0...count {
        let point = route.start + (route.target - route.start) * Float(index) / Float(count)
        let ground = world.groundHeight(point.x, point.y)
        let water = world.localWaterHeight(point.x, point.y)
        guard (water ?? ground) <= ground + 0.5 else {
          let waterDescription = water.map { String(describing: $0) } ?? "none"
          throw SimulationFailure.invalid(
            "Exploration fixture \(fixture) reaches water at (\(point.x), \(point.y)): "
              + "ground \(ground), water \(waterDescription)")
        }
        for height: Float in [0.35, 1, 1.55] {
          let sample = V3(point.x, ground + height, point.y)
          guard !world.world.collisionGrid.candidates(at: sample).contains(where: {
            world.world.solids[$0].value(sample) < 0.28
          }) else {
            throw SimulationFailure.invalid("Exploration route must use a production clearing, not a landmark solid")
          }
        }
      }
    }
    func walkingExploration(_ biome: SanctuaryBiome) -> GameTest {
      let landmark = geography.landmark(for: biome)
      let route = walkingRoutes[biome]!
      return GameTest(
        "explore-" + biome.rawValue, fixture: "explore-" + biome.rawValue,
        tags: ["exploration", "render"]
      ) {
        // Fixture relocation establishes the local route start. These walks use
        // SanctuaryWorld movement, collision, terrain, and fixed ticks.
        TestStep.walk(x: route.target.x, z: route.target.y, maxTicks: 1200)
        TestStep.expect("nearestLandmarkID", landmark.id)
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.save("visited")
        TestStep.walk(x: route.start.x, z: route.start.y, maxTicks: 1200)
        TestStep.load("visited")
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.capture("explore-" + biome.rawValue)
      }
    }
    func companionExploration(
      _ biome: SanctuaryBiome, fixture: String, control: String, movement: V3,
      expectedMode: String
    ) -> GameTest {
      let landmark = geography.landmark(for: biome)
      return GameTest(
        "explore-" + biome.rawValue, fixture: fixture, tags: ["exploration", "render", "route"]
      ) {
        // This fixture declares only the selected companion familiar. The production control
        // below still validates the nearby animal, authored willingness, and capability.
        TestStep.action(.control(control))
        TestStep.expect("travelMode", expectedMode)
        TestStep.action(.move(movement))
        TestStep.advance(1)
        TestStep.expect("nearestLandmarkID", landmark.id)
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.save("visited")
        TestStep.action(.move(movement))
        TestStep.load("visited")
        TestStep.expect("travelMode", expectedMode)
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.capture("explore-" + biome.rawValue)
      }
    }
    func mountedExploration(_ biome: SanctuaryBiome, fixture: String, movement: V3) -> GameTest {
      let landmark = geography.landmark(for: biome)
      return GameTest(
        "explore-" + biome.rawValue, fixture: fixture, tags: ["exploration", "render", "route"]
      ) {
        // This named fixture starts in a production flying journey. It validates
        // only the bounded local flight and persistence, not a 10 km crossing or
        // an encounter from the shore.
        TestStep.expect("travelMode", "flying")
        TestStep.action(.move(movement))
        TestStep.advance(1)
        TestStep.expect("nearestLandmarkID", landmark.id)
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.save("visited")
        TestStep.action(.move(movement))
        TestStep.load("visited")
        TestStep.expect("travelMode", "flying")
        TestStep.expect("discoveredLandmarkIDs", landmark.id)
        TestStep.capture("explore-" + biome.rawValue)
      }
    }
    func legacyRescueTrustScenario() -> GameTest {
      GameTest(
        "rescue-trust", fixture: "frostling", tags: ["legacy", "smoke", "render"]
      ) {
        // Calm presence establishes at most familiarity. The low-trust request remains a real
        // visible legacy-brain refusal, without auto-following the wandering Frostling.
        TestStep.action(.request("follow"))
        TestStep.expect("behavior", "declining")
        TestStep.advance(720)
        TestStep.range("trust", 0, 0.1)
        TestStep.capture("wild")
      }
    }
    func legacyRescueReplayScenario() -> GameTest {
      GameTest("rescue-replay", fixture: "legacy-rescue", tags: ["legacy", "smoke", "render"]) {
        TestStep.action(.interact)
        TestStep.expect("phase", "carrying")
        TestStep.save("rescued")
        TestStep.advance(60)
        TestStep.load("rescued")
        TestStep.expect("creatureID", "frostling-001")
        TestStep.expect("phase", "carrying")
        TestStep.capture("wild")
      }
    }
    func legacyReturnTraversalScenario() -> GameTest {
      GameTest("return-traversal", fixture: "legacy-rescue", tags: ["legacy", "route", "render"]) {
        TestStep.action(.interact)
        TestStep.expect("phase", "carrying")
        TestStep.walk(x: -2, z: -68)
        TestStep.walk(x: 0, z: -55)
        TestStep.walk(x: -10, z: -32)
        TestStep.walk(x: 0, z: -2)
        TestStep.walk(x: 0, z: 21)
        TestStep.action(.interact)
        TestStep.expect("phase", "settled")
      }
    }
    return TestProject(
      id: "sanctuary", product: "Sanctuary",
      fixtures: [
        "arrival", "frostling", "legacy-rescue", "home", "living", "homestead", "rider", "flier",
        "explore-woodland", "explore-meadow", "explore-creek", "explore-wetland", "explore-lake",
        "explore-alpine", "explore-desert", "explore-rainforest", "explore-coast",
        "explore-tidepool", "explore-ocean",
      ] + SanctuaryStreamingScenarios.fixtures + SanctuaryConnectedJourneyScenarios.fixtures
        + SanctuaryPlayerExperienceJourneys.fixtures + SanctuaryBridgeCompanionJourney.fixtures,
      tests: [
        // Migration coverage for the retired rescue loop. New production scenarios use
        // the living-world request and control paths below.
        legacyRescueTrustScenario(),
        legacyRescueReplayScenario(),
        GameTest("running-frightens", fixture: "frostling", tags: ["legacy", "smoke", "behavior"]) {
          TestStep.advance(120, running: true)
          TestStep.range("fear", 0.9, 1)
          TestStep.range("trust", 0, 0.1)
          TestStep.action(.interact)
          TestStep.expect("phase", "searching")
        },
        GameTest("habitat-release", fixture: "home", tags: ["legacy", "smoke", "render"]) {
          TestStep.action(.interact)
          TestStep.expect("phase", "settled")
          TestStep.capture("new-home")
          TestStep.advance(1200)
          TestStep.range("habitat", 0.99, 1)
          TestStep.capture("established-home")
        },
        legacyReturnTraversalScenario(),
        GameTest("living-requests", fixture: "living", tags: ["smoke", "living", "behavior"]) {
          TestStep.advance(1)
          TestStep.expect("nearbyAnimalID", "sunhare-001")
          TestStep.action(.request("hello"))
          TestStep.expect("lastAnimalRequest", "greeting")
          TestStep.expect("lastAnimalResponse", "accepted")
          // A real low-trust request reaches the same authored animal brain and is refused.
          // This living fixture intentionally declares no companion relationship.
          TestStep.action(.request("follow"))
          TestStep.expect("lastAnimalRequest", "follow")
          TestStep.expect("lastAnimalResponse", "refused")
          TestStep.advance(1)
          TestStep.expect("nearbyAnimalID", "sunhare-001")
          TestStep.range("populationCount", initialPopulationCount, initialPopulationCount)
          TestStep.range("observedAnimals", 1, 128)
          TestStep.range("discoveredPlaces", 1, 11)
          TestStep.save("greeted-sunhare")
          TestStep.advance(120)
          TestStep.load("greeted-sunhare")
          TestStep.expect("nearbyAnimalID", "sunhare-001")
          TestStep.capture("sunhare-greeting")
        },
        GameTest("habitat-and-building", fixture: "homestead", tags: ["smoke", "living", "construction"]) {
          TestStep.action(.control("flowers"))
          TestStep.action(.control("grove"))
          TestStep.action(.control("reeds"))
          TestStep.action(.control("water"))
          TestStep.action(.control("raise"))
          TestStep.action(.control("lower"))
          TestStep.action(.control("smooth"))
          TestStep.range("gardenPatchCount", 4, 4)
          TestStep.range("terrainPatchCount", 3, 3)
          TestStep.action(.control("build-cabin"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-path"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-bridge"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-deck"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-bench"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-lantern"))
          TestStep.action(.move(V3(10, 0, 0)))
          TestStep.action(.control("build-fence"))
          TestStep.range("buildingCount", 7, 7)
          TestStep.save("shaped-homestead")
          TestStep.action(.control("undoBuilding"))
          TestStep.range("buildingCount", 6, 6)
          TestStep.action(.control("undoPlanting"))
          TestStep.range("terrainPatchCount", 2, 2)
          TestStep.load("shaped-homestead")
          TestStep.range("buildingCount", 7, 7)
          TestStep.range("gardenPatchCount", 4, 4)
          TestStep.range("terrainPatchCount", 3, 3)
          TestStep.capture("shaped-homestead")
        },
        GameTest("riding-movement", fixture: "rider", tags: ["living", "route", "behavior"]) {
          TestStep.action(.control("ride"))
          TestStep.expect("travelMode", "riding")
          TestStep.action(.move(V3(0, 0, 5)))
          TestStep.range("z", Double(riderStart.y - 15.5), Double(riderStart.y - 14.5))
          TestStep.action(.control("dismount"))
          TestStep.expect("travelMode", "walking")
          TestStep.capture("riding-route")
        },
        GameTest("flying-movement", fixture: "flier", tags: ["living", "route", "behavior"]) {
          // Observe the departure habitat through its ordinary fixed tick before
          // this bounded flight input travels beyond the discovery radius.
          TestStep.advance(1)
          TestStep.action(.control("fly"))
          TestStep.expect("travelMode", "flying")
          TestStep.action(.move(V3(0, 0, 5)))
          TestStep.range("z", Double(flierStart.y - 60.5), Double(flierStart.y - 59.5))
          TestStep.action(.control("dismount"))
          TestStep.expect("travelMode", "walking")
          TestStep.action(.control("journal"))
          TestStep.range("discoveredPlaces", 1, 11)
          TestStep.capture("flying-route")
        },
        walkingExploration(.woodland),
        walkingExploration(.meadow),
        walkingExploration(.creek),
        walkingExploration(.wetland),
        companionExploration(.lake, fixture: "explore-lake", control: "ride", movement: V3(7, 0, 0), expectedMode: "riding"),
        companionExploration(.alpine, fixture: "explore-alpine", control: "ride", movement: V3(0, 0, 7), expectedMode: "riding"),
        walkingExploration(.desert),
        walkingExploration(.rainforest),
        walkingExploration(.coast),
        walkingExploration(.tidepool),
        mountedExploration(.ocean, fixture: "explore-ocean", movement: V3(0, 0, 2)),
      ] + SanctuaryStreamingScenarios.tests + SanctuaryConnectedJourneyScenarios.tests
        + SanctuaryPlayerExperienceJourneys.tests + SanctuaryBridgeCompanionJourney.tests,
      workloads: [
        SimulationWorkload("living-population-60-ticks", fixture: "living"),
        SimulationWorkload("one-actor-60-ticks", fixture: "frostling"),
        SimulationWorkload("sixteen-actors-60-ticks", fixture: "frostling", actors: 16),
      ],
      create: { fixture, seed in
        let dir = workspace.appendingPathComponent("Games/Sanctuary/Authoring/Assets")
        let tree =
          try JSONSerialization.jsonObject(
            with: Data(contentsOf: dir.appendingPathComponent("tree.json"))) as? [String: Any]
        let parameters = (tree?["parameters"] as? [String: NSNumber] ?? [:]).mapValues(\.floatValue)
        let w = try SanctuaryWorld(seed: seed, parameters: parameters)
        let creature =
          try JSONSerialization.jsonObject(
            with: Data(contentsOf: dir.appendingPathComponent("frostling.json"))) as? [String: Any]
        if let motion = creature?["motion"] {
          w.motion = try JSONDecoder().decode(
            CreatureMotion.self, from: JSONSerialization.data(withJSONObject: motion))
        }
        if fixture != "arrival" {
          w.camera = PlayerCamera(
            position: V3(-3, w.world.terrain.height(-3, -80) + 1.72, -80), pitch: -0.2)
        }
        switch fixture {
        case let name where SanctuaryStreamingScenarios.fixtures.contains(name):
          _ = try SanctuaryStreamingScenarios.configure(fixture: name, in: w)
        case let name where SanctuaryConnectedJourneyScenarios.fixtures.contains(name):
          _ = try SanctuaryConnectedJourneyScenarios.configure(fixture: name, in: w)
        case let name where SanctuaryPlayerExperienceJourneys.fixtures.contains(name):
          _ = try SanctuaryPlayerExperienceJourneys.configure(fixture: name, in: w)
        case let name where SanctuaryBridgeCompanionJourney.fixtures.contains(name):
          _ = try SanctuaryBridgeCompanionJourney.configure(fixture: name, in: w)
        case "arrival", "frostling": break
        case "home":
          w.legacyInteractions = true
          try SanctuaryFixtureRelationships.declareLegacyFamiliar(in: w)
          _ = try w.interact()
          w.camera = PlayerCamera(
            position: V3(0, w.world.terrain.height(0, 21) + 1.72, 21), pitch: -0.3)
          w.syncExpeditionPlayer()
        case "legacy-rescue":
          w.legacyInteractions = true
          try SanctuaryFixtureRelationships.declareLegacyFamiliar(in: w)
        case "living":
          w.camera = PlayerCamera(
            position: V3(2, w.groundHeight(2, 23) + 1.72, 23), yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
        case "rider":
          w.camera = PlayerCamera(
            position: V3(
              riderStart.x, w.groundHeight(riderStart.x, riderStart.y) + 1.72, riderStart.y),
            yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
          try SanctuaryFixtureRelationships.declareFamiliar("moonhart-001", in: w)
        case "flier":
          w.camera = PlayerCamera(
            position: V3(
              flierStart.x, w.groundHeight(flierStart.x, flierStart.y) + 1.72, flierStart.y),
            yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
          try SanctuaryFixtureRelationships.declareFamiliar("canopy-glider-001", in: w)
        case "homestead":
          w.camera = PlayerCamera(
            position: V3(100, w.groundHeight(100, 100) + 1.72, 100), yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
        case "explore-lake":
          // moonhart-002 lives on Moonlake's east shore, 1,065 m from the landmark
          // center. Keep this fixture tied to that real production actor.
          let start = lakeMoonhart + SIMD2<Float>(-4, 0)
          w.camera = PlayerCamera(
            position: V3(start.x, w.groundHeight(start.x, start.y) + 1.72, start.y),
            yaw: .pi / 2, pitch: -0.08)
          w.syncExpeditionPlayer()
          try SanctuaryFixtureRelationships.declareFamiliar("moonhart-002", in: w)
        case "explore-alpine":
          let cloudstepper = WildlifePopulation.initial().actor(id: "cloudstepper-001")!.position
          // Stay on the outside of Cloudstep's stone spire while looking across it.
          let start = cloudstepper + SIMD2<Float>(-4, 2)
          w.camera = PlayerCamera(
            position: V3(start.x, w.groundHeight(start.x, start.y) + 1.72, start.y), yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
          try SanctuaryFixtureRelationships.declareFamiliar("cloudstepper-001", in: w)
        case "explore-ocean":
          let start = oceanRay + SIMD2<Float>(0, -4)
          // Fixture staging places the observer beside the water-supported Ray,
          // then runs its production relationship brain before starting mounted.
          // This unrecorded staging pose does not claim walking on the ocean or
          // earning this encounter through a shore route. Only the final mounted
          // fixture is checkpointed; flight/movement/save are the behavior under test.
          w.camera = PlayerCamera(
            position: V3(start.x, w.flightSurfaceHeight(start.x, start.y) + 1.72, start.y),
            yaw: .pi, pitch: -0.08)
          w.syncExpeditionPlayer()
          try SanctuaryFixtureRelationships.declareFamiliar("cloud-ray-001", in: w)
          try w.controller.editLiving { state in
            var population = state.population
            try population.updateCompanionPosition(
              id: "cloud-ray-001", position: start, using: .fly,
              expectedRevision: population.revision)
            var journey = state.travel
            try journey.travel(.flying, with: "cloud-ray-001")
            journey.flightHeight = 12
            state.wildlife = population
            state.journey = journey
            state.player = start
            state.playerElevation = w.flightSurfaceHeight(start.x, start.y) + journey.flightHeight
          }
          w.camera = PlayerCamera(
            position: V3(start.x, w.flightSurfaceHeight(start.x, start.y) + 12, start.y),
            yaw: .pi, pitch: -0.08)
          w.move(.zero)
          w.syncExpeditionPlayer()
        case let name where explorationLandmarks[name] != nil:
          let landmark = explorationLandmarks[name]!
          let route = walkingRoutes[landmark.biome]!
          w.camera = PlayerCamera(
            position: V3(
              route.start.x, w.groundHeight(route.start.x, route.start.y) + 1.72, route.start.y),
            yaw: 0, pitch: -0.08)
          w.syncExpeditionPlayer()
          try validateWalkingRoute(route, fixture: name, in: w)
        default: throw SimulationFailure.invalid("Unknown Sanctuary fixture \(fixture)")
        }
        if fixture == "frostling" { w.legacyInteractions = true }
        try w.validate()
        return w
      })
  }
}
