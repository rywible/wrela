import FieldCompiler
import FieldCore
import simd

/// Small source-authored surfaces for cut timber and soft plant silhouettes.
/// These are compiled presentation meshes, never replacement collision geometry.
enum SanctuaryProceduralCraft {
  static func box(_ center: V3, _ half: V3, color: V3) -> Mesh {
    let outline: [SIMD2<Float>] = [
      SIMD2(center.x - half.x, center.y - half.y),
      SIMD2(center.x + half.x, center.y - half.y),
      SIMD2(center.x + half.x, center.y + half.y),
      SIMD2(center.x - half.x, center.y + half.y),
    ]
    return prism(outline, near: center.z - half.z, far: center.z + half.z, color: color)
  }

  /// A convex counterclockwise XY profile extruded along Z, with exact planar normals.
  static func prism(_ profile: [SIMD2<Float>], near: Float, far: Float, color: V3) -> Mesh {
    precondition(profile.count >= 3)
    var mesh = Mesh()
    func point(_ index: Int, _ z: Float) -> V3 {
      V3(profile[index].x, profile[index].y, z)
    }
    for index in 1..<(profile.count - 1) {
      mesh.triangle(
        Vertex(point(0, near), V3(0, 0, -1), color),
        Vertex(point(index + 1, near), V3(0, 0, -1), color),
        Vertex(point(index, near), V3(0, 0, -1), color))
      mesh.triangle(
        Vertex(point(0, far), V3(0, 0, 1), color),
        Vertex(point(index, far), V3(0, 0, 1), color),
        Vertex(point(index + 1, far), V3(0, 0, 1), color))
    }
    for index in profile.indices {
      let next = (index + 1) % profile.count
      let delta = profile[next] - profile[index]
      let normal = normalize(V3(delta.y, -delta.x, 0))
      let a = Vertex(point(index, near), normal, color)
      let b = Vertex(point(next, near), normal, color)
      let c = Vertex(point(next, far), normal, color)
      let d = Vertex(point(index, far), normal, color)
      mesh.triangle(a, b, c)
      mesh.triangle(a, c, d)
    }
    return mesh
  }

  static func rayTail() -> Mesh {
    // One taper replaces the visible join between two constant-radius capsules.
    let tail = ParametricMesh.tube(segments: 36, sides: 14, center: { t in
      let y: Float = 0.04 + 0.20 * t * t
      let x: Float = 0.035 * sin(t * Float.pi)
      return V3(x, y, 1.0 + 2.03 * t)
    }, radius: { t in
      0.105 * pow(1 - t, 0.85) + 0.012
    })
    let tip = ParametricMesh.ellipsoid(V3(0, 0.24, 3.03), V3(repeating: 0.012), detail: 10)
    return ParametricMesh.joined([tail, tip])
  }

  static func flower() -> Mesh {
    let stemColor = V3(0.25, 0.40, 0.19)
    let leafColor = V3(0.36, 0.49, 0.25)
    var pieces = [ParametricMesh.tube(segments: 10, sides: 7, center: { t in
      V3(0.014 * sin(t * Float.pi), t * 0.35, 0.008 * t)
    }, radius: { t in 0.008 - t * 0.0025 }, color: stemColor)]
    for side: Float in [-1, 1] {
      let y: Float = side < 0 ? 0.14 : 0.22
      let rotation = simd_quatf(angle: side * 0.58, axis: V3(0, 0, 1))
      let leaf = ParametricMesh.surface(u: 12, v: 8, color: leafColor) { u, v in
        let longitude: Float = u * 2 * Float.pi
        let latitude: Float = (0.0001 + v * 0.9998) * Float.pi
        let local = V3(0.066 * sin(latitude) * cos(longitude),
          0.006 * cos(latitude), 0.020 * sin(latitude) * sin(longitude))
        return rotation.act(local) + V3(side * 0.046, y, 0.004)
      }
      pieces.append(leaf)
    }
    for index in 0..<5 {
      let angle: Float = Float(index) * (2 * Float.pi / 5)
      let center = V3(cos(angle) * 0.047, 0.351, sin(angle) * 0.047 + 0.008)
      let rotation = simd_quatf(angle: -angle, axis: V3(0, 1, 0))
      let petalColor = V3(0.83, 0.76, 0.64) * (index % 2 == 0 ? Float(1) : 0.95)
      let petal = ParametricMesh.surface(u: 16, v: 8, color: petalColor) { u, v in
        let longitude: Float = u * 2 * Float.pi
        let latitude: Float = (0.0001 + v * 0.9998) * Float.pi
        let local = V3(0.047 * sin(latitude) * cos(longitude),
          0.010 * cos(latitude), 0.023 * sin(latitude) * sin(longitude))
        return rotation.act(local) + center
      }
      pieces.append(petal)
    }
    pieces.append(ParametricMesh.ellipsoid(V3(0, 0.359, 0.008), V3(0.025, 0.014, 0.025),
      color: V3(0.65, 0.47, 0.20), detail: 8))
    return ParametricMesh.joined(pieces)
  }
}
