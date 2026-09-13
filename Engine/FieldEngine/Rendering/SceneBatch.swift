import FieldCompiler
import FieldCore
import CryptoKit
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
  package var skinJoints: [String] = []
  package var skinWeights: [SkinWeight] = []
  package var name: String
  package var mesh: Mesh { didSet { preparedUpload = nil } }
  package var instances: [Instance]
  package var grass: [GrassBlade] = []
  package var lodMeshes: [Mesh] = [] { didSet { preparedUpload = nil } }
  package var roughness: Float = -1
  package var doubleSided = false
  package var metallic: Float = 0
  /// Recipe-owned rigid surface reduction, applied after source sculpting.
  /// Skinned surfaces need a binding-aware reduction policy instead.
  package var surfaceReduction: (ratio:Float,error:Float)?
  package private(set) var preparedUpload: PreparedSceneUpload?

  package init(name:String,mesh:Mesh,instances:[Instance],grass:[GrassBlade]=[],lodMeshes:[Mesh]=[],roughness:Float = -1,doubleSided:Bool=false,metallic:Float=0) {
    self.name=name;self.mesh=mesh;self.instances=instances;self.grass=grass;self.lodMeshes=lodMeshes;self.roughness=roughness;self.doubleSided=doubleSided;self.metallic=metallic
  }
}

/// CPU metadata derived from immutable mesh source before a batch reaches the render thread.
/// Instances and material values intentionally remain outside this payload: changing either
/// does not require recomputing bounds or meshlets. Exact source signatures prevent a payload
/// from being reused after an in-place base or LOD mesh mutation.
package struct PreparedSceneUpload: Sendable {
  package struct MeshletDescriptor: Sendable {
    package var ranges: SIMD4<UInt32>
    package var sphere: SIMD4<Float>
  }

  package struct Level: Sendable {
    fileprivate let signature: SHA256.Digest
    package let center: V3
    package let radius: Float
    package let descriptors: [MeshletDescriptor]
    package let meshletVertices: [UInt32]
    package let meshletTriangles: [UInt8]
    package let hasGroomCoverage: Bool
  }

  package let base: Level
  package let lods: [Level]

  /// Packed meshlet-array bytes, excluding fixed struct/array overhead and the retained source mesh.
  package var byteCount: Int {
    ([base] + lods).reduce(0) { total, level in
      total + level.descriptors.count * MemoryLayout<MeshletDescriptor>.stride
        + level.meshletVertices.count * MemoryLayout<UInt32>.stride
        + level.meshletTriangles.count * MemoryLayout<UInt8>.stride
    }
  }

  package func matches(_ batch: SceneBatch) -> Bool {
    guard batch.grass.isEmpty, lods.count == batch.lodMeshes.count,
      base.signature == Self.signature(of: batch.mesh) else { return false }
    return zip(lods, batch.lodMeshes).allSatisfy { prepared, mesh in
      prepared.signature == Self.signature(of: mesh)
    }
  }

  fileprivate init(_ batch: SceneBatch) {
    base = Self.prepare(batch.mesh)
    lods = batch.lodMeshes.map(Self.prepare)
  }

  private static func signature(of mesh: Mesh) -> SHA256.Digest {
    var hash = SHA256()
    mesh.vertices.withUnsafeBytes { hash.update(bufferPointer: $0) }
    mesh.indices.withUnsafeBytes { hash.update(bufferPointer: $0) }
    var error = mesh.geometricError.bitPattern
    withUnsafeBytes(of: &error) { hash.update(bufferPointer: $0) }
    return hash.finalize()
  }

  private static func prepare(_ mesh: Mesh) -> Level {
    let points = mesh.vertices.map { V3($0.position.x, $0.position.y, $0.position.z) }
    let lower = points.reduce(V3(repeating: Float.greatestFiniteMagnitude), simd_min)
    let upper = points.reduce(V3(repeating: -Float.greatestFiniteMagnitude), simd_max)
    let clusters = MeshletData(mesh)
    return Level(
      signature: signature(of: mesh), center: (lower + upper) / 2,
      radius: length(upper - lower) / 2 + 1,
      descriptors: zip(clusters.descriptors, clusters.spheres).map {
        MeshletDescriptor(ranges: $0.0, sphere: $0.1)
      }, meshletVertices: clusters.vertices, meshletTriangles: clusters.triangles,
      hasGroomCoverage: mesh.vertices.contains { $0.groom.z > 0 })
  }
}

package extension SceneBatch {
  /// Procedural grass uses a distinct descriptor preparation path and returns no mesh payload.
  @discardableResult mutating func prepareUpload() -> PreparedSceneUpload? {
    guard grass.isEmpty else { preparedUpload = nil; return nil }
    if preparedUpload == nil { preparedUpload = PreparedSceneUpload(self) }
    return preparedUpload
  }
}
