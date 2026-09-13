import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

/// Production tree source and template assembly. No alternate renderer selection math.
final class RegionalVegetationLODTests: XCTestCase {
  private struct Fixture {
    let trunk: Shape
    let crown: Mesh
    let templates: BiomePresentationTemplates
  }

  private static let fixture: Result<Fixture, Error> = Result {
    let sanctuary = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    let url = sanctuary.appendingPathComponent("Authoring/Assets/tree.json")
    let source = try JSONDecoder().decode(AssetSource.self, from: Data(contentsOf: url))
    let fields = SanctuaryLayout.treeFields(source.parameters)
    let trunk = try Mesher.compile(fields.0, resolution: 48)
    let crown = try Mesher.compile(fields.1, resolution: 48)
    let stone = try Mesher.compile(SanctuaryLayout.stoneField, resolution: 40)
    return Fixture(trunk: fields.0, crown: crown,
      templates: try BiomePresentationTemplates(trunk: trunk, broadCrown: crown, stone: stone))
  }

  private func leaves(_ fixture: Fixture) throws -> SceneBatch {
    try XCTUnwrap(fixture.templates.vegetationFull[.broadleaf]?.first {
      $0.name == "Population broadleaf leaves"
    })
  }

  private func xyz(_ p: SIMD4<Float>) -> V3 { V3(p.x, p.y, p.z) }

  private func bounds(_ mesh: Mesh) -> Bounds {
    let points = mesh.vertices.map { xyz($0.position) }
    return Bounds(points.reduce(V3(repeating: Float.greatestFiniteMagnitude), simd_min),
      points.reduce(V3(repeating: -Float.greatestFiniteMagnitude), simd_max))
  }

  func testProductionBroadleafLeavesHaveHalfTriangleLODAndUnchangedNearSource() throws {
    let fixture = try Self.fixture.get()
    let batch = try leaves(fixture)
    let full = batch.mesh
    let flat = try XCTUnwrap(batch.lodMeshes.first)
    XCTAssertEqual(full.indices.count / 3, 7_200)
    XCTAssertEqual(flat.indices.count / 3, 3_600)
    XCTAssertEqual(full.vertices.count, 9_000)
    XCTAssertEqual(flat.vertices.count, 7_200)
    XCTAssertEqual(flat.geometricError, 0.035, accuracy: 0.000001)
    XCTAssertTrue(batch.doubleSided)
    let original = Mesher.leaves(on: fixture.crown, count: 1_800)
    XCTAssertEqual(full.indices, original.indices)
    XCTAssertEqual(full.vertices.map(\.position), original.vertices.map(\.position))
    XCTAssertEqual(full.vertices.map(\.normal), original.vertices.map(\.normal))
    XCTAssertEqual(full.vertices.map(\.color), original.vertices.map(\.color))
    let originalVertices = Set(full.vertices.map(\.position))
    XCTAssertTrue(flat.vertices.allSatisfy { originalVertices.contains($0.position) })
    let fullBounds = bounds(full), flatBounds = bounds(flat)
    for axis in 0..<3 {
      XCTAssertGreaterThanOrEqual(flatBounds.min[axis], fullBounds.min[axis])
      XCTAssertLessThanOrEqual(flatBounds.max[axis], fullBounds.max[axis])
    }
    func receipt(_ mesh: Mesh) -> [String: Any] {
      let b = bounds(mesh)
      return ["vertices": mesh.vertices.count, "triangles": mesh.indices.count / 3,
        "sourceBytes": mesh.vertices.count * MemoryLayout<Vertex>.stride + mesh.indices.count * 4,
        "errorMetres": mesh.geometricError,
        "min": [b.min.x, b.min.y, b.min.z], "max": [b.max.x, b.max.y, b.max.z]]
    }
    let json = try JSONSerialization.data(withJSONObject: ["full": receipt(full), "flat": receipt(flat)],
      options: [.sortedKeys])
    print("RegionalVegetationLOD production receipt: " + String(decoding: json, as: UTF8.self))
  }

  private struct LeafKey: Hashable {
    let normal: SIMD4<Float>
    let color: SIMD4<Float>
  }

  func testEveryLeafKeepsItsBoundaryAndMeasuredRidgeErrorFitsDeclaredBound() throws {
    let batch = try leaves(Self.fixture.get())
    let flat = try XCTUnwrap(batch.lodMeshes.first)
    let original = Dictionary(grouping: batch.mesh.vertices) { LeafKey(normal: $0.normal, color: $0.color) }
    let reduced = Dictionary(grouping: flat.vertices) { LeafKey(normal: $0.normal, color: $0.color) }
    XCTAssertEqual(original.count, 1_800)
    XCTAssertEqual(Set(original.keys), Set(reduced.keys))
    var maximumDisplacement: Float = 0
    for (key, vertices) in original {
      let boundary = try XCTUnwrap(reduced[key])
      XCTAssertEqual(vertices.count, 5)
      XCTAssertEqual(boundary.count, 4)
      let positions = Set(boundary.map(\.position))
      let ridge = try XCTUnwrap(vertices.first { !positions.contains($0.position) })
      let center = boundary.reduce(V3.zero) { $0 + xyz($1.position) } * 0.25
      let displacement = distance(xyz(ridge.position), center)
      maximumDisplacement = max(maximumDisplacement, displacement)
      XCTAssertEqual(displacement, 0.035, accuracy: 0.000003)
      XCTAssertLessThanOrEqual(displacement, flat.geometricError + 0.000003)
      XCTAssertTrue(boundary.allSatisfy { vertex in vertices.contains { $0.position == vertex.position } })
    }
    print("RegionalVegetationLOD measured ridge displacement metres: \(maximumDisplacement)")
  }

