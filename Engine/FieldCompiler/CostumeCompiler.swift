import FieldCore
import simd

/// Surface-attached stitch geometry. Positions project onto actual compiled
/// triangles; their barycentric skin weights follow the host's deformation.
public enum CostumeCompiler {
  public struct Attachment {
    public var position:V3
    public var normal:V3
    public var skin:SkinWeight
    public var distance:Float
  }
  public static func attach(_ p:V3,to mesh:Mesh,weights:[SkinWeight])->Attachment {
    var result=Attachment(position:.zero,normal:V3(0,1,0),skin:SkinWeight(0),distance:.greatestFiniteMagnitude)
    for k in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{Int(mesh.indices[k+$0])},ps=ids.map{i->V3 in let v=mesh.vertices[i].position;return V3(v.x,v.y,v.z)}
      let bary=closestBarycentric(p,ps[0],ps[1],ps[2]),q=ps[0]*bary.x+ps[1]*bary.y+ps[2]*bary.z,d=length(p-q)
      if d<result.distance {
        let normals=ids.map{i->V3 in let n=mesh.vertices[i].normal;return V3(n.x,n.y,n.z)}
        let n=normals[0]*bary.x+normals[1]*bary.y+normals[2]*bary.z
        result=Attachment(position:q,normal:length_squared(n)>1e-12 ? normalize(n):V3(0,1,0),
          skin:weights.isEmpty ? SkinWeight(0):SkinFieldCompiler.mix([(weights[ids[0]],bary.x),(weights[ids[1]],bary.y),(weights[ids[2]],bary.z)]),distance:d)
      }
    };return result
  }
  /// Bounded closest point on a triangle, including degenerate edges.
  public static func closestBarycentric(_ p:V3,_ a:V3,_ b:V3,_ c:V3)->V3 {
    let ab=b-a,ac=c-a,n=cross(ab,ac),n2=length_squared(n)
    if n2>1e-16 {
      let q=p-n*(dot(p-a,n)/n2),v=dot(cross(q-a,ac),n)/n2,w=dot(cross(ab,q-a),n)/n2
      if v>=0 && w>=0 && v+w<=1 {return V3(1-v-w,v,w)}
    }
    var best=Float.greatestFiniteMagnitude,result=V3(1,0,0)
    let ps=[a,b,c]
    for i in 0..<3 {
      let j=(i+1)%3,d=ps[j]-ps[i],t=max(0,min(1,dot(p-ps[i],d)/max(1e-16,length_squared(d))))
      let error=length_squared(p-(ps[i]+d*t))
      if error<best {best=error;result = .zero;result[i]=1-t;result[j]=t}
    };return result
  }
  public static func compile(_ seams:[CostumeSeam],host:Mesh,weights:[SkinWeight]) throws -> (Mesh,[SkinWeight]) {
    var meshes=[host],skin=weights
    var stitchCount=0
    for seam in seams {
      for i in 1..<seam.points.count {
        let a=seam.points[i-1],b=seam.points[i],distance=length(b-a)
        let count=max(1,Int(ceil(distance/seam.spacing)))
        stitchCount += count
        try CraftError.require(stitchCount<=2048,"seam.\(seam.id)","More than 2048 stitches on a part")
        for k in 0..<count {
          let u=(Float(k)+0.12)/Float(count),v=(Float(k)+0.88)/Float(count)
          let left=attach(a+(b-a)*u,to:host,weights:weights),right=attach(a+(b-a)*v,to:host,weights:weights)
          try CraftError.require(max(left.distance,right.distance)<0.15,"seam.\(seam.id)","Path misses host surface by more than 15 cm")
          let mesh=ParametricMesh.tube(segments:3,sides:5,center:{t in
            let p=left.position+(right.position-left.position)*t,n=normalize(left.normal+(right.normal-left.normal)*t)
            return p+n*(seam.lift+sin(t*Float.pi)*seam.radius)
          },radius:{_ in seam.radius},color:seam.color)
          if !weights.isEmpty {
            for j in mesh.vertices.indices {let t=min(1,Float(j/6)/3);skin.append(SkinFieldCompiler.mix([(left.skin,1-t),(right.skin,t)]))}
          }
          meshes.append(mesh)
        }
      }
    }
    return(ParametricMesh.joined(meshes),skin)
  }
}
