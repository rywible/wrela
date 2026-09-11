import FieldCompiler
import FieldCore
import simd

package struct Instance {
  package var model: simd_float4x4
  package var tint: SIMD4<Float>
  package init(
    position: V3 = .zero, scale: V3 = V3(repeating: 1), yaw: Float = 0, tint: V3 = V3(repeating: 1),
    kind: Float = 0
  ) {
    model = transform(position, scale, yaw)
    self.tint = SIMD4(tint, kind)
  }
}

package func transform(_ p: V3, _ s: V3, _ yaw: Float) -> simd_float4x4 {
  let c = cos(yaw)
  let t = sin(yaw)
  return simd_float4x4(
    columns: (
      SIMD4(c * s.x, 0, -t * s.x, 0), SIMD4(0, s.y, 0, 0), SIMD4(t * s.z, 0, c * s.z, 0),
      SIMD4(p, 1)
    ))
}

package struct SceneBatch {
  package var name: String
  package var mesh: Mesh
  package var instances: [Instance]
  package var grass: [GrassBlade] = []
  package var lodMeshes: [Mesh] = []
  package var roughness: Float = -1
  package var doubleSided = false
  package var metallic: Float = 0

  package init(name:String,mesh:Mesh,instances:[Instance],grass:[GrassBlade]=[],lodMeshes:[Mesh]=[],roughness:Float = -1,doubleSided:Bool=false,metallic:Float=0) {
    self.name=name;self.mesh=mesh;self.instances=instances;self.grass=grass;self.lodMeshes=lodMeshes;self.roughness=roughness;self.doubleSided=doubleSided;self.metallic=metallic
  }
}
