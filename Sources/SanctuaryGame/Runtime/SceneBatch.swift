import FieldCompiler
import FieldCore
import simd

struct Instance {
  var model: simd_float4x4
  var tint: SIMD4<Float>
  init(
    position: V3 = .zero, scale: V3 = V3(repeating: 1), yaw: Float = 0, tint: V3 = V3(repeating: 1),
    kind: Float = 0
  ) {
    model = transform(position, scale, yaw)
    self.tint = SIMD4(tint, kind)
  }
}

func transform(_ p: V3, _ s: V3, _ yaw: Float) -> simd_float4x4 {
  let c = cos(yaw)
  let t = sin(yaw)
  return simd_float4x4(
    columns: (
      SIMD4(c * s.x, 0, -t * s.x, 0), SIMD4(0, s.y, 0, 0), SIMD4(t * s.z, 0, c * s.z, 0),
      SIMD4(p, 1)
    ))
}

struct SceneBatch {
  var name: String
  var mesh: Mesh
  var instances: [Instance]
  var grass: [GrassBlade] = []
  var lodMeshes: [Mesh] = []
  var roughness: Float = -1
  var doubleSided = false
  var metallic: Float = 0
}
