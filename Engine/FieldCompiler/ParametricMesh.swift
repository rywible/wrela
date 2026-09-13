import FieldCore
import simd

/// Procedural surfaces are source programs; tessellation is a disposable representation.
/// Cross derivatives determine normals. Winding is consistent with those derivatives.
public enum ParametricMesh {
  /// Normalize direction before measuring the frame angle. A raw derivative cross
  /// product has units of metres squared: an absolute area cutoff incorrectly
  /// rejects perfectly regular millimetre-sized surfaces. Rescaling also avoids
  /// overflow/underflow in the vector length itself.
  private static func unitDirection(_ value: V3) -> V3? {
    guard value.x.isFinite, value.y.isFinite, value.z.isFinite else { return nil }
    let scale = max(abs(value.x), max(abs(value.y), abs(value.z)))
    guard scale > 0 else { return nil }
    let scaled = value / scale
    return scaled / sqrt(length_squared(scaled))
  }

  private static func frameNormal(_ du: V3, _ dv: V3) -> V3? {
    guard let u = unitDirection(du), let v = unitDirection(dv) else { return nil }
    let crossed = cross(u, v)
    // Dimensionless sin(angle)^2, independent of the object's metre scale.
    guard length_squared(crossed) > 1e-12 else { return nil }
    return unitDirection(crossed)
  }

  private static func surfaceNormal(
    u: Float, v: Float, position: (Float, Float) -> V3
  ) -> V3? {
    let e: Float = 0.0002
    let du = position(min(1, u + e), v) - position(max(0, u - e), v)
    let dv = position(u, min(1, v + e)) - position(u, max(0, v - e))
    // Differences cannot recover detail already rounded away inside a Float
    // position closure. Author small surfaces locally, then instance/translate.
    return frameNormal(du, dv)
  }

  /// Differential frame for ornaments attached to a parametric source surface.
  /// Orientation matches surface() winding; distance is signed in metres.
  public static func offsetPoint(u:Float,v:Float,distance:Float,position:(Float,Float)->V3)->V3 {
    return position(u,v)+(surfaceNormal(u:u,v:v,position:position).map { $0 * distance } ?? .zero)
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
        let n = surfaceNormal(u:a,v:b,position:position) ?? V3(0, 1, 0)
        mesh.vertices.append(
          Vertex(
            position(a, b), n,
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
    var mesh = surface(u: detail * 2, v: detail, color: color) { u, v in
      let a = u * 2 * Float.pi
      let b = (0.0001 + v * 0.9998) * Float.pi
      return center + radius * V3(sin(b) * cos(a), cos(b), sin(b) * sin(a))
    }
    // This source has an exact normal. Evaluate it in local coordinates rather
    // than subtracting a possibly large center or differencing near its poles.
    // Signed radii preserve the existing triangle winding for mirrored sources.
    let orientation: Float = (radius.x < 0 ? -1 : 1) * (radius.y < 0 ? -1 : 1)
      * (radius.z < 0 ? -1 : 1)
    for index in mesh.vertices.indices {
      let u = Float(index % (detail * 2 + 1)) / Float(detail * 2)
      let v = Float(index / (detail * 2 + 1)) / Float(detail)
      let a = u * 2 * Float.pi, b = (0.0001 + v * 0.9998) * Float.pi
      let normal = unitDirection(V3(sin(b) * cos(a), cos(b), sin(b) * sin(a)) / radius)
      mesh.vertices[index].normal = SIMD4(normal.map { $0 * orientation } ?? V3(0, 1, 0), 0)
    }
    return mesh
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
      unitDirection(centers[min(segments, i + 1)] - centers[max(0, i - 1)]) ?? V3(0, 1, 0)
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
        let n = frameNormal(du, dv) ?? V3(0, 1, 0)
        mesh.vertices.append(
          Vertex(points[index], n, color))
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
