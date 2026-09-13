import FieldCore
import simd

extension SculptCompiler {
  /// Repair only a local diagonal whose two triangles fail after projection.
  /// The replacement must pass orientation/area checks in BOTH representations,
  /// and face outward in the target field. No vertex/attribute positions move.
  /// Existing indexed seams and exterior topology are preserved.
  static func repairProjectedTriangles(_ input:Mesh,projected:Mesh,detail:SculptDetail,field:Shape)->(reference:Mesh,projected:Mesh,flips:Int) {
    struct Side {var triangle:Int;var a:UInt32;var b:UInt32;var c:UInt32}
    var indices=input.indices,total=0
    func key(_ a:UInt32,_ b:UInt32)->UInt64 {UInt64(min(a,b))<<32|UInt64(max(a,b))}
    func point(_ mesh:Mesh,_ id:UInt32)->V3 {let v=mesh.vertices[Int(id)].position;return V3(v.x,v.y,v.z)}
    func points(_ mesh:Mesh,_ ids:[UInt32])->[V3] {ids.map{point(mesh,$0)}}
    func area(_ p:[V3])->V3 {cross(p[1]-p[0],p[2]-p[0])}
    func valid(_ ids:[UInt32])->Bool {
      let a=points(input,ids),b=points(projected,ids),before=area(a),after=area(b)
      guard length_squared(before)>1e-16 && dot(before,after)>0 && length_squared(after)>length_squared(before)*0.0025 else {return false}
      let sample=field.sample(at:(b[0]+b[1]+b[2])/3),gradient=length(sample.gradient)
      return gradient>1e-6 && abs(sample.value)/gradient<=detail.edgeLength*0.5 && dot(normalize(after),sample.normal)>0.05
    }
    for _ in 0..<16 {
      var edges:[UInt64:[Side]]=[:],bad:Set<Int>=[]
      for t in stride(from:0,to:indices.count,by:3) {
        let ids=Array(indices[t..<t+3])
        guard ids.contains(where:{length_squared((point(input,$0)-detail.center)/detail.radius)<1}) else {continue}
        if !valid(ids) {bad.insert(t)}
        for j in 0..<3 {let a=ids[j],b=ids[(j+1)%3];edges[key(a,b),default:[]].append(Side(triangle:t,a:a,b:b,c:ids[(j+2)%3]))}
      }
      if bad.isEmpty {break}
      var touched:Set<Int>=[],newEdges:Set<UInt64>=[],flips=0
      for edge in edges.keys.sorted() {
        let sides=edges[edge]!
        guard sides.count==2 else {continue}
        let x=sides[0],y=sides[1],a=x.a,b=x.b,c=x.c,d=y.c
        guard (bad.contains(x.triangle) || bad.contains(y.triangle)) && y.a==b && y.b==a && c != d
          && edges[key(c,d)]==nil && !newEdges.contains(key(c,d))
          && !touched.contains(x.triangle) && !touched.contains(y.triangle)
          && [a,b,c,d].allSatisfy({length_squared((point(input,$0)-detail.center)/detail.radius)<0.9*0.9}) else {continue}
        let first:[UInt32]=[c,d,b],second:[UInt32]=[d,c,a]
        guard valid(first) && valid(second) else {continue}
        let oldNormal=area(points(input,[a,b,c]))+area(points(input,[b,a,d]))
        guard dot(area(points(input,first)),oldNormal)>1e-20 && dot(area(points(input,second)),oldNormal)>1e-20 else {continue}
        indices.replaceSubrange(x.triangle..<x.triangle+3,with:first)
        indices.replaceSubrange(y.triangle..<y.triangle+3,with:second)
        touched.insert(x.triangle);touched.insert(y.triangle);newEdges.insert(key(c,d));flips+=1
      }
      total+=flips;if flips==0 {break}
    }
    var reference=input,candidate=projected;reference.indices=indices;candidate.indices=indices
    return (reference,candidate,total)
  }
}
