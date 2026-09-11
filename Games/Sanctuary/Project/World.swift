import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

struct GardenWorld {
  static let podMetres: Float = 0.035
  static var podUnitScale: Float { podMetres / (podShape.bounds.max.y - podShape.bounds.min.y) }
  let terrain = Terrain()
  var batches: [SceneBatch] = []
  var layout: SanctuaryLayout
  var solids: [Solid] { layout.solids }
  var collisionGrid: SpatialBoundsGrid { layout.collisionGrid }
  var studioMesh: Mesh
  var treeSource = AssetSource.defaults("tree")
  var studioShape: Shape
  let seed: UInt64 = 82317

  init() throws {
    func mesh(_ s: Shape, _ r: Int) throws -> Mesh { try Mesher.compile(s, resolution: r) }
    // A sculptural seed pod exercises smooth composition and subtraction.
    studioShape = Self.podShape
    studioMesh = try mesh(studioShape, 58)
    batches.append(
      SceneBatch(name: "Terrain", mesh: Mesher.terrain(terrain), instances: [Instance(kind: 1)]))

    treeSource = try AssetSource.load("tree")
    let (branches, crown) = treeSource.treeFields()
    let trunkMesh = try mesh(branches, 48)
    let crownMesh = try mesh(crown, 48)
    let rockShape = AssetSource.stoneField
    let rockMesh = try mesh(rockShape, 40)
    layout = SanctuaryLayout(parameters: treeSource.parameters)
    func instances(_ placements: [Placement]) -> [Instance] {
      placements.map {
        Instance(position: $0.position, scale: $0.scale, yaw: $0.yaw, tint: $0.color, kind: $0.kind)
      }
    }
    let trunks = instances(layout.trunks)
    let crowns = instances(layout.crowns)
    let rocks = instances(layout.rocks)
    var rng = layout.remainingRandom
    batches += [
      SceneBatch(
        name: "Trunks", mesh: trunkMesh, instances: trunks,
        lodMeshes: [
          MeshProcessing.simplify(trunkMesh, ratio: 0.45, error: 0.012),
          MeshProcessing.simplify(trunkMesh, ratio: 0.18, error: 0.035),
        ]),
      SceneBatch(
        name: "Canopies", mesh: crownMesh, instances: crowns,
        lodMeshes: [
          MeshProcessing.simplify(crownMesh, ratio: 0.35, error: 0.03),
          MeshProcessing.simplify(crownMesh, ratio: 0.12, error: 0.08),
        ]),
      SceneBatch(
        name: "Stones", mesh: rockMesh, instances: rocks,
        lodMeshes: [
          MeshProcessing.simplify(rockMesh, ratio: 0.4, error: 0.012),
          MeshProcessing.simplify(rockMesh, ratio: 0.15, error: 0.04),
        ]),
    ]

    batches.append(
      SceneBatch(
        name: "Leaves",
        mesh: Mesher.leaves(on: crownMesh, count: Int(treeSource.parameters["leaves"] ?? 1800)),
        instances: crowns.map {
          var i = $0
          i.tint.w = 8
          return i
        },
        lodMeshes: [
          Mesher.leaves(
            on: crownMesh, count: Int(treeSource.parameters["leaves"] ?? 1800), flattened: true)
        ]))
    for index in batches.indices
    where ["Trunks", "Canopies", "Leaves"].contains(batches[index].name) {
      batches[index].roughness = treeSource.appearance["roughness"] ?? -1
      batches[index].metallic = treeSource.appearance["metallic"] ?? 0
      for i in batches[index].instances.indices {
        let kind = batches[index].instances[i].tint.w
        batches[index].instances[i].tint *= treeSource.appearance["tint"] ?? 1
        batches[index].instances[i].tint.w = kind
      }
    }

    // A landmark assembled from bounded fields: two leaning uprights and a lintel.
    let arch = SanctuaryLayout.arch
    let ap = layout.gatePosition
    batches.append(
      SceneBatch(
        name: "Gate", mesh: try mesh(arch, 48),
        instances: [Instance(position: ap, tint: V3(0.58, 0.62, 0.53), kind: 5)]))

    var grass: [GrassBlade] = []
    var flowers = Mesh()
    for _ in 0..<24000 {
      let x = rng.range(-76, 76)
      let z = rng.range(-108, 54)
      if abs(x - terrain.pathX(z)) < rng.range(2.2, 3.4) { continue }
      let p = V3(x, terrain.height(x, z) - 0.015, z)
      let angle = rng.range(0, 6.28)
      let h = rng.range(0.18, 0.48)
      let w = rng.range(0.018, 0.045)
      let col = V3(rng.range(0.30, 0.49), rng.range(0.49, 0.64), rng.range(0.17, 0.30))
      grass.append(GrassBlade(anchor: p, angle: angle, height: h, width: w, color: col))
      if rng.next() < 0.065 && z > -60 {
        let fp = p + V3(0, h, 0)
        let fc = rng.next() < 0.6 ? V3(0.92, 0.71, 0.68) : V3(0.83, 0.85, 0.60)
        for j in 0..<5 {
          let a = Float(j) * 1.256
          let b = a + 1.0
          flowers.triangle(
            Vertex(fp, V3(0, 1, 0), fc, wind: 1),
            Vertex(fp + V3(cos(a) * 0.16, 0.045, sin(a) * 0.16), V3(0, 1, 0), fc, wind: 1),
            Vertex(fp + V3(cos(b) * 0.16, 0.045, sin(b) * 0.16), V3(0, 1, 0), fc, wind: 1))
        }
      }
    }
    batches += [
      SceneBatch(name: "Meadow", mesh: Mesh(), instances: [Instance(kind: 3)], grass: grass),
      SceneBatch(name: "Wildflowers", mesh: flowers, instances: [Instance(kind: 3)]),
    ]
    for index in batches.indices {
      batches[index].doubleSided = ["Meadow", "Wildflowers", "Leaves"].contains(batches[index].name)
    }

  }

  static var podShape: Shape { SanctuaryLayout.podShape }
}