  func testActualPopulationPlacementsKeepTransformsAndCollisionAcrossRepresentations() throws {
    let fixture = try Self.fixture.get()
    let terrain = Terrain()
    let population = SanctuaryBiomeVegetationPopulation(habitat: .init(terrain: terrain))
    let query = try population.query(in: SanctuaryMapBounds(minimum: .zero, maximum: SIMD2(repeating: 512)))
    let mask = SanctuaryVegetationLocalEditMask()
    let available = query.exact.filter { $0.species == .broadleaf }.compactMap { record in
      SanctuaryVegetationPlacement.resolve(record, terrain: terrain, garden: nil,
        decision: mask.decision(for: record), constructionFacts: [], treeTrunk: fixture.trunk)
    }
    let placements = Array(available.prefix(3))
    XCTAssertEqual(placements.count, 3)
    let sourceIDs = placements.map { $0.record.id }
    XCTAssertEqual(placements.compactMap { $0.solid?.name }, sourceIDs)
    for tier in SanctuaryVegetationResidencyTier.allCases {
      let batches = BiomeWorldPresentation.vegetationBatches(placements: [tier: placements],
        templates: fixture.templates)
      XCTAssertFalse(batches.isEmpty)
      for batch in batches {
        let prepared = try XCTUnwrap(batch.preparedUpload)
        XCTAssertTrue(prepared.matches(batch), "Instance assembly must retain exact prepared topology")
        XCTAssertEqual(batch.instances.count, placements.count)
        for (index, placement) in placements.enumerated() {
          let expected = transform(placement.position, V3(repeating: placement.record.scale), placement.record.yaw)
          for column in 0..<4 { XCTAssertEqual(batch.instances[index].model[column], expected[column]) }
        }
      }
      if tier == .detailed {
        let leafBatch = try XCTUnwrap(batches.first { $0.name.hasSuffix("Population broadleaf leaves") })
        XCTAssertEqual(leafBatch.lodMeshes.first?.indices.count, 10_800)
      }
    }
    XCTAssertEqual(placements.map { $0.record.id }, sourceIDs)
    XCTAssertEqual(placements.compactMap { $0.solid?.name }, sourceIDs)
  }
  func testAllImmutableVegetationTemplatesCarryPreparedUploadData() throws {
    let templates = try Self.fixture.get().templates
    var batchCount = 0, metadataBytes = 0
    for tier in [templates.vegetationFull, templates.vegetationMiddle, templates.vegetationFar] {
      for batches in tier.values {
        for batch in batches {
          let prepared = try XCTUnwrap(batch.preparedUpload)
          XCTAssertTrue(prepared.matches(batch))
          XCTAssertEqual(prepared.lods.count, batch.lodMeshes.count)
          XCTAssertGreaterThan(prepared.byteCount, 0)
          batchCount += 1
          metadataBytes += prepared.byteCount
        }
      }
    }
    print("RegionalVegetation prepared upload receipt: templates=\(batchCount) metadataBytes=\(metadataBytes)")
  }

  func testProductionSelectorKeepsNearLeavesAndHalvesDistantSubmittedTriangles() throws {
    let batch = try leaves(Self.fixture.get())
    let fullBounds = bounds(batch.mesh)
    let center = (fullBounds.min + fullBounds.max) * 0.5
    let radius = length(fullBounds.max - fullBounds.min) * 0.5 + 1
    let top = max(0, center.y + radius)
    let errors = batch.lodMeshes.map(\.geometricError)
    let triangleCounts = ([batch.mesh] + batch.lodMeshes).map { $0.indices.count / 3 }
    func selected(_ distance: Float, _ wind: Float, shadow: Bool = false,
      texels: Float = 0) -> Int {
      SceneLODSelection.selectedLevel(currentLOD: 0, errors: errors, scale: 1,
        distance: distance, kind: 8, top: top, wind: wind,
        shadow: shadow, shadowTexelsPerMetre: texels)
    }
    for wind: Float in [0, 1] {
      XCTAssertEqual(selected(1, wind), 0)
      XCTAssertEqual(triangleCounts[selected(1_024, wind)], 3_600)
      XCTAssertEqual(triangleCounts[selected(1, wind, shadow: true, texels: 100)], 7_200)
      let distances: [Float] = [1, 32, 64, 160, 512, 1_024]
      let receipt = distances.map { distance -> [String: Any] in
        let level = selected(distance, wind)
        return ["surfaceDistanceMetres": distance, "wind": wind,
          "selectedLevel": level, "submittedTriangles": triangleCounts[level]]
      }
      let json = try JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
      print("RegionalVegetationLOD production selector receipt: " + String(decoding: json, as: UTF8.self))
    }
  }

}
