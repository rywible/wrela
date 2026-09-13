import FieldCompiler
import FieldCore
import FieldEngine
import simd

/// Isolated calibration for the existing coverage/A2C path. Not a coat recipe.
/// Left is planar; right uses the0212 undercoat's along-guide lift in metres.
enum SanctuaryCoatCoverageStudy {
  static let analyticKind: Float = 30
  static let opaqueKind: Float = 31
  static let width: Float = 0.055
  static let length: Float = 0.067
  static let controls: [ScalarControl] = [
    .init("coverageMode", "Mode · 0 material / 1 coverage / 2 opaque", 1, 0...2),
    .init("density", "Source fibre coverage", 0.94, 0.05...1),
    .init("period", "Strand interval · m", 0.0008, 0.0002...0.004),
  ]
  static var generator: AssetGenerator {
    AssetGenerator(id: "coat-coverage", name: "Coat · Coverage calibration",
      controls: controls, compile: { try compile($0.parameters) })
  }

  static func elevation(_ t: Float) -> Float {
    -0.0015 + 0.004 * sin(t * .pi) + t * 0.003
  }
  private static let geometry = [patch(lifted: false), patch(lifted: true)]
  private static let substrate: Mesh = {
    var meshes = [rectangle(x0: -0.082, x1: 0.082, y0: 0.012, y1: 0.112,
      z: -0.006, color: .zero)]
    // Visible linear black/.18/white reference chips below the two patches.
    for (index, value) in [Float(0), 0.18, 1].enumerated() {
      let x = Float(index - 1) * 0.047
      meshes.append(rectangle(x0: x - 0.02, x1: x + 0.02, y0: 0, y1: 0.008,
        z: -0.005, color: V3(repeating: value)))
    }
    return ParametricMesh.joined(meshes)
  }()

  private static func rectangle(x0: Float, x1: Float, y0: Float, y1: Float,
    z: Float, color: V3) -> Mesh {
    var mesh = Mesh()
    mesh.vertices = [V3(x0,y0,z), V3(x1,y0,z), V3(x0,y1,z), V3(x1,y1,z)]
      .map { Vertex($0, V3(0,0,1), color) }
    mesh.indices = [0,1,2,1,3,2]
    return mesh
  }
  private static func patch(lifted: Bool) -> Mesh {
    var mesh = Mesh()
    let centerX: Float = lifted ? 0.04 : -0.04
    for row in 0...24 {
      let t = Float(row) / 24
      let taper: Float = 1 - 0.18 * t
      let z: Float = lifted ? elevation(t) : 0
      let dz: Float = lifted ? 0.004 * .pi * cos(t * .pi) + 0.003 : 0
      for column in 0...8 {
        let u = Float(column) / 8
        let x = centerX + (u - 0.5) * width * taper
        let point = V3(x, 0.025 + t * length, z)
        let du = V3(width * taper, 0, 0)
        let dv = V3((u - 0.5) * width * -0.18, length, dz)
        var vertex = Vertex(point, normalize(cross(du,dv)), V3(repeating: 1))
        vertex.groom = SIMD4(370 + u * width / 0.0008, t, 0.94, 0.18)
        mesh.vertices.append(vertex)
      }
    }
    for row in 0..<24 {
      for column in 0..<8 {
        let a = UInt32(row * 9 + column), b = a + 1, c = a + 9
        mesh.indices += [a,b,c,b,c+1,c]
      }
    }
    return mesh
  }

  static func compile(_ parameters: [String: Float]) throws -> [SceneBatch] {
    for control in controls {
      let value = parameters[control.key] ?? control.initial
      guard value.isFinite, control.range.contains(value) else {
        throw RuntimeError.message("Invalid coverage calibration control: " + control.key)
      }
    }
    let mode = parameters["coverageMode"] ?? 1
    guard mode.rounded() == mode else { throw RuntimeError.message("Coverage mode must be0,1 or2") }
    let kind: Float = mode == 0 ? 13 : mode == 1 ? analyticKind : opaqueKind
    let density = parameters["density"] ?? 0.94, period = parameters["period"] ?? 0.0008
    var batches = [SceneBatch(name: "Coverage black substrate and linear reference chips",
      mesh: substrate, instances: [Instance(kind: opaqueKind)], doubleSided: true)]
    for index in geometry.indices {
      var mesh = geometry[index]
      for vertex in mesh.vertices.indices {
        let u = Float(vertex % 9) / 8
        mesh.vertices[vertex].groom.x = 370 + u * width / period
        mesh.vertices[vertex].groom.z = density
      }
      batches.append(SceneBatch(name: index == 0 ? "Coverage planar patch" : "Coverage lifted patch",
        mesh: mesh, instances: [Instance(kind: kind)], roughness: 0.86, doubleSided: true))
    }
    return batches
  }
}
