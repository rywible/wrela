import FieldCore
import simd

/// Attribute seams may duplicate indices without opening the geometric surface.
/// This audit welds exact positions only; it does not merge nearby features or
/// certify vertex links, triangle intersections or collision equivalence.
enum SamplingGeometry {
  struct Audit {var orientedEdgeIncidence:Bool;var degenerateTriangles:Int;var inwardSamples:[V3];var badEdgeSamples:[V3]}
  static func removeZeroAreaTriangles(_ input:Mesh)->(mesh:Mesh,removed:Int) {
    var mesh=input,indices:[UInt32]=[],removed=0
    indices.reserveCapacity(input.indices.count)
    for i in stride(from:0,to:input.indices.count,by:3) {
      let ids=Array(input.indices[i..<i+3]),p=ids.map{j->SIMD3<Double> in
        let v=input.vertices[Int(j)].position;return SIMD3(Double(v.x),Double(v.y),Double(v.z))
      }
      if length_squared(cross(p[1]-p[0],p[2]-p[0]))==0 {removed+=1}
      else {indices+=ids}
    }
    mesh.indices=indices;return (mesh,removed)
  }
  static func audit(_ mesh:Mesh,field:Shape)->Audit {
    var positions:[V3:UInt32]=[:],remap:[UInt32]=[]
    for vertex in mesh.vertices {
      let p=V3(vertex.position.x,vertex.position.y,vertex.position.z)
      if let id=positions[p] {remap.append(id)} else {let id=UInt32(positions.count);positions[p]=id;remap.append(id)}
    }
    var edges:[UInt64:(count:Int,balance:Int)]=[:],degenerate=0,inward:[V3]=[]
    for i in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{Int(mesh.indices[i+$0])}
      let p=ids.map{j->V3 in let v=mesh.vertices[j].position;return V3(v.x,v.y,v.z)}
      let d=p.map{SIMD3<Double>(Double($0.x),Double($0.y),Double($0.z))}
      let area=cross(d[1]-d[0],d[2]-d[0])
      if !area.x.isFinite || !area.y.isFinite || !area.z.isFinite || length_squared(area)==0 {degenerate+=1}
      else {
        let center=(p[0]+p[1]+p[2])/3,n=field.normal(at:center)
        if dot(normalize(area),SIMD3<Double>(Double(n.x),Double(n.y),Double(n.z))) < -0.01 {inward.append(center)}
      }
      for j in 0..<3 {
        let a=remap[ids[j]],b=remap[ids[(j+1)%3]],key=UInt64(min(a,b))<<32|UInt64(max(a,b))
        let old=edges[key] ?? (0,0);edges[key]=(old.count+1,old.balance+(a<b ? 1:-1))
      }
    }
    var byID=[V3](repeating:.zero,count:positions.count)
    for (point,id) in positions {byID[Int(id)]=point}
    let bad=edges.keys.filter{let edge=edges[$0]!;return edge.count != 2 || edge.balance != 0}.sorted()
    let samples=bad.prefix(64).map{key in (byID[Int(key>>32)]+byID[Int(key & 0xffffffff)])/2}
    return Audit(orientedEdgeIncidence:!edges.isEmpty && bad.isEmpty,
      degenerateTriangles:degenerate,inwardSamples:inward,badEdgeSamples:samples)
  }
}
