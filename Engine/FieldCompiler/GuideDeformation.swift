import FieldCore
import simd

public enum GuideDeformation {
  /// Reuse a guide's skin field for rest-shape edits; recompute surface normals
  /// from the resulting triangles. This changes source geometry, not pose state.
  public static func apply(_ input:Mesh,weights:[SkinWeight],joints:[String],offsets:[String:V3])->Mesh {
    guard !weights.isEmpty,joints.contains(where:{offsets[$0] != nil}) else {return input}
    precondition(weights.count==input.vertices.count)
    var mesh=input
    for i in mesh.vertices.indices {
      var delta=V3.zero
      for k in 0..<4 {
        let index=Int(weights[i].joints[k])
        if joints.indices.contains(index) {delta += (offsets[joints[index]] ?? .zero)*weights[i].weights[k]}
      }
      mesh.vertices[i].position += SIMD4(delta,0)
    }
    var normals=[V3](repeating:.zero,count:mesh.vertices.count)
    for i in stride(from:0,to:mesh.indices.count,by:3) {
      let a=Int(mesh.indices[i]),b=Int(mesh.indices[i+1]),c=Int(mesh.indices[i+2])
      func p(_ i:Int)->V3 {let v=mesh.vertices[i].position;return V3(v.x,v.y,v.z)}
      let normal=cross(p(b)-p(a),p(c)-p(a))
      normals[a]+=normal;normals[b]+=normal;normals[c]+=normal
    }
    for i in mesh.vertices.indices where length_squared(normals[i])>1e-16 {mesh.vertices[i].normal=SIMD4(normalize(normals[i]),mesh.vertices[i].normal.w)}
    mesh.report=nil
    return mesh
  }
}
