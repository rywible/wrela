import FieldCore
import simd

public struct SkinContactResult: Codable, Sendable {
  public var vertices = 0
  public var penetratingVertices = 0
  public var maximumPenetration:Float = 0
  public var vertex = -1
  public var position = V3.zero
  public var collider = ""
  public init() {}
}
/// Checks dense compiled vertices after the renderer's four-weight deformation.
/// This does not test triangle interiors, shader wind, self-contact or swept time.
public enum SkinContactAudit {
  public static func evaluate(mesh:Mesh,weights:[SkinWeight],palette:[simd_float4x4],
    model:simd_float4x4,bodies:[RigCapsule],height:(Float,Float)->Float,tolerance:Float = 0.0005)->SkinContactResult {
    precondition(tolerance.isFinite && tolerance>=0)
    precondition(palette.isEmpty || (weights.count==mesh.vertices.count && weights.allSatisfy{$0.validate(count:palette.count)}))
    var result=SkinContactResult();result.vertices=mesh.vertices.count
    let skinning=SkinningPalette(palette)
    for (i,v) in mesh.vertices.enumerated() {
      let skinned=palette.isEmpty ? v.position:skinning.matrix(weights[i])*v.position
      let world=model*skinned,p=V3(world.x,world.y,world.z)
      var depth=max(0,height(p.x,p.z)-p.y),body=depth>0 ? "ground":""
      for capsule in bodies {
        let low=simd_min(capsule.a,capsule.b)-V3(repeating:capsule.radius)
        let high=simd_max(capsule.a,capsule.b)+V3(repeating:capsule.radius)
        if (0..<3).contains(where:{p[$0]<low[$0] || p[$0]>high[$0]}) {continue}
        let d=capsule.radius-RigCollision.segmentDistance(p,p,capsule.a,capsule.b)
        if d>depth {depth=d;body=capsule.id}
      }
      if depth>tolerance {result.penetratingVertices+=1}
      if depth>result.maximumPenetration {result.maximumPenetration=depth;result.vertex=i;result.position=p;result.collider=body}
    }
    return result
  }
}
