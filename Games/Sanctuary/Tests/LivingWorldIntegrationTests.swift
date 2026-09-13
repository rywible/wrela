import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class LivingWorldIntegrationTests: XCTestCase {
  private func livingWorld(root: URL? = nil) throws -> SanctuaryWorld {
    let world = try SanctuaryWorld(root: root, seed: 17)
    world.camera.position = SIMD3<Float>(2, world.groundHeight(2, 23) + 1.72, 23)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    return world
  }

  private func companionWorld(_ id: String, root: URL? = nil) throws -> SanctuaryWorld {
    let world = try SanctuaryWorld(root: root, seed: 17)
    // Mounting is under test; relationship pacing is covered in
    // WildlifePopulationTests with real, spaced interactions.
    try SanctuaryRelationshipFixture.establish(id, in: world)
    let animal = try XCTUnwrap(world.controller.state.population.actor(id: id))
    let start = animal.position + SIMD2<Float>(0, 4)
    world.camera.position = SIMD3<Float>(
      start.x, world.groundHeight(start.x, start.y) + 1.72, start.y)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    return world
  }

  private func riderWorld(root: URL? = nil) throws -> SanctuaryWorld {
    try companionWorld("moonhart-001", root: root)
  }

  private func flierWorld(root: URL? = nil) throws -> SanctuaryWorld {
    try companionWorld("canopy-glider-001", root: root)
  }

  private func assertMountedMovement(_ world: SanctuaryWorld, control: String) throws {
    _ = try world.control(control)
    let companionID = try XCTUnwrap(world.controller.state.travel.companionID)
    world.move(SIMD3<Float>(0, 0, 5))
    let expectedPosition = SIMD2<Float>(world.camera.position.x, world.camera.position.z)
    XCTAssertEqual(world.controller.state.population.actor(id: companionID)?.position, expectedPosition)

    let checkpoint = try world.checkpoint()
    world.advance(1 / 60, running: false)
    XCTAssertFalse(world.controller.message.hasPrefix("Wildlife update:"))
    XCTAssertEqual(world.controller.state.population.actor(id: companionID)?.position, expectedPosition)
    let nextTick = try world.checkpoint()
    try world.restore(checkpoint)
    world.advance(1 / 60, running: false)
    XCTAssertEqual(try world.checkpoint(), nextTick)
  }

  private func blockSaves(in root: URL) throws {
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    try Data("not a directory".utf8).write(to: saves)
  }

  func testInvalidProductionActionsPreserveTheEntireLivingWorld() throws {
    let world = try livingWorld()
    world.advance(1 / 60, running: false)
    let before = try world.checkpoint()

    XCTAssertThrowsError(try world.request("describe a new animal for me"))
    XCTAssertEqual(try world.checkpoint(), before)
    XCTAssertThrowsError(try world.control("build-imaginary-tower"))
    XCTAssertEqual(try world.checkpoint(), before)

    _ = try world.control("build-bench")
    let built = try world.checkpoint()
    XCTAssertThrowsError(try world.control("build-bench"))
    XCTAssertEqual(try world.checkpoint(), built)
  }

  func testLivingPopulationConstructionAndDiscoverySurviveSaveAndFuture() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try livingWorld(root: root)
    world.advance(1 / 60, running: false)
    _ = try world.request("hello")
    _ = try world.control("flowers")
    _ = try world.control("build-bench")
    _ = try world.control("journal")
    let saved = try world.checkpoint()
    let savedState = world.controller.state

    world.advance(1, running: false)
    try world.restore(saved)
    XCTAssertEqual(world.controller.state, savedState)

    let reopened = try SanctuaryWorld(root: root, seed: 17)
    XCTAssertEqual(reopened.controller.state, savedState)
    XCTAssertEqual(
      reopened.observations["populationCount"], String(WildlifePopulation.initial().actors.count))
    XCTAssertEqual(reopened.observations["buildingCount"], "1")
    XCTAssertEqual(reopened.observations["observedAnimals"], "1")
    XCTAssertEqual(reopened.observations["discoveredPlaces"], "1")

    world.advance(1, running: false)
    reopened.advance(1, running: false)
    XCTAssertEqual(reopened.controller.state, world.controller.state)

    let autosaveRoot = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: autosaveRoot) }
    let autosaveWorld = try livingWorld(root: autosaveRoot)
    for _ in 0..<300 { autosaveWorld.advance(1 / 60, running: false) }
    let autosavedPopulation = autosaveWorld.controller.state
    let autosaved = try SanctuaryWorld(root: autosaveRoot, seed: 17)
    XCTAssertEqual(autosaved.controller.state, autosavedPopulation)
  }

  func testConstructionAndCompanionSaveFailuresAreAtomic() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let builder = try livingWorld(root: root)
    try blockSaves(in: root)
    let beforeConstruction = try builder.checkpoint()
    XCTAssertThrowsError(try builder.control("build-bench"))
    XCTAssertEqual(try builder.checkpoint(), beforeConstruction)

    try FileManager.default.removeItem(at: root.appendingPathComponent("saves"))
    let rider = try riderWorld(root: root)
    try blockSaves(in: root)
    let beforeCompanion = try rider.checkpoint()
    XCTAssertThrowsError(try rider.control("ride"))
    XCTAssertEqual(try rider.checkpoint(), beforeCompanion)
  }

  func testMountedMovementSaveReopenAndNextTickStaySynchronized() throws {
    for (id, control, mode) in [
      ("moonhart-001", "ride", "riding"), ("canopy-glider-001", "fly", "flying"),
    ] {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
      defer { try? FileManager.default.removeItem(at: root) }
      let world = try companionWorld(id, root: root)
      try assertMountedMovement(world, control: control)
      try world.controller.save()
      let expectedState = world.controller.state
      let expectedCamera = world.camera

      let reopened = try SanctuaryWorld(root: root, seed: 17)
      XCTAssertEqual(reopened.controller.state, expectedState, id)
      XCTAssertEqual(reopened.observations["travelMode"], mode, id)
      XCTAssertEqual(reopened.camera, expectedCamera, id)
    }
  }
  func testFlyingCheckpointUsesSavedEditedEcologyPoolHeight() throws {
    let world = try flierWorld()
    _ = try world.control("fly")
    var population = world.controller.state.population
    let builder = try XCTUnwrap(population.actor(id: "brookweaver-001"))
    // Fixture advances the real habitat work; movement is not under test here.
    for _ in 0..<181 {
      try population.advance(seconds: 1, player: builder.position + SIMD2(20, 0),
        running: false, garden: HabitatGarden(), canTraverse: { _, _ in true },
        isVisible: { _ in false })
    }
    let pool = try XCTUnwrap(population.ecology.waterFacts.first {
      $0.structureID.hasPrefix(builder.id)
    })
    try world.controller.editLiving { state in
      state.wildlife = population
      _ = try state.applyNature(.sculpt(.raise,
        at: .init(x: pool.center.x, z: pool.center.y), radius: 2, amount: 4, targetHeight: nil))
    }
    let point = pool.center + SIMD2<Float>(4, 0)
    world.camera.position = SIMD3(point.x,
      world.flightSurfaceHeight(point.x, point.y) + world.controller.state.travel.flightHeight, point.y)
    world.syncExpeditionPlayer()
    XCTAssertGreaterThan(world.flightSurfaceHeight(point.x, point.y), world.groundHeight(point.x, point.y) + 0.55)
    let saved = try world.checkpoint()
    _ = try world.control("undoPlanting")
    try world.restore(saved)
    XCTAssertEqual(try world.checkpoint(), saved)
  }

  func testEarthShapingPersistsTheRegroundedCameraBeforeReturning() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try livingWorld(root: root)
    // Enter the ordinary foot-supported pose before measuring reversible edits;
    // the helper's initial camera is the historical center-only terrain sample.
    world.move(.zero)
    world.syncExpeditionPlayer()
    for _ in 0..<10 { _ = try world.control("brush-larger") }
    _ = try world.control("spell-stronger")
    let before = world.camera.position.y
    _ = try world.control("raise")
    XCTAssertGreaterThan(world.camera.position.y, before + 0.5)
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.camera, world.camera)
    XCTAssertEqual(reopened.controller.state, world.controller.state)
    _ = try world.control("undoPlanting")
    let undone = try SanctuaryWorld(root: root)
    XCTAssertEqual(undone.camera, world.camera)
    XCTAssertEqual(world.camera.position.y, before, accuracy: 0.001)
  }

}
