import FieldCore
import simd

public enum SurfaceEditing {
  public static func apply(_ edits: [SurfaceEdit], to input: Mesh, requireCoverage: Bool = true) throws -> Mesh {
    var mesh=input
    for edit in edits {
      try edit.validate()
      var touched=false
      for i in mesh.vertices.indices {
        let v=mesh.vertices[i], p=V3(v.position.x,v.position.y,v.position.z)
        let sample=edit.evaluate(p)
        guard sample.weight > 0 else {continue}
        // Reject locally inverted/collapsed mappings at sampled vertices. This
        // does not certify triangle intersections or continuous injectivity.
        guard simd_determinant(sample.jacobian) > 0.05 else {throw SurfaceEditError.folded(edit.id)}
        let n=sample.jacobian.inverse.transpose*V3(v.normal.x,v.normal.y,v.normal.z)
        guard length_squared(n)>1e-12 else {throw SurfaceEditError.folded(edit.id)}
        mesh.vertices[i].position=SIMD4(sample.position,1)
        mesh.vertices[i].normal=SIMD4(normalize(n),v.normal.w)
        touched=true
      }
      guard touched || !requireCoverage else {throw SurfaceEditError.missed(edit.id)}
    }
    if !edits.isEmpty {mesh.report=nil}
    return mesh
  }
}
