import FieldCore
import simd

/// Procedural surfaces are source programs; tessellation is a disposable representation.
/// Cross derivatives determine normals. Winding is consistent with those derivatives.
public enum ParametricMesh {
  /// Differential frame for ornaments attached to a parametric source surface.
  /// Orientation matches surface() winding; distance is signed in metres.
  public static func offsetPoint(u:Float,v:Float,distance:Float,position:(Float,Float)->V3)->V3 {
    let e:Float=0.0002
    let du=position(min(1,u+e),v)-position(max(0,u-e),v)
    let dv=position(u,min(1,v+e))-position(u,max(0,v-e))
    let n=cross(du,dv)
    return position(u,v)+(length_squared(n)>1e-16 ? normalize(n)*distance:.zero)
  }
  public static func surface(
    u: Int, v: Int, color: V3 = V3(repeating: 1),
    position: (Float, Float) -> V3, tint: ((Float, Float) -> V3)? = nil
  ) -> Mesh {
    precondition(u > 0 && v > 0 && u <= 1024 && v <= 1024)
    var mesh = Mesh()
    for j in 0...v {
      for i in 0...u {
        let a = Float(i) / Float(u)
        let b = Float(j) / Float(v)
        let e: Float = 0.0002
        let du = position(min(1, a + e), b) - position(max(0, a - e), b)
        let dv = position(a, min(1, b + e)) - position(a, max(0, b - e))
        let n = cross(du, dv)
        mesh.vertices.append(
          Vertex(
            position(a, b), length_squared(n) > 1e-16 ? normalize(n) : V3(0, 1, 0),
            tint?(a, b) ?? color))
      }
    }
    for j in 0..<v {
      for i in 0..<u {
        let a = UInt32(j * (u + 1) + i)
        let b = a + 1
        let c = a + UInt32(u + 1)
        let d = c + 1
        mesh.indices += [a, b, c, b, d, c]
      }
    }
    return mesh
  }
  public static func ellipsoid(
    _ center: V3, _ radius: V3, color: V3 = V3(repeating: 1), detail: Int = 24
  ) -> Mesh {
    surface(u: detail * 2, v: detail, color: color) { u, v in
      let a = u * 2 * Float.pi
      let b = (0.0001 + v * 0.9998) * Float.pi
      return center + radius * V3(sin(b) * cos(a), cos(b), sin(b) * sin(a))
    }
  }
  /// A parallel-ish local frame with a stable reference. Avoid tangent/reference alignment.
  public static func tube(
    segments: Int = 32, sides: Int = 10,
    center: (Float) -> V3, radius: (Float) -> Float, color: V3 = V3(repeating: 1),
    profile: ((Float,Float)->Float)? = nil
  ) -> Mesh {
    precondition(segments > 0 && sides >= 3)
    let centers = (0...segments).map { center(Float($0) / Float(segments)) }
    let tangents = (0...segments).map { i in
      normalize(centers[min(segments, i + 1)] - centers[max(0, i - 1)])
    }
    var normal = normalize(cross(tangents[0], abs(tangents[0].y) < 0.9 ? V3(0, 1, 0) : V3(1, 0, 0)))
    var points: [V3] = []
    for j in 0...segments {
      if j > 0 { normal = simd_quatf(from: tangents[j - 1], to: tangents[j]).act(normal) }
      normal = normalize(normal - tangents[j] * dot(normal, tangents[j]))
      let binormal = cross(tangents[j], normal)
      let r = radius(Float(j) / Float(segments))
      for i in 0...sides {
        let a = Float(i) / Float(sides) * 2 * Float.pi
        let shape=profile?(Float(j)/Float(segments),a) ?? 1
        precondition(shape.isFinite && shape>0)
        points.append(centers[j] + r * shape * (cos(a) * normal + sin(a) * binormal))
      }
    }
    var mesh = Mesh()
    for j in 0...segments {
      for i in 0...sides {
        let ring = j * (sides + 1)
        let index = ring + i
        let du = points[ring + (i + 1) % sides] - points[ring + (i + sides - 1) % sides]
        let dv =
          points[min(segments, j + 1) * (sides + 1) + i] - points[max(0, j - 1) * (sides + 1) + i]
        let n = cross(du, dv)
        mesh.vertices.append(
          Vertex(points[index], length_squared(n) > 1e-16 ? normalize(n) : V3(0, 1, 0), color))
      }
    }
    for j in 0..<segments {
      for i in 0..<sides {
        let a = UInt32(j * (sides + 1) + i)
        let b = a + 1
        let c = a + UInt32(sides + 1)
        mesh.indices += [a, b, c, b, c + 1, c]
      }
    }
    return mesh
  }

  public static func joined(_ meshes: [Mesh]) -> Mesh {
    var result = Mesh()
    for mesh in meshes {
      let start = UInt32(result.vertices.count)
      result.vertices += mesh.vertices
      result.indices += mesh.indices.map { $0 + start }
    }
    return result
  }
}
