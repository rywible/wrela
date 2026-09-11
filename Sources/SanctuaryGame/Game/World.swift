import FieldCompiler
import FieldCore
import Foundation
import SanctuaryContent
import simd

struct Solid {
  var shape: Shape
  var position: V3
  var scale: Float
  var name: String
  func value(_ p: V3) -> Float { shape.value(at: (p - position) / scale) * scale }
}

struct GardenWorld {
  static let podMetres: Float = 0.035
  static var podUnitScale: Float { podMetres / (podShape.bounds.max.y - podShape.bounds.min.y) }
  let terrain = Terrain()
  var batches: [SceneBatch] = []
  var solids: [Solid] = []
  var collisionGrid = SpatialBoundsGrid(bounds: [])
  var studioMesh: Mesh
  var treeSource = AssetSource.defaults("tree")
  var studioShape: Shape
  let seed: UInt64 = 82317

  init(soundstageOnly: Bool = false) throws {
    func mesh(_ s: Shape, _ r: Int) throws -> Mesh { try Mesher.compile(s, resolution: r) }
    // A sculptural seed pod exercises smooth composition and subtraction.
    studioShape = Self.podShape
    studioMesh = try mesh(studioShape, 58)
    if !soundstageOnly {
      batches.append(
        SceneBatch(name: "Terrain", mesh: Mesher.terrain(terrain), instances: [Instance(kind: 1)]))
    }

    treeSource = try AssetSource.load("tree")
    let (branches, crown) = treeSource.treeFields()
    let trunk = branches
    let trunkMesh = try mesh(branches, 48)
    let crownMesh = try mesh(crown, 48)
    let rockShape = AssetSource.stoneField
    let rockMesh = try mesh(rockShape, 40)
    if soundstageOnly {
      batches = [
        SceneBatch(name: "Trunks", mesh: trunkMesh, instances: [Instance(kind: 4)]),
        SceneBatch(name: "Stones", mesh: rockMesh, instances: [Instance(kind: 5)]),
      ]
      return
    }
    var trunks: [Instance] = []
    var crowns: [Instance] = []
    var rocks: [Instance] = []
    func encounterClearing(_ x: Float, _ z: Float) -> Bool {
      let p = SIMD2(x, z)
      return distance(p, Expedition.den) < 6 || distance(p, Expedition.home) < 7
        || Expedition.signs.contains { distance(p, $0) < 2 }
    }
    var rng = SeededRandom(seed: seed)
    for _ in 0..<260 {
      let x = rng.range(-105, 105)
      let z = rng.range(-125, 65)
      if encounterClearing(x, z) { continue }
      let pathDistance = abs(x - terrain.pathX(z))
      if pathDistance < 5 || (abs(x) < 13 && z > 9 && z < 33) { continue }
      let s = rng.range(1.1, 2.3)
      let p = V3(x, terrain.height(x, z) - 0.15, z)
      let yaw = rng.range(0, 6.28)
      trunks.append(
        Instance(
          position: p, scale: V3(repeating: s), yaw: yaw, tint: V3(0.31, 0.25, 0.18), kind: 4))
      let gold = rng.next()
      let color =
        gold < 0.22
        ? V3(0.73, 0.70, 0.32)
        : V3(rng.range(0.33, 0.48), rng.range(0.57, 0.70), rng.range(0.27, 0.39))
      crowns.append(Instance(position: p, scale: V3(repeating: s), yaw: yaw, tint: color, kind: 2))
      solids.append(Solid(shape: trunk, position: p, scale: s, name: "Tree"))
    }
    for _ in 0..<95 {
      let x = rng.range(-75, 75)
      let z = rng.range(-105, 50)
      if abs(x - terrain.pathX(z)) < 3.4 || encounterClearing(x, z) { continue }
      let s = rng.range(0.5, 2.3)
      let p = V3(x, terrain.height(x, z), z)
      rocks.append(
        Instance(position: p, scale: V3(repeating: s), tint: V3(0.53, 0.57, 0.51), kind: 5))
      solids.append(Solid(shape: rockShape, position: p, scale: s, name: "Stone"))
    }
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
    let arch = Shape.capsule(V3(-3, 0, 0), V3(-2, 8, 0), 0.85)
      .blended(.capsule(V3(3, 0, 0), V3(2, 8, 0), 0.85), radius: 0.5)
      .blended(.capsule(V3(-2, 8, 0), V3(2, 8, 0), 1), radius: 0.75)
    let ap = V3(terrain.pathX(-67), terrain.height(terrain.pathX(-67), -67), -67)
    batches.append(
      SceneBatch(
        name: "Gate", mesh: try mesh(arch, 48),
        instances: [Instance(position: ap, tint: V3(0.58, 0.62, 0.53), kind: 5)]))
    solids.append(Solid(shape: arch, position: ap, scale: 1, name: "The old gate"))

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
    collisionGrid = SpatialBoundsGrid(
      bounds: solids.map {
        let b = $0.shape.bounds
        // A conservative field can be smaller than Euclidean distance.
        // Include its distance scale when bounding the player's clearance.
        return Bounds(b.min * $0.scale + $0.position, b.max * $0.scale + $0.position).expanded(
          0.28 / $0.shape.exteriorDistanceScale)
      })
    batches += [
      SceneBatch(name: "Meadow", mesh: Mesh(), instances: [Instance(kind: 3)], grass: grass),
      SceneBatch(name: "Wildflowers", mesh: flowers, instances: [Instance(kind: 3)]),
    ]
    for index in batches.indices {
      batches[index].doubleSided = ["Meadow", "Wildflowers", "Leaves"].contains(batches[index].name)
    }

  }

  static var podShape: Shape {
    Shape.sphere(1.15).blended(.sphere(0.72).moved(V3(0, 0.75, 0)), radius: 0.55)
      .blended(.capsule(V3(0, 1.1, 0), V3(0.48, 2.15, 0), 0.18), radius: 0.28)
      .cut(.sphere(0.82).moved(V3(0, 0.24, 0.85)))
  }
}

func perspective(_ fov: Float, _ aspect: Float, _ near: Float, _ far: Float) -> simd_float4x4 {
  let y = 1 / tan(fov / 2)
  let x = y / aspect
  let z = far / (near - far)
  return simd_float4x4(
    columns: (SIMD4(x, 0, 0, 0), SIMD4(0, y, 0, 0), SIMD4(0, 0, z, -1), SIMD4(0, 0, z * near, 0)))
}
func lookAt(_ eye: V3, _ target: V3, up: V3 = V3(0, 1, 0)) -> simd_float4x4 {
  let z = normalize(eye - target)
  let x = normalize(cross(up, z))
  let y = cross(z, x)
  return simd_float4x4(
    columns: (
      SIMD4(x.x, y.x, z.x, 0), SIMD4(x.y, y.y, z.y, 0), SIMD4(x.z, y.z, z.z, 0),
      SIMD4(-dot(x, eye), -dot(y, eye), -dot(z, eye), 1)
    ))
}
func orthographic(_ size: Float, _ near: Float, _ far: Float) -> simd_float4x4 {
  simd_float4x4(
    columns: (
      SIMD4(1 / size, 0, 0, 0), SIMD4(0, 1 / size, 0, 0), SIMD4(0, 0, 1 / (near - far), 0),
      SIMD4(0, 0, near / (near - far), 1)
    ))
}
