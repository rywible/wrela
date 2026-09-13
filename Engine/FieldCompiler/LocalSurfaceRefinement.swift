import FieldCore
import simd

extension SculptCompiler {
  /// Edge splits propagate to both incident triangles. Red/green subdivisions
  /// preserve the original piecewise-linear surface and introduce no T junctions.
  /// All vertex channels and skin influences follow the same correspondence.
  public static func refineLocal(_ input:Mesh,weights:[SkinWeight],detail:SculptDetail,field:Shape?=nil) throws -> (Mesh,[SkinWeight]) {
    do {return try refineLocalAttempt(input,weights:weights,detail:detail,field:field,reduceFirst:true)}
    catch let first as CraftError {
      guard detail.projection?.retriangulate == true && first.path == "detail.\(detail.id).projection" else {throw first}
      // Geometric simplification can improve the coarse mesh yet worsen its
      // projection. Retry the original topology with projection-aware flips.
      do {return try refineLocalAttempt(input,weights:weights,detail:detail,field:field,reduceFirst:false)}
      catch let second as CraftError {
        throw CraftError(path:second.path,reason:"Reduced topology failed: \(first.reason). Original-topology recovery failed: \(second.reason)")
      }
    }
  }
  private static func refineLocalAttempt(_ input:Mesh,weights:[SkinWeight],detail:SculptDetail,field:Shape?,reduceFirst:Bool) throws -> (Mesh,[SkinWeight]) {
    try detail.validate()
    try CraftError.require(detail.projection==nil || field != nil,"detail.projection.source","Field projection requires an authoritative anatomy surface")
    try field?.validate()
    try CraftError.require(weights.isEmpty || weights.count==input.vertices.count,"detail.skin","Vertex and skin counts differ")
    var mesh=input,skin=weights
    if reduceFirst,let policy=detail.projection,policy.retriangulate == true,let field {
      mesh=retriangulateLocal(mesh,weights:weights,detail:detail,field:field,maximumDistance:policy.maximumDistance)
      // Discard orphaned representation vertices with the identical remap for
      // skin. Unreferenced points must not affect later picking/bounds/audits.
      let used=Set(mesh.indices),vertices=mesh.vertices,oldSkin=skin
      var remap=Array(repeating:UInt32(0),count:vertices.count)
      mesh.vertices=[];skin=[]
      for i in vertices.indices where used.contains(UInt32(i)) {
        remap[i]=UInt32(mesh.vertices.count);mesh.vertices.append(vertices[i])
        if !oldSkin.isEmpty {skin.append(oldSkin[i])}
      }
      mesh.indices=mesh.indices.map{remap[Int($0)]}
    }
    let originalCount=mesh.vertices.count
    func attributeKey(_ a:UInt32,_ b:UInt32)->UInt64 {UInt64(min(a,b))<<32|UInt64(max(a,b))}
    // The compiler duplicates vertices across normal/material seams. Coordinate
    // geometric edge splits across those copies while keeping their attributes
    // separate. Exact positions only: nearby distinct surfaces are never welded.
    var positionIDs:[V3:UInt32]=[:],geometryIDs:[UInt32]=[]
    for v in mesh.vertices {
      let p=V3(v.position.x,v.position.y,v.position.z),id=positionIDs[p] ?? UInt32(positionIDs.count)
      positionIDs[p]=id;geometryIDs.append(id)
    }
    var nextGeometryID=UInt32(positionIDs.count)
    func key(_ a:UInt32,_ b:UInt32)->UInt64 {attributeKey(geometryIDs[Int(a)],geometryIDs[Int(b)])}
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
      if split.isEmpty {
        if let policy=detail.projection,let field {mesh=try projectLocal(mesh,field:field,detail:detail,policy:policy)}
        return (mesh,skin)
      }
      var attributeSplits:Set<UInt64>=[]
      for i in stride(from:0,to:mesh.indices.count,by:3) {
        let a=mesh.indices[i],b=mesh.indices[i+1],c=mesh.indices[i+2]
        for (x,y) in [(a,b),(b,c),(c,a)] where split.contains(key(x,y)) {attributeSplits.insert(attributeKey(x,y))}
      }
      let added=mesh.vertices.count-originalCount+attributeSplits.count
      try CraftError.require(added<=detail.maximumNewVertices && mesh.vertices.count+attributeSplits.count<=2_000_000,
        "detail.\(detail.id).budget","This local pass needs at least \(added) added vertices, above its budget. Increase edge length, narrow the patch, or explicitly raise its budget.")
      var newIndices:[UInt32]=[],midpoints:[UInt64:UInt32]=[:],geometricMidpoints:[UInt64:UInt32]=[:]
      newIndices.reserveCapacity(mesh.indices.count+split.count*6)
      func midpoint(_ a:UInt32,_ b:UInt32)->UInt32? {
        let k=key(a,b);guard split.contains(k) else {return nil}
        let attributes=attributeKey(a,b)
        if let i=midpoints[attributes] {return i}
        let x=mesh.vertices[Int(a)],y=mesh.vertices[Int(b)],index=UInt32(mesh.vertices.count)
        var v=x;v.position=(x.position+y.position)*0.5;v.color=(x.color+y.color)*0.5;v.groom=(x.groom+y.groom)*0.5
        let n=V3(x.normal.x+y.normal.x,x.normal.y+y.normal.y,x.normal.z+y.normal.z)
        v.normal=SIMD4(length_squared(n)>1e-12 ? normalize(n):V3(x.normal.x,x.normal.y,x.normal.z),(x.normal.w+y.normal.w)*0.5)
        mesh.vertices.append(v)
        if let id=geometricMidpoints[k] {geometryIDs.append(id)}
        else {geometryIDs.append(nextGeometryID);geometricMidpoints[k]=nextGeometryID;nextGeometryID+=1}
        if !skin.isEmpty {skin.append(SkinFieldCompiler.mix([(skin[Int(a)],0.5),(skin[Int(b)],0.5)]))}
        midpoints[attributes]=index;return index
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

  /// Project a bounded local representation back to its authoritative field.
  /// A C2 feather in the outer 30% avoids a seam at the patch boundary. The
  /// inner patch meets field tolerance; the feather deliberately interpolates.
  /// Attributes stay attached to their vertices; optional local diagonal repair
  /// changes topology. This cannot recover missing components.
  private static func projectLocal(_ input:Mesh,field:Shape,detail:SculptDetail,policy:SculptProjection) throws -> Mesh {
    var mesh=input,reference=input
    var projectionFlips=0
    for i in mesh.vertices.indices {
      let vertex=input.vertices[i],start=V3(vertex.position.x,vertex.position.y,vertex.position.z)
      let r=length((start-detail.center)/detail.radius),weight=1-CraftMath.smooth((r-0.7)/0.3)
      if weight<=0 {continue}
      var point=start
      for _ in 0..<24 {
        let sample=field.sample(at:point),g2=length_squared(sample.gradient)
        if abs(sample.value)<=policy.tolerance {break}
        try CraftError.require(g2>1e-12,"detail.\(detail.id).projection","Singular field gradient at vertex \(i); inspect the field construction")
        var step=sample.gradient*(sample.value/g2)
        let magnitude=length(step),limit=policy.maximumDistance*0.25
        if magnitude>limit {step *= limit/magnitude}
        let next=point-step
        try CraftError.require(length(next-start)<=policy.maximumDistance,"detail.\(detail.id).projection","Vertex \(i) exceeds the projection bound; inspect source correspondence or explicitly increase maximumDistance")
        point=next
      }
      try CraftError.require(abs(field.value(at:point))<=policy.tolerance,"detail.\(detail.id).projection","Vertex \(i) did not converge to the source field")
      let oldNormal=V3(vertex.normal.x,vertex.normal.y,vertex.normal.z),normal=field.normal(at:point)
      let blended=oldNormal+(normal-oldNormal)*weight
      try CraftError.require(length_squared(blended)>1e-12,"detail.\(detail.id).projection","Opposing normals at the patch boundary; inspect the field and surface")
      mesh.vertices[i].position=SIMD4(start+(point-start)*weight,1)
      mesh.vertices[i].normal=SIMD4(normalize(blended),vertex.normal.w)
    }
    if policy.retriangulate == true {
      let repaired=repairProjectedTriangles(input,projected:mesh,detail:detail,field:field)
      reference=repaired.reference;mesh=repaired.projected;projectionFlips=repaired.flips
    }
    for i in stride(from:0,to:mesh.indices.count,by:3) {
      func area(_ m:Mesh)->V3 {
        func point(_ j:Int)->V3 {let p=m.vertices[Int(m.indices[i+j])].position;return V3(p.x,p.y,p.z)}
        return cross(point(1)-point(0),point(2)-point(0))
      }
      let before=area(reference),after=area(mesh)
      let motionIDs=(0..<3).map{Int(mesh.indices[i+$0])}
      let starts=motionIDs.map{j->V3 in let v=reference.vertices[j].position;return V3(v.x,v.y,v.z)}
      let ends=motionIDs.map{j->V3 in let v=mesh.vertices[j].position;return V3(v.x,v.y,v.z)}
      if length_squared(before)>1e-16 && !TriangleMotion.preservesArea(from:starts,to:ends) {
        func center(_ m:Mesh)->V3 {
          let p=(m.vertices[Int(m.indices[i])].position+m.vertices[Int(m.indices[i+1])].position+m.vertices[Int(m.indices[i+2])].position)/3
          return V3(p.x,p.y,p.z)
        }
        let ids=(0..<3).map{Int(reference.indices[i+$0])},positions=ids.map{input.vertices[$0].position},projected=ids.map{mesh.vertices[$0].position}
        // A large change in face-normal direction is not by itself a proof of
        // an inverted surface. Diagnose the candidate against its actual field
        // before deciding whether extraction/retriangulation needs improvement.
        var rotatedFaces=0,inwardFaces=0,collapsedFaces=0,touchedFaces=0,worstField:Float=1
        for t in stride(from:0,to:mesh.indices.count,by:3) {
          let ids=(0..<3).map{Int(mesh.indices[t+$0])}
          if ids.allSatisfy({mesh.vertices[$0].position==input.vertices[$0].position}) {continue}
          touchedFaces+=1
          let old=ids.map{j->V3 in let p=input.vertices[j].position;return V3(p.x,p.y,p.z)}
          let new=ids.map{j->V3 in let p=mesh.vertices[j].position;return V3(p.x,p.y,p.z)}
          let a=cross(old[1]-old[0],old[2]-old[0]),b=cross(new[1]-new[0],new[2]-new[0])
          if dot(a,b)<=0 {rotatedFaces+=1}
          if length_squared(b)<=length_squared(a)*0.0025 {collapsedFaces+=1}
          if length_squared(b)>1e-20 {
            let alignment=dot(normalize(b),field.normal(at:(new[0]+new[1]+new[2])/3))
            worstField=min(worstField,alignment);if alignment < -0.01 {inwardFaces+=1}
          }
        }
        throw CraftError(path:"detail.\(detail.id).projection",reason:"Triangle motion loses its area bound at triangle \(i/3) at \(center(reference)); normal agreement \(dot(normalize(before),normalize(after))), area ratio \(length(after)/length(before)); field-normal alignment before \(dot(normalize(before),field.normal(at:center(reference)))), after \(dot(normalize(after),field.normal(at:center(mesh)))); original \(positions), projected \(projected). Candidate audit: \(projectionFlips) projection-aware flips, \(touchedFaces) changed faces, \(rotatedFaces) rotate past 90 degrees, \(collapsedFaces) collapse, \(inwardFaces) point into the field, worst alignment \(worstField). Inspect the source extraction or use a smoother field transition")
      }
    }
    for t in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{Int(mesh.indices[t+$0])}
      if ids.allSatisfy({mesh.vertices[$0].position==input.vertices[$0].position}) {continue}
      let p=ids.map{j->V3 in let v=mesh.vertices[j].position;return V3(v.x,v.y,v.z)},a=cross(p[1]-p[0],p[2]-p[0])
      if length_squared(a)>1e-20 {
        try CraftError.require(dot(normalize(a),field.normal(at:(p[0]+p[1]+p[2])/3)) >= -0.01,
          "detail.\(detail.id).projection","Projected triangle \(t/3) points into the source field after \(projectionFlips) local flips; rebuild this patch")
      }
    }
    mesh.report=nil;return mesh
  }
}
