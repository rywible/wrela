import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import simd

extension AssetSource {
  func treeFields() -> (Shape, Shape) { SanctuaryLayout.treeFields(parameters) }
  static var stoneField: Shape { SanctuaryLayout.stoneField }
  func compileSanctuary() throws -> [SceneBatch] {
    func batch(
      _ name: String, _ shape: Shape, _ tint: V3, _ kind: Float, _ roughness: Float = -1,
      _ metallic: Float = 0
    ) throws -> SceneBatch {
      var b = SceneBatch(
        name: name, mesh: try Mesher.compile(shape, resolution: 48),
        instances: [Instance(tint: tint, kind: kind)])
      b.roughness = roughness
      b.metallic = metallic
      return b
    }
    switch generator {
    case "tree", "branch":
      let (trunk, crown) = treeFields()
      var result = [try batch("Trunks", trunk, V3(0.31, 0.25, 0.18), 4)]
      if generator == "tree" {
        let canopy = try batch("Canopies", crown, V3(0.40, 0.64, 0.31), 2)
        result.append(canopy)
        result.append(
          SceneBatch(
            name: "Leaves",
            mesh: Mesher.leaves(on: canopy.mesh, count: Int(parameters["leaves"] ?? 1800)),
            instances: [Instance(tint: V3(0.40, 0.64, 0.31), kind: 8)]))
      }
      return result
    case "seed":
      var seed = try batch("Seed pod", GardenWorld.podShape, V3(0.48, 0.27, 0.13), 6)
      seed.instances[0].model = transform(.zero, V3(repeating: GardenWorld.podUnitScale), 0)
      return [seed]
    case "stone": return [try batch("Stones", Self.stoneField, V3(0.53, 0.57, 0.51), 5)]
    case "calibration":
      return try (0..<5).map { i in
        try batch(
          "Sphere \(i+1)", Shape.sphere(0.45).moved(V3(Float(i - 2) * 1.1, 0.45, 0)),
          V3(repeating: 0.55), 7, [Float(0.08), 0.25, 0.5, 0.9, 0.04][i], i == 4 ? 1 : 0)
      }
    default:
      return try resolvedParts.enumerated().map { i, p in
        try batch(
          p.key, p.field.shape(), V3(p.color[0], p.color[1], p.color[2]),
          Float(p.material), p.roughness, p.metallic)
      }
    }
  }
}
