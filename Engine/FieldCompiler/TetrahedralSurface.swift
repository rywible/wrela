import FieldCore
import simd

/// Shared polygon extraction for uniform and conforming refined tetrahedra.
/// Vertex IDs identify volume edges; every incident cell reuses the same root.
struct TetrahedralSurface {
  let field:Shape
  let color:V3
  let refine:Bool
  private var mesh=Mesh()
  private var intersections:[UInt64:(linear:V3,root:V3)]=[:]
  private var vertexEdges:[UInt64]=[]
  private var linearNormals:[UInt64:V3]=[:]
  init(field:Shape,color:V3,refine:Bool) {self.field=field;self.color=color;self.refine=refine}
  mutating func append(ids:[Int],points ps:[V3],values ds:[Float]) {
    if ds.allSatisfy({$0>=0}) || ds.allSatisfy({$0<0}) {return}
    var hits:[(key:UInt64,point:V3)]=[]
    for (a,b) in [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)] where (ds[a]<0) != (ds[b]<0) {
      let ia=ids[a],ib=ids[b],key=UInt64(min(ia,ib))<<32|UInt64(max(ia,ib))
      if let p=intersections[key] {hits.append((key,p.linear));continue}
      let forward=ia<ib,pa=forward ? ps[a]:ps[b],pb=forward ? ps[b]:ps[a]
      let da=forward ? ds[a]:ds[b],db=forward ? ds[b]:ds[a]
      let linear=da==0 ? pa:(db==0 ? pb:pa+(pb-pa)*(da/(da-db)));var root=linear
      if da==0 {root=pa} else if db==0 {root=pb} else if refine {
        var lo=pa,hi=pb,dlo=da,dhi=db
        for _ in 0..<12 {
          if length(hi-lo)<=length(pb-pa)*0.00001 {break}
          let t=clamp(dlo/(dlo-dhi),0.1,0.9),q=lo+(hi-lo)*t,d=field.value(at:q)
          if (d<0)==(dlo<0) {lo=q;dlo=d} else {hi=q;dhi=d}
        }
        root=lo+(hi-lo)*clamp(dlo/(dlo-dhi),0,1)
        if abs(field.value(at:root))>abs(field.value(at:linear)) {root=linear}
      }
      intersections[key]=(linear,root);hits.append((key,linear))
    }
    guard hits.count>=3 else {return}
    let center=hits.reduce(V3.zero){$0+$1.point}/Float(hits.count)
    let basis=simd_float3x3(columns:(ps[1]-ps[0],ps[2]-ps[0],ps[3]-ps[0]))
    let gradient=basis.transpose.inverse*V3(ds[1]-ds[0],ds[2]-ds[0],ds[3]-ds[0])
    let normal=normalize(gradient),u=normalize(abs(normal.y)<0.9 ? cross(normal,V3(0,1,0)):cross(normal,V3(1,0,0))),v=cross(normal,u)
    hits.sort{atan2(dot($0.point-center,v),dot($0.point-center,u))<atan2(dot($1.point-center,v),dot($1.point-center,u))}
    for i in 1..<(hits.count-1) {
      let points=[hits[0],hits[i],hits[i+1]]
      let vertices=points.map{hit->Vertex in
        let n=linearNormals[hit.key] ?? field.normal(at:hit.point);linearNormals[hit.key]=n
        return Vertex(hit.point,n,color)
      }
      vertexEdges += points.map(\.key);mesh.triangle(vertices[0],vertices[1],vertices[2])
    }
  }
  mutating func finish()->Mesher.ReferenceRecovery {
    guard refine else {return Mesher.ReferenceRecovery(mesh:mesh,refinedVertices:0,heldVertices:intersections.count)}
    var held:Set<UInt64>=[]
    func point(_ vertex:Int)->V3 {
      let edge=vertexEdges[vertex],p=intersections[edge]!
      return held.contains(edge) ? p.linear:p.root
    }
    for _ in 0..<16 {
      var rejected:Set<UInt64>=[]
      for i in stride(from:0,to:mesh.vertices.count,by:3) {
        let p=(0..<3).map{intersections[vertexEdges[i+$0]]!.linear},q=(0..<3).map{point(i+$0)}
        let before=cross(p[1]-p[0],p[2]-p[0]),after=cross(q[1]-q[0],q[2]-q[0])
        if length_squared(before)>1e-20 && !(dot(before,after)>0 && length_squared(after)>length_squared(before)*0.0025) {rejected.formUnion(vertexEdges[i..<i+3])}
      }
      if rejected.isEmpty {break};let old=held.count;held.formUnion(rejected);if held.count==old {break}
    }
    for i in stride(from:0,to:mesh.vertices.count,by:3) {
      let p=(0..<3).map{intersections[vertexEdges[i+$0]]!.linear},q=(0..<3).map{point(i+$0)}
      let before=cross(p[1]-p[0],p[2]-p[0]),after=cross(q[1]-q[0],q[2]-q[0])
      if length_squared(before)>1e-20 && !(dot(before,after)>0 && length_squared(after)>length_squared(before)*0.0025) {
        return Mesher.ReferenceRecovery(mesh:mesh,refinedVertices:0,heldVertices:intersections.count)
      }
    }
    var finalNormals:[UInt64:V3]=[:]
    for i in mesh.vertices.indices {
      let p=point(i),edge=vertexEdges[i],normal=finalNormals[edge] ?? field.normal(at:p)
      finalNormals[edge]=normal;mesh.vertices[i].position=SIMD4(p,1);mesh.vertices[i].normal=SIMD4(normal,0)
    }
    return Mesher.ReferenceRecovery(mesh:mesh,refinedVertices:intersections.count-held.count,heldVertices:held.count)
  }
}
