import FieldCore
import simd

public struct DeformationObservation: Codable, Sendable {
  public var vertices:Int
  public var triangles:Int
  public var maximumStretch:Float
  public var minimumStretch:Float
  public var minimumAreaRatio:Float
  public var reversedTriangles:Int
  public var worstTriangle:Int
  public var worstPosition:V3
  /// Counts alone can be dominated by extraction slivers. Report the bind
  /// surface area and the largest reversed patch's source/posed positions too.
  public var totalRestArea:Float = 0
  public var reversedRestArea:Float = 0
  public var worstReversedTriangle:Int = -1
  public var worstReversedRestPosition:V3 = .zero
  public var worstReversedPosition:V3 = .zero
}
public enum DeformationAudit {
  public static func corrected(_ input:Mesh,fields:[SurfaceEdit])->Mesh {
    var mesh=input
    for field in fields {
      for i in mesh.vertices.indices {
        let v=mesh.vertices[i],p=V3(v.position.x,v.position.y,v.position.z),s=field.evaluate(p)
        mesh.vertices[i].position=SIMD4(s.position,1)
        let n=V3(v.normal.x,v.normal.y,v.normal.z),j=s.jacobian
        let normal=cross(j[1],j[2])*n.x+cross(j[2],j[0])*n.y+cross(j[0],j[1])*n.z
        mesh.vertices[i].normal=SIMD4(length_squared(normal)>1e-16 ? normalize(normal):n,v.normal.w)
      }
    };return mesh
  }
  public static func deformed(_ input:Mesh,weights:[SkinWeight],palette:[simd_float4x4],rigid:simd_float4x4=matrix_identity_float4x4)->Mesh {
    var mesh=input
    let skinning=SkinningPalette(palette)
    for i in mesh.vertices.indices {
      let m=weights.isEmpty ? rigid:skinning.matrix(weights[i]),v=mesh.vertices[i]
      mesh.vertices[i].position=m*v.position
      let j=simd_float3x3(columns:(V3(m[0].x,m[0].y,m[0].z),V3(m[1].x,m[1].y,m[1].z),V3(m[2].x,m[2].y,m[2].z)))
      let n=V3(v.normal.x,v.normal.y,v.normal.z),normal=cross(j[1],j[2])*n.x+cross(j[2],j[0])*n.y+cross(j[0],j[1])*n.z
      mesh.vertices[i].normal=SIMD4(length_squared(normal)>1e-16 ? normalize(normal):n,v.normal.w)
    };return mesh
  }
  public static func inspect(rest:Mesh,posed:Mesh)->DeformationObservation {
    var out=DeformationObservation(vertices:rest.vertices.count,triangles:rest.indices.count/3,
      maximumStretch:1,minimumStretch:1,minimumAreaRatio:1,reversedTriangles:0,worstTriangle:-1,worstPosition:.zero)
    var largestReversedArea:Float=0
    func p(_ m:Mesh,_ i:Int)->V3 {let v=m.vertices[i].position;return V3(v.x,v.y,v.z)}
    for k in stride(from:0,to:rest.indices.count,by:3) {
      let ids=(0..<3).map{Int(rest.indices[k+$0])}
      let a=ids.map{p(rest,$0)},b=ids.map{p(posed,$0)}
      for j in 0..<3 {
        let d=length(a[(j+1)%3]-a[j]);if d<1e-8 {continue}
        let ratio=length(b[(j+1)%3]-b[j])/d
        out.minimumStretch=min(out.minimumStretch,ratio)
        if ratio>out.maximumStretch {out.maximumStretch=ratio;out.worstTriangle=k/3;out.worstPosition=b[j]}
      }
      let before=cross(a[1]-a[0],a[2]-a[0]),after=cross(b[1]-b[0],b[2]-b[0])
      if length(before)>1e-10 {
        let area=length(before)*0.5
        out.totalRestArea+=area
        out.minimumAreaRatio=min(out.minimumAreaRatio,length(after)/length(before))
        func sumNormal(_ m:Mesh)->V3 {ids.reduce(V3.zero){v,i in let n=m.vertices[i].normal;return v+V3(n.x,n.y,n.z)}}
        if dot(before,sumNormal(rest))*dot(after,sumNormal(posed))<0 {
          out.reversedTriangles+=1;out.reversedRestArea+=area
          if area>largestReversedArea {
            largestReversedArea=area;out.worstReversedTriangle=k/3
            out.worstReversedRestPosition=(a[0]+a[1]+a[2])/3
            out.worstReversedPosition=(b[0]+b[1]+b[2])/3
          }
        }
      }
    };return out
  }
}
