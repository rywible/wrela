import FieldCore
import SanctuaryContent
import SimulationCore
import TestKit
import simd

/// Player-facing acceptance journeys. Fixtures declare their opening pose, familiar companions,
/// or previously observed committed edit geometry. Every interaction, later edit, discovery,
/// and movement after that named setup comes from the ordinary Sanctuary simulation.
public enum SanctuaryPlayerExperienceJourneys {
  public static let establishedFixture = "experience-established-transitions"
  public static let cabinWaterBridgeFixture = "experience-cabin-water-bridge"

  public static var fixtures: [String] { [establishedFixture, cabinWaterBridgeFixture] }

  public static var tests: [GameTest] {
    let population = WildlifePopulation.initial()
    let moonhart = population.actor(id: "moonhart-001")!.position
    let canopyGlider = population.actor(id: "canopy-glider-001")!.position
    let saltback = population.actor(id: "saltback-001")!.position
    let moonhartGreeting = moonhart + SIMD2<Float>(0, 4)
    let gliderGreeting = canopyGlider + SIMD2<Float>(0, 4)
    let saltbackGreeting = saltback + SIMD2<Float>(0, 4)
    // The short ridden coast approach moves the attached Saltback south to the nature site.
    // The greeting point remains a dry-bank approach: looking east from there places the bench
    // five metres away and more than fourteen metres from the invited-water center.
    let coastRest = saltbackGreeting + SIMD2<Float>(0, -9)
    let coastBenchApproach = saltbackGreeting
    // This is the dry shelf already used by the Tidepool exploration fixture, rather than the
    // underwater landmark center. The Coast → Tidepools route is ordinary walking both ways.
    let tidepoolShore = SIMD2<Float>(12_976, -9_500)

    return [
      GameTest(
        "player-opening-company-play", fixture: "arrival",
        tags: ["experience", "living", "behavior", "render"]
      ) {
        // `arrival` leaves SanctuaryWorld's authored fresh-save camera at (0, 24), with no
        // relationship, edit, mount, or discovery staged by this test.
        TestStep.advance(1)
        TestStep.expect("nearbyAnimalID", "sunhare-001")
        TestStep.expect("discoveredLandmarkIDs", "cabin-glade")
        TestStep.action(.request("play"))
        TestStep.expect("lastAnimalRequest", "play")
        TestStep.expect("lastAnimalResponse", "refused")

        // Calm presence reaches only the authored familiarity cap. It does not manufacture a
        // companion relationship; greeting then starts the actual shared company invitation.
        TestStep.advance(720)
        TestStep.action(.request("hello"))
        TestStep.expect("lastAnimalRequest", "greeting")
        TestStep.expect("lastAnimalResponse", "accepted")
        TestStep.expect("nearbyAnimalActivity", "company")
        TestStep.advance(600)
        TestStep.expect("nearbyAnimalExperiences", "1")

        // Sunhare's completed company visit makes the later play invitation eligible. A real
        // movement input and fixed ticks complete the authored play activity.
        TestStep.action(.request("play"))
        TestStep.expect("lastAnimalResponse", "accepted")
        TestStep.expect("nearbyAnimalActivity", "play")
        TestStep.action(.move(V3(0, 0, 1)))
        TestStep.advance(180)
        TestStep.expect("nearbyAnimalExperiences", "2")
        TestStep.save("welcomed-play")
        TestStep.capture("opening-welcomed-play")

        // Departure and return are ordinary walking, not a fixture relocation. The returning
        // greeting checks that the observed individual recognizes this saved relationship.
        // Cabin Glade's authored cabin occupies x ±2.8 m around z 26. The default pose sits
        // in its south-door threshold, so first leave through that front opening (−Z), then
        // take the maintained x = 4 m side clearing instead of crossing a side or rear wall.
        TestStep.walk(x: 0, z: 20, maxTicks: 600)
        TestStep.walk(x: 4, z: 20, maxTicks: 600)
        TestStep.walk(x: 4, z: 48, maxTicks: 1_200)
        TestStep.walk(x: 0, z: 48, maxTicks: 600)
        // Remain beyond the production 24 m departure radius long enough to cross the
        // 600-population-tick absence gate. The walks themselves contributed 443 ticks in
        // the sealed 22:57 trace; this pause makes the later approach a real return event.
        TestStep.advance(240)
        TestStep.walk(x: 4, z: 48, maxTicks: 600)
        TestStep.walk(x: 4, z: 20, maxTicks: 1_200)
        TestStep.walk(x: 0, z: 20, maxTicks: 600)
        TestStep.walk(x: 0, z: 24, maxTicks: 600)
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.expect("nearbyAnimalID", "sunhare-001")
        TestStep.expect("animalReturnResponses", "1")
        TestStep.expect("lastAnimalReturnResponse", "approached")
        TestStep.action(.request("hello"))
        TestStep.expect("lastAnimalResponse", "accepted")
        TestStep.expect("nearbyAnimalExperiences", "2")
        TestStep.save("returned-opening")
        TestStep.advance(120)
        TestStep.load("returned-opening")
        TestStep.expect("nearbyAnimalID", "sunhare-001")
        TestStep.expect("nearbyAnimalExperiences", "2")
        TestStep.capture("opening-returned-recognition")
      },
      GameTest(
        "player-established-adjacent-transitions", fixture: establishedFixture,
        tags: ["experience", "living", "route", "exploration", "nature", "construction", "render"]
      ) {
        // The fixture starts at Cabin Glade. The walk to the real Moonhart is under test.
        TestStep.advance(1)
        TestStep.expect("nearestLandmarkID", "cabin-glade")
        TestStep.walk(x: moonhartGreeting.x, z: moonhartGreeting.y, maxTicks: 3_600)
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.expect("nearbyAnimalID", "moonhart-001")
        TestStep.action(.control("ride"))
        TestStep.expect("travelMode", "riding")

        // These are the authored Cabin → Meadow and Meadow → Rainforest corridors.
        TestStep.walk(x: 2_450, z: -630, maxTicks: 12_000)
        TestStep.expect("nearestLandmarkID", "golden-meadow")
        TestStep.walk(x: gliderGreeting.x, z: gliderGreeting.y, maxTicks: 30_000)
        TestStep.expect("nearestLandmarkID", "verdant-canopy")
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.action(.control("dismount"))
        TestStep.expect("travelMode", "walking")
        TestStep.expect("nearbyAnimalID", "canopy-glider-001")
        TestStep.action(.control("fly"))
        TestStep.expect("travelMode", "flying")

        // Flight crosses the authored Rainforest → Coast adjacency to the real Saltback.
        TestStep.walk(x: saltbackGreeting.x, z: saltbackGreeting.y, maxTicks: 10_000)
        TestStep.expect("nearestLandmarkID", "saltwind-bluffs")
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.action(.control("dismount"))
        TestStep.expect("travelMode", "walking")
        TestStep.expect("nearbyAnimalID", "saltback-001")
        TestStep.action(.control("ride"))
        TestStep.expect("travelMode", "riding")
        TestStep.action(.move(V3(0, 0, 3)))
        TestStep.expect("travelMode", "riding")
        TestStep.action(.control("dismount"))
        TestStep.expect("travelMode", "walking")

        // Grow reeds through invited shallow water at the real reached coast site. Both edits
        // use the ordinary camera brush target, revisions, compatibility rules, and save path.
        TestStep.action(.control("water"))
        TestStep.action(.control("reeds"))
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)

        // Walk back onto the nearby dry bank before building. The eastward five-metre target is
        // outside the invited-water footprint and away from Saltback's home/resting bed. The CPU
        // scenario uses immediate construction controls; native preview is covered by its host path.
        TestStep.walk(x: coastBenchApproach.x, z: coastBenchApproach.y, maxTicks: 900)
        TestStep.action(.look(yaw: Float.pi / 2, pitch: -0.08))
        TestStep.action(.control("build-bench"))
        TestStep.range("buildingCount", 1, 1)
        TestStep.action(.control("building-select"))
        TestStep.action(.control("building-turn"))
        TestStep.action(.control("building-larger"))
        TestStep.action(.control("building-resize"))
        TestStep.action(.control("building-finish"))
        TestStep.save("coast-edited-bench")
        TestStep.capture("established-coast-edit")

        // Leave the saved construction through the Coast → Tidepools walking corridor. A benign
        // tool edit at the destination forces the production controller to persist the exact
        // reached pose, nature, construction, relationships, and simulation progress. `reopen`
        // then creates a fresh CPU world; native replay closes and launches a fresh host process
        // on the same named isolated slot before continuing the real return.
        TestStep.walk(x: tidepoolShore.x, z: tidepoolShore.y, maxTicks: 36_000)
        TestStep.expect("nearestLandmarkID", "lantern-tidepools")
        TestStep.range("buildingCount", 1, 1)
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.action(.control("brush-larger"))
        TestStep.save("tidepool-reopen")
        TestStep.reopen("tidepool-reopen")
        TestStep.expect("nearestLandmarkID", "lantern-tidepools")
        TestStep.range("buildingCount", 1, 1)
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.capture("established-tidepool-reopened")

        TestStep.walk(x: coastRest.x, z: coastRest.y, maxTicks: 36_000)
        TestStep.expect("nearestLandmarkID", "saltwind-bluffs")
        // In the frozen 00:17 route Saltback settles about 5.3 m south of this reached pose,
        // inside the actual observation range. Keep that relationship return independent from
        // the dry-bank construction check.
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.expect("nearbyAnimalID", "saltback-001")
        TestStep.action(.request("hello"))
        TestStep.expect("lastAnimalResponse", "accepted")
        // Reacquire the bench from its original approach and direction. Successful selection is
        // the ordinary reach/location assertion for the saved dry-bank placement.
        TestStep.walk(x: coastBenchApproach.x, z: coastBenchApproach.y, maxTicks: 900)
        TestStep.action(.look(yaw: Float.pi / 2, pitch: -0.08))
        TestStep.action(.control("building-select"))
        TestStep.range("buildingCount", 1, 1)
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.capture("established-coast-returned")
      },
      GameTest(
        "player-cabin-water-bridge-replay", fixture: cabinWaterBridgeFixture,
        tags: ["experience", "nature", "construction", "persistence", "render"]
      ) {
        // The fixture contains only the committed water, reeds, and bridge already produced by
        // the native Cabin controls. Editing begins here through ordinary production controls.
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.range("buildingCount", 1, 1)
        TestStep.action(.control("building-select"))

        // Shorten the ordinary placement reach so the selected bridge moves less than a metre
        // within the radius-four water patch. Undo must restore the exact prior placement state;
        // every frame snapshot records the transform and construction history for native replay.
        TestStep.action(.control("reach-nearer"))
        TestStep.action(.control("reach-nearer"))
        TestStep.action(.control("building-move"))
        TestStep.capture("cabin-water-bridge-moved")
        TestStep.action(.control("undoBuilding"))
        TestStep.action(.control("building-finish"))
        TestStep.save("cabin-water-bridge-restored")
        TestStep.reopen("cabin-water-bridge-restored")

        // A fresh process must load both garden contributions and the restored bridge from its
        // named isolated slot. Re-selecting from the original player pose checks actual reach and
        // location after disk-canonical verification, rather than treating the count as enough.
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.range("buildingCount", 1, 1)
        TestStep.action(.control("building-select"))
        TestStep.capture("cabin-water-bridge-reopened")
      },
      GameTest(
        "player-cabin-bridge-shore-chord", fixture: cabinWaterBridgeFixture,
        tags: ["experience", "nature", "construction", "movement", "persistence", "render"]
      ) {
        // Turn the existing bridge across the pool's eastern chord. The player movement stages
        // the exact three-metre production target; fixture setup does not relocate this edit.
        TestStep.action(.control("building-select"))
        TestStep.action(.control("building-turn"))
        TestStep.action(.control("building-turn"))
        TestStep.action(.control("reach-nearer"))
        TestStep.action(.control("reach-nearer"))
        TestStep.action(.move(V3(0.5, 0, 2.156549)))
        TestStep.action(.look(yaw: Float.pi / 2, pitch: -0.08))
        TestStep.action(.control("building-move"))
        TestStep.action(.control("building-finish"))

        // Approach around the north edge, then cross and return along the bridge centerline.
        // The cabin's front wall overlaps the south end's approach lane, while the north bank
        // leaves a clear ordinary route around the rail. The midpoint eye height must follow
        // the shared visible platform top over the water.
        TestStep.walk(x: 0.51451075, z: 17.5, maxTicks: 900)
        TestStep.walk(x: 3.5145109, z: 17.5, maxTicks: 900)
        TestStep.walk(x: 3.5145109, z: 21.128466, maxTicks: 900)
        TestStep.range("y", 3.65, 3.76)
        TestStep.capture("cabin-bridge-chord-midpoint")
        TestStep.walk(x: 3.5145109, z: 24.8, maxTicks: 1_200)
        TestStep.range("z", 24.55, 25.05)
        TestStep.walk(x: 3.5145109, z: 17.5, maxTicks: 1_200)
        TestStep.range("z", 17.25, 17.75)

        // Approach the west rail from outside the platform. One ordinary eastward move is
        // subdivided by SanctuaryWorld and must stop before the rail's x = 2.7395 m near face.
        TestStep.walk(x: 1.8, z: 17.5, maxTicks: 600)
        TestStep.walk(x: 1.8, z: 21.128466, maxTicks: 900)
        TestStep.action(.look(yaw: Float.pi / 2, pitch: -0.08))
        TestStep.action(.move(V3(0, 0, 3.4)))
        TestStep.range("x", 2.66, 2.72)
        TestStep.range("z", 20.88, 21.38)
        TestStep.capture("cabin-bridge-rail-blocked")

        // Reverse the move and both turns. Saving after those three production undos retains
        // the original ID, transform, one-record history, and both garden contributions.
        TestStep.action(.control("undoBuilding"))
        TestStep.action(.control("undoBuilding"))
        TestStep.action(.control("undoBuilding"))
        TestStep.action(.control("building-finish"))
        TestStep.range("buildingCount", 1, 1)
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.save("cabin-bridge-chord-restored")
        TestStep.walk(x: 4.8, z: 25.5, maxTicks: 900)
        TestStep.reopen("cabin-bridge-chord-restored")
        TestStep.range("x", 2.66, 2.72)
        TestStep.range("buildingCount", 1, 1)
        TestStep.range("gardenRevision", 2, 2)
        TestStep.range("gardenPatchCount", 2, 2)
        TestStep.action(.look(yaw: -Float.pi / 2, pitch: -0.08))
        TestStep.action(.control("building-select"))
        TestStep.capture("cabin-bridge-chord-restored")
      },
    ]
  }

  /// Keeps real authored actors at their normal population homes. The established route declares
  /// trust only; the Cabin edit route declares its already committed garden/construction
  /// state. Neither fixture observes, relocates, mounts, or grants a capability to an animal.
  @discardableResult public static func configure(
    fixture name: String, in world: SanctuaryWorld
  ) throws -> Bool {
    switch name {
    case establishedFixture:
      let cabin = SanctuaryGeography().landmark(for: .woodland).coordinate
      world.camera = PlayerCamera(
        position: V3(cabin.x, world.groundHeight(cabin.x, cabin.y) + 1.72, cabin.y),
        yaw: 0, pitch: -0.08)
      world.syncExpeditionPlayer()
      try SanctuaryFixtureRelationships.declareFamiliar("moonhart-001", in: world)
      try SanctuaryFixtureRelationships.declareFamiliar("canopy-glider-001", in: world)
      try SanctuaryFixtureRelationships.declareFamiliar("saltback-001", in: world)
    case cabinWaterBridgeFixture:
      // These positions and radii come from the isolated native Cabin save after ordinary Water,
      // Reeds, and Bridge confirmations. Fixture setup uses the same persisted production
      // transactions with a fresh fixture time, and does not claim those confirmations as steps.
      let center = HabitatGarden.Location(x: 0.014510762, z: 21.128466)
      _ = try world.controller.applyNature(
        .plant(.shallowWater, at: center, radius: 4), expectedRevision: 0)
      _ = try world.controller.applyNature(
        .plant(.reeds, at: center, radius: 3), expectedRevision: 1)
      let bed = PersonalConstruction.Location(
        x: center.x, y: world.groundHeight(center.x, center.z), z: center.z)
      _ = try world.commitPlacement(
        .place(.bridge, at: bed, yawRadians: 0, scale: 1), expectedRevision: 0)
      world.camera = PlayerCamera(
        position: V3(0.014510762, 3.1025672, 23.285015), yaw: 0, pitch: -0.67999995)
      world.syncExpeditionPlayer()
    default: return false
    }
    try world.validate()
    return true
  }
}
