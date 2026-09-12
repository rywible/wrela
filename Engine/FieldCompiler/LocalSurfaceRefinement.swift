import FieldCore
import simd

extension SculptCompiler {
  /// Edge splits propagate to both incident triangles. Red/green subdivisions
  /// preserve the original piecewise-linear surface and introduce no T junctions.
  /// All vertex channels and skin influences follow the same correspondence.
  public static func refineLocal(_ input:Mesh,weights:[SkinWeight],detail:SculptDetail) throws -> (Mesh,[SkinWeight]) {
    try detail.validate()
    try CraftError.require(weights.isEmpty || weights.count==input.vertices.count,"detail.skin","Vertex and skin counts differ")
    var mesh=input,skin=weights
    let originalCount=mesh.vertices.count
    func key(_ a:UInt32,_ b:UInt32)->UInt64 {UInt64(min(a,b))<<32|UInt64(max(a,b))}
    func point(_ i:UInt32)->V3 {let p=mesh.vertices[Int(i)].position;return V3(p.x,p.y,p.z)}
    for _ in 0..<16 {
      var split:Set<UInt64>=[]
      for i in stride(from:0,to:mesh.indices.count,by:3) {
        let p=(point(mesh.indices[i])-detail.center)/detail.radius,q=(point(mesh.indices[i+1])-detail.center)/detail.radius,r=(point(mesh.indices[i+2])-detail.center)/detail.radius
        let bary=CostumeCompiler.closestBarycentric(.zero,p,q,r)
        if length_squared(p*bary.x+q*bary.y+r*bary.z)>1 {continue}
        for (a,b) in [(mesh.indices[i],mesh.indices[i+1]),(mesh.indices[i+1],mesh.indices[i+2]),(mesh.indices[i+2],mesh.indices[i])] {
          let k=key(a,b)
          if split.contains(k) {continue}
          let p=point(a),q=point(b)
          if length_squared(q-p)>detail.edgeLength*detail.edgeLength*1.00001 {split.insert(k)}
        }
      }
      if split.isEmpty {return (mesh,skin)}
      let added=mesh.vertices.count-originalCount+split.count
      try CraftError.require(added<=detail.maximumNewVertices && mesh.vertices.count+split.count<=2_000_000,
        "detail.\(detail.id).budget","This local pass needs at least \(added) added vertices, above its budget. Increase edge length, narrow the patch, or explicitly raise its budget.")
      var newIndices:[UInt32]=[],midpoints:[UInt64:UInt32]=[:]
      newIndices.reserveCapacity(mesh.indices.count+split.count*6)
      func midpoint(_ a:UInt32,_ b:UInt32)->UInt32? {
        let k=key(a,b);guard split.contains(k) else {return nil}
        if let i=midpoints[k] {return i}
        let x=mesh.vertices[Int(a)],y=mesh.vertices[Int(b)],index=UInt32(mesh.vertices.count)
        var v=x;v.position=(x.position+y.position)*0.5;v.color=(x.color+y.color)*0.5;v.groom=(x.groom+y.groom)*0.5
        let n=V3(x.normal.x+y.normal.x,x.normal.y+y.normal.y,x.normal.z+y.normal.z)
        v.normal=SIMD4(length_squared(n)>1e-12 ? normalize(n):V3(x.normal.x,x.normal.y,x.normal.z),(x.normal.w+y.normal.w)*0.5)
        mesh.vertices.append(v)
        if !skin.isEmpty {skin.append(SkinFieldCompiler.mix([(skin[Int(a)],0.5),(skin[Int(b)],0.5)]))}
        midpoints[k]=index;return index
      }
      for i in stride(from:0,to:mesh.indices.count,by:3) {
        let a=mesh.indices[i],b=mesh.indices[i+1],c=mesh.indices[i+2],ab=midpoint(a,b),bc=midpoint(b,c),ca=midpoint(c,a)
        switch (ab,bc,ca) {
        case (nil,nil,nil):newIndices += [a,b,c]
        case (let ab?,nil,nil):newIndices += [a,ab,c,ab,b,c]
        case (nil,let bc?,nil):newIndices += [b,bc,a,bc,c,a]
        case (nil,nil,let ca?):newIndices += [c,ca,b,ca,a,b]
        case (let ab?,let bc?,nil):newIndices += [b,bc,ab,a,ab,c,ab,bc,c]
        case (let ab?,nil,let ca?):newIndices += [a,ab,ca,ab,b,c,ca,ab,c]
        case (nil,let bc?,let ca?):newIndices += [c,ca,bc,a,b,ca,b,bc,ca]
        case (let ab?,let bc?,let ca?):newIndices += [a,ab,ca,ab,b,bc,ca,bc,c,ab,bc,ca]
        }
      }
      mesh.indices=newIndices;mesh.report=nil
    }
    throw CraftError(path:"detail.\(detail.id).convergence",reason:"Refinement exceeded 16 levels; use a larger edge length or a smaller patch")
  }
}
