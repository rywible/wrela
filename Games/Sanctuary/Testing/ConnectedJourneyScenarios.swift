import FieldCore
import SanctuaryContent
import SimulationCore
import TestKit
import simd

/// A connected, production-traversed Cabin Glade → Golden Meadow → Willow Creek route.
///
/// Fixture setup places the player at Cabin Glade and declares a familiar, willing Moonhart.
/// The scenario walks about 122 m to that real animal, then rides roughly 5.3 km on the authored
/// Cabin–Meadow and Meadow–Creek corridors. At riding speed the recorded route stays below
/// 22,000 fixed simulation ticks. No landmark or companion is relocated after fixture setup.
public enum SanctuaryConnectedJourneyScenarios {
  public static let fixture = "connected-cabin-meadow-creek"

  public static var fixtures: [String] { [fixture] }

  public static var tests: [GameTest] {
    let moonhart = WildlifePopulation.initial().actor(id: "moonhart-001")!.position
    let mountPoint = moonhart + SIMD2<Float>(0, 4)
    let meadow = SanctuaryGeography().landmark(for: .meadow)
    let creek = SanctuaryGeography().landmark(for: .creek)
    // The direct route is the creek centerline. The retained failure artifact
    // `20260912-192228-run-035d9` stopped on brookweaver-002's active dam at (2065, 770).
    // These are authored bank positions: the short pair crosses the shallow creek upstream,
    // then the route follows the opposite bank around the dam and approaches Willow Creek's
    // landmark from dry ground. They are movement targets, never fixture relocations.
    let eastBank = SIMD2<Float>(2_171, 498)
    let westBank = SIMD2<Float>(2_113, 482)
    let damDetour = SIMD2<Float>(2_010, 900)
    let creekBank = SIMD2<Float>(1_705, 2_177)
    return [
      GameTest(
        "connected-cabin-meadow-creek", fixture: fixture,
        tags: ["connected", "route", "exploration", "render"]
      ) {
        // Cabin Glade is observed before any movement. This is the only placement done by
        // the fixture; every later position comes from recorded production input.
        TestStep.advance(1)
        TestStep.expect("nearestLandmarkID", "cabin-glade")
        TestStep.expect("discoveredLandmarkIDs", "cabin-glade")
        TestStep.save("cabin-glade")
        TestStep.capture("connected-cabin-before-departure")

        TestStep.walk(x: mountPoint.x, z: mountPoint.y, maxTicks: 3_600)
        TestStep.action(.look(yaw: 0, pitch: -0.08))
        TestStep.expect("nearbyAnimalID", "moonhart-001")
        TestStep.action(.request("hello"))
        TestStep.expect("lastAnimalRequest", "greeting")
        // The named fixture supplies an already familiar companion; the actual control still
        // requires this nearby, visible, willing Moonhart.
        TestStep.action(.control("ride"))
        TestStep.expect("travelMode", "riding")

        TestStep.walk(x: meadow.coordinate.x, z: meadow.coordinate.y, maxTicks: 12_000)
        TestStep.expect("nearestLandmarkID", "golden-meadow")
        TestStep.expect("discoveredLandmarkIDs", "cabin-glade,golden-meadow")
        TestStep.save("golden-meadow")
        TestStep.capture("connected-golden-meadow")

        // The water source keeps this crossing shallow (0.42 m), while the active dam remains
        // solid collision. `ecologyStructureCount` exposes that the animal-owned structures
        // are present, although the public observation does not expose their individual IDs.
        TestStep.walk(x: eastBank.x, z: eastBank.y, maxTicks: 7_200)
        TestStep.range("ecologyStructureCount", 2, 128)
        TestStep.capture("connected-creek-east-bank")
        TestStep.walk(x: westBank.x, z: westBank.y, maxTicks: 600)
        TestStep.capture("connected-creek-crossing")
        TestStep.walk(x: damDetour.x, z: damDetour.y, maxTicks: 2_400)
        TestStep.walk(x: creekBank.x, z: creekBank.y, maxTicks: 5_600)
        TestStep.expect("nearestLandmarkID", "willow-creek")
        TestStep.expect("discoveredLandmarkIDs", "cabin-glade,golden-meadow,willow-creek")
        TestStep.save("willow-creek")
        TestStep.capture("connected-willow-creek")

        // The named destination checkpoint must preserve the journey and observed companion.
        TestStep.advance(120)
        TestStep.load("willow-creek")
        TestStep.expect("travelMode", "riding")
        TestStep.expect("discoveredLandmarkIDs", "cabin-glade,golden-meadow,willow-creek")
        TestStep.range("observedAnimals", 1, 128)
      },
    ]
  }

  /// Applies named initial placement plus the documented familiar Moonhart relationship. It does
  /// not establish discoveries, a mount, or any route progress.
  @discardableResult public static func configure(
    fixture name: String, in world: SanctuaryWorld
  ) throws -> Bool {
    guard name == fixture else { return false }
    let cabin = SanctuaryGeography().landmark(for: .woodland).coordinate
    world.camera = PlayerCamera(
      position: V3(cabin.x, world.groundHeight(cabin.x, cabin.y) + 1.72, cabin.y),
      yaw: 0, pitch: -0.08)
    world.syncExpeditionPlayer()
    try SanctuaryFixtureRelationships.declareFamiliar("moonhart-001", in: world)
    try world.validate()
    return true
  }
}
