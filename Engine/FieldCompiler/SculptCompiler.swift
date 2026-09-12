import FieldCore
import simd

public enum SculptCompiler {
  /// Conforming midpoint subdivision. Shared indexed edges get one new vertex;
  /// seams remain seams. Skin data is interpolated and deterministically reduced.
  public static func refine(_ input:Mesh,weights:[SkinWeight],levels:Int) throws -> (Mesh,[SkinWeight]) {
    try CraftError.require((0...2).contains(levels),"refinement.levels","Use 0…2 levels")
    var mesh=input,skin=weights
    try CraftError.require(skin.isEmpty || skin.count==mesh.vertices.count,"refinement.skin","Vertex/weight counts differ")
    for _ in 0..<levels {
      try CraftError.require(mesh.indices.count<=750_000 && mesh.vertices.count<=250_000,"refinement.budget","Candidate exceeds bounded refinement budget")
      var edges:[UInt64:UInt32]=[:],indices:[UInt32]=[]
      func midpoint(_ a:UInt32,_ b:UInt32)->UInt32 {
        let key=UInt64(min(a,b))<<32|UInt64(max(a,b))
        if let i=edges[key] {return i}
        let x=mesh.vertices[Int(a)],y=mesh.vertices[Int(b)],index=UInt32(mesh.vertices.count)
        var v=x;v.position=(x.position+y.position)*0.5;v.color=(x.color+y.color)*0.5
        v.groom=(x.groom+y.groom)*0.5
        let n=V3(x.normal.x+y.normal.x,x.normal.y+y.normal.y,x.normal.z+y.normal.z)
        v.normal=SIMD4(length_squared(n)>1e-12 ? normalize(n):V3(x.normal.x,x.normal.y,x.normal.z),(x.normal.w+y.normal.w)*0.5)
        mesh.vertices.append(v)
        if !skin.isEmpty {skin.append(SkinFieldCompiler.mix([(skin[Int(a)],0.5),(skin[Int(b)],0.5)]))}
        edges[key]=index;return index
      }
      for i in stride(from:0,to:mesh.indices.count,by:3) {
        let a=mesh.indices[i],b=mesh.indices[i+1],c=mesh.indices[i+2]
        let ab=midpoint(a,b),bc=midpoint(b,c),ca=midpoint(c,a)
        indices += [a,ab,ca,ab,b,bc,ca,bc,c,ab,bc,ca]
      }
      mesh.indices=indices;mesh.report=nil
    }
    return (mesh,skin)
  }
  public static func apply(_ strokes:[SculptStroke],to input:Mesh) throws -> Mesh {
    var mesh=input
    for stroke in strokes {
      try stroke.validate()
      let before=mesh
      var touched=0
      var neighbors:[Set<Int>]=[]
      if stroke.brush == .smooth {
        neighbors=Array(repeating:[],count:mesh.vertices.count)
        for k in stride(from:0,to:mesh.indices.count,by:3) {
          let a=Int(mesh.indices[k]),b=Int(mesh.indices[k+1]),c=Int(mesh.indices[k+2])
          neighbors[a].formUnion([b,c]);neighbors[b].formUnion([a,c]);neighbors[c].formUnion([a,b])
        }
      }
      for i in mesh.vertices.indices {
        let v=before.vertices[i],p=V3(v.position.x,v.position.y,v.position.z),f=stroke.field(p)
        guard f.weight>1e-7 else {continue};touched+=1
        if stroke.brush == .smooth {
          let ns=neighbors[i].sorted()
          if !ns.isEmpty {
            let average=ns.reduce(V3.zero){sum,j in let q=before.vertices[j].position;return sum+V3(q.x,q.y,q.z)}/Float(ns.count)
            mesh.vertices[i].position=SIMD4(p+(average-p)*(stroke.strength*f.weight),1)
          }
        } else {
          try CraftError.require(simd_determinant(f.jacobian)>0.05,"stroke.\(stroke.id)","Sampled fold at vertex \(i); reduce strength or enlarge radius")
          let n=f.jacobian.inverse.transpose*V3(v.normal.x,v.normal.y,v.normal.z)
          mesh.vertices[i].position=SIMD4(f.position,1);mesh.vertices[i].normal=SIMD4(normalize(n),v.normal.w)
        }
      }
      try CraftError.require(touched>0,"stroke.\(stroke.id)","No surface coverage; probe the part or refine before sculpting")
      // Reject collapsed or flipped sampled triangles, including smoothing edits.
      for k in stride(from:0,to:mesh.indices.count,by:3) {
        func area(_ m:Mesh)->V3 {
          func p(_ o:Int)->V3 {let v=m.vertices[Int(m.indices[k+o])].position;return V3(v.x,v.y,v.z)}
          return cross(p(1)-p(0),p(2)-p(0))
        }
        let a=area(before),b=area(mesh)
        if length_squared(a)>1e-16 {
          try CraftError.require(dot(a,b)>0 && length_squared(b)>length_squared(a)*0.0025,"stroke.\(stroke.id)","Collapsed/reversed triangle \(k/3)")
        }
      }
      if stroke.brush == .smooth {recomputeNormals(&mesh)}
      mesh.report=nil
    }
    return mesh
  }
  public static func recomputeNormals(_ mesh:inout Mesh) {
    var sums=Array(repeating:V3.zero,count:mesh.vertices.count)
    for k in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{Int(mesh.indices[k+$0])}
      let p=ids.map{i->V3 in let v=mesh.vertices[i].position;return V3(v.x,v.y,v.z)}
      let n=cross(p[1]-p[0],p[2]-p[0]);for i in ids {sums[i]+=n}
    }
    for i in sums.indices where length_squared(sums[i])>1e-16 {mesh.vertices[i].normal=SIMD4(normalize(sums[i]),mesh.vertices[i].normal.w)}
  }
}
