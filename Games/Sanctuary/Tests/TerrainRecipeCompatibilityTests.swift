import FieldCore
import Foundation
import XCTest
import simd
@testable import SanctuaryContent

final class TerrainRecipeCompatibilityTests: XCTestCase {
  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  }

  private func assertUnsupported(_ action: () throws -> Void, file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertThrowsError(try action(), file: file, line: line) { error in
      guard case ExpeditionError.unsupportedVersion = error else {
        return XCTFail("Expected unsupported recipe/version, received \(error)", file: file, line: line)
      }
    }
  }

  private var future: SanctuaryTerrainRecipe {
    .init(generatorID: "sanctuary-uplift-drainage", version: 1)
  }

  func testLegacyRecipeRetainsReferenceHeightAndEveryLandmarkQuery() throws {
    let implicit = Terrain(), explicit = try Terrain(recipe: .legacy)
    XCTAssertEqual(explicit.recipe.identity, "sanctuary-legacy-v3@1")
    // Retained pre-injection CPU grade probe, seed-independent source, 2026-09-12:
    // .build/sanctuary-native-20260912/mesh-diagnosis/grade-candidate.json, profiles[0].
    let profile: [(Float, Float)] = [
      (10_745.171875, 342.37542724609375),
      (10_745.421875, 342.49801635742188),
      (11_385.171875, 462.384033203125),
    ]
    for (z, expected) in profile {
      XCTAssertEqual(explicit.height(-427, z), expected, accuracy: 0.001)
    }
    let sites = SanctuaryGeography.landmarks.map(\.coordinate)
      + [SIMD2<Float>(112, 78), SIMD2(-112, -142), SIMD2(512, 512), SIMD2(-512, 0)]
    for site in sites {
      for delta in [SIMD2<Float>.zero, SIMD2(0.25, 0), SIMD2(0, -0.25)] {
        let p = site + delta
        XCTAssertEqual(try implicit.surface(at: p), try explicit.surface(at: p))
        for body in SanctuaryWaterBody.allCases {
          XCTAssertEqual(implicit.waterField(for: body, at: p), explicit.waterField(for: body, at: p))
        }
      }
    }
    assertUnsupported { _ = try Terrain(recipe: future) }
    assertUnsupported { _ = try Terrain(recipe: .init(generatorID: SanctuaryTerrainRecipe.legacy.generatorID, version: 2)) }
  }

  func testSourceInjectionPreservesPlacementsFeatureIDsAndRandomContinuation() throws {
    let a = SanctuaryLayout(), b = SanctuaryLayout(terrain: try Terrain(recipe: .legacy))
    for (left, right) in [(a.trunks, b.trunks), (a.crowns, b.crowns), (a.rocks, b.rocks)] {
      XCTAssertEqual(left.count, right.count)
      for (x, y) in zip(left, right) {
        XCTAssertEqual(x.position, y.position); XCTAssertEqual(x.scale, y.scale)
        XCTAssertEqual(x.yaw, y.yaw); XCTAssertEqual(x.color, y.color); XCTAssertEqual(x.kind, y.kind)
      }
    }
    var ar = a.remainingRandom, br = b.remainingRandom
    for _ in 0..<64 { XCTAssertEqual(ar.next(), br.next()) }
    XCTAssertEqual(SanctuaryGeography.landmarks.map(\.id), [
      "cabin-glade", "golden-meadow", "willow-creek", "reed-mirror", "moonlake", "cloudstep",
      "sunglass-dunes", "verdant-canopy", "saltwind-bluffs", "lantern-tidepools", "open-blue",
    ])
    for site in SanctuaryGeography.landmarks {
      let key = SanctuaryTerrainChunkKey(containing: site.coordinate)
      let old = a.terrain.geography.plan(for: key), selected = b.terrain.geography.plan(for: key)
      XCTAssertEqual(old, selected)
      for feature in selected.features {
        if SanctuaryGeography.chunkKey(forFeatureID: feature.id) != nil {
          XCTAssertEqual(b.terrain.geography.feature(id: feature.id), feature)
        } else {
          XCTAssertNotNil(b.terrain.geography.landmark(id: feature.id))
        }
      }
    }
  }

  private func editedState() throws -> Expedition {
    var state = Expedition(seed: 82317)
    try state.initializeLivingWorld()
    let center = HabitatGarden.Location(x: 4.7, z: 3.8)
    for operation in [HabitatGarden.TerrainOperation.raise, .lower, .smooth] {
      _ = try state.applyNature(.sculpt(operation, at: center, radius: 6, amount: 0.5,
        targetHeight: operation == .smooth ? 2 : nil))
    }
    _ = try state.applyNature(.plant(.shallowWater, at: center, radius: 3))
    let geography = SanctuaryGeography()
    let features = (-2...2).flatMap { z in
      (-2...2).flatMap { x in geography.plan(for: .init(x: x, z: z)).features }
    }
    let boulder = try XCTUnwrap(features.first { $0.kind == .boulder })
    var moved = BoulderArrangements()
    _ = try moved.apply(.push(featureID: boulder.id, direction: SIMD2(1, 0), distance: 1,
      helperID: "moonhart-001"), expectedRevision: 0)
    state.boulders = moved
    return state
  }

  func testSchemaOneThroughThreeKeepMissingIdentityAndSavedEdits() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = ExpeditionStore(url: root.appendingPathComponent("expedition.json"))
    let original = try editedState()
    for schema in 1...3 {
      try store.save(original)
      var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: store.url)) as? [String: Any])
      object["version"] = schema
      try JSONSerialization.data(withJSONObject: object).write(to: store.url)
      let loaded = try store.load().state
      XCTAssertEqual(loaded, original)
      XCTAssertNil(loaded.terrainRecipe)
      XCTAssertEqual(loaded.resolvedTerrainRecipe, .legacy)
      let source = try Terrain(recipe: loaded.resolvedTerrainRecipe)
      let point = HabitatGarden.Location(x: 4.7, z: 3.8)
      XCTAssertEqual(loaded.garden?.surfaceHeight(baseHeight: source.height(point.x, point.z), at: point),
        original.garden?.surfaceHeight(baseHeight: Terrain().height(point.x, point.z), at: point))
      try store.save(loaded)
      let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: store.url)) as? [String: Any])
      XCTAssertEqual(encoded["version"] as? Int, 3)
      XCTAssertNil((encoded["expedition"] as? [String: Any])?["terrainRecipe"])
    }
    var explicit = original
    explicit.terrainRecipe = .legacy
    try store.save(explicit)
    XCTAssertEqual(try store.load().state, explicit)
  }

  func testUnsupportedRecipeCannotLoadFallbackOrOverwritePrimaryAndBackup() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = ExpeditionStore(url: root.appendingPathComponent("expedition.json"))
    let good = try editedState()
    try store.save(good); try store.save(good)
    let backup = try Data(contentsOf: store.backupURL)
    let valid = try Data(contentsOf: store.url)
    var unavailable = good
    unavailable.terrainRecipe = future
    assertUnsupported { try store.save(unavailable) }
    XCTAssertEqual(try Data(contentsOf: store.url), valid)
    XCTAssertEqual(try Data(contentsOf: store.backupURL), backup)
    var document = try XCTUnwrap(JSONSerialization.jsonObject(with: valid) as? [String: Any])
    document["expedition"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(unavailable))
    let unsupported = try JSONSerialization.data(withJSONObject: document)
    try unsupported.write(to: store.url)
    assertUnsupported { _ = try store.load() }
    assertUnsupported { try store.save(good) }
    XCTAssertEqual(try Data(contentsOf: store.url), unsupported)
    XCTAssertEqual(try Data(contentsOf: store.backupURL), backup)
  }

  func testWorldResolvesBeforeSupportAndRejectsUnsupportedRestoreAtomically() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    var state = try editedState()
    state.terrainRecipe = .legacy
    try ExpeditionStore(url: root.appendingPathComponent("saves/expedition.json")).save(state)
    let world = try SanctuaryWorld(root: root)
    XCTAssertEqual(world.world.terrain.recipe, state.resolvedTerrainRecipe)
    XCTAssertEqual(world.controller.state.garden, state.garden)
    XCTAssertEqual(world.controller.state.boulders, state.boulders)
    XCTAssertEqual(world.observations["terrainRecipe"], state.resolvedTerrainRecipe.identity)
    let before = try world.checkpoint()
    var snapshot = try XCTUnwrap(JSONSerialization.jsonObject(with: before) as? [String: Any])
    let encoded = try XCTUnwrap(snapshot["expedition"] as? String)
    let data = try XCTUnwrap(Data(base64Encoded: encoded))
    var memory = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    var expedition = try XCTUnwrap(memory["state"] as? [String: Any])
    expedition["terrainRecipe"] = ["generatorID": future.generatorID, "version": future.version]
    memory["state"] = expedition
    let futureMemory = try JSONSerialization.data(withJSONObject: memory)
    snapshot["expedition"] = futureMemory.base64EncodedString()
    assertUnsupported { try world.controller.restore(futureMemory) }
    assertUnsupported { try world.restore(JSONSerialization.data(withJSONObject: snapshot)) }
    XCTAssertEqual(try world.checkpoint(), before)
    try world.restore(before)
    XCTAssertEqual(try world.checkpoint(), before)
  }
}
