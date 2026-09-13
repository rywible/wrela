import FieldCore
import simd
import CGeometry

extension SculptCompiler {
  /// Bounded edge flips improve local triangle quality before field projection.
  /// Positions and all vertex channels stay fixed. Only shared indexed edges
  /// can flip, so normal/material/skin seams are never welded or crossed.
  static func retriangulateLocal(_ input:Mesh,weights:[SkinWeight],detail:SculptDetail,field:Shape,maximumDistance:Float)->Mesh {
    struct Side {var triangle:Int;var a:UInt32;var b:UInt32;var opposite:UInt32}
    var mesh=input
    func key(_ a:UInt32,_ b:UInt32)->UInt64 {UInt64(min(a,b))<<32|UInt64(max(a,b))}
    func point(_ i:UInt32)->V3 {let p=mesh.vertices[Int(i)].position;return V3(p.x,p.y,p.z)}
    func area(_ a:V3,_ b:V3,_ c:V3)->V3 {cross(b-a,c-a)}
    func quality(_ a:V3,_ b:V3,_ c:V3)->Float {
      2*sqrt(Float(3))*length(area(a,b,c))/max(length_squared(a-b)+length_squared(b-c)+length_squared(c-a),1e-20)
    }
    // Remove locally redundant sliver edges within a fraction of the declared
    // projection/detail budget. The library returns original vertex indices;
    // buffers, skin and other attributes retain exact correspondence. Varying
    // skin/color/groom and all exterior vertices are conservatively locked.
    var locks=mesh.vertices.indices.map{i->UInt8 in length_squared((point(UInt32(i))-detail.center)/detail.radius)<0.9*0.9 ? 0:1}
    func sameSkin(_ a:Int,_ b:Int)->Bool {
      if weights.isEmpty {return true}
      let x=weights[a],y=weights[b]
      for i in 0..<4 where x.weights[i]>0.000001 {
        var other:Float=0;for j in 0..<4 where y.joints[j]==x.joints[i] {other+=y.weights[j]}
        if abs(other-x.weights[i])>0.00001 {return false}
      }
      for i in 0..<4 where y.weights[i]>0.000001 {
        var other:Float=0;for j in 0..<4 where x.joints[j]==y.joints[i] {other+=x.weights[j]}
        if abs(other-y.weights[i])>0.00001 {return false}
      }
      return true
    }
    for i in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{Int(mesh.indices[i+$0])}
      if (0..<3).contains(where:{j in
        let a=ids[j],b=ids[(j+1)%3],x=mesh.vertices[a],y=mesh.vertices[b]
        return !sameSkin(a,b) || length(x.color-y.color)>0.00001 || length(x.groom-y.groom)>0.00001
      }) {for id in ids {locks[id]=1}}
    }
    var indices=Array(repeating:UInt32(0),count:mesh.indices.count),error:Float=0
    let budget=min(maximumDistance*0.1,detail.edgeLength*0.25)
    let count=mesh.vertices.withUnsafeBytes {v in mesh.indices.withUnsafeBufferPointer {i in locks.withUnsafeBufferPointer {l in
      sgSimplifyLocal(&indices,i.baseAddress!,i.count,v.baseAddress!,mesh.vertices.count,l.baseAddress!,budget,&error,MemoryLayout<Vertex>.stride)
    }}}
    if count>0 && error<=budget*1.0001 {mesh.indices=Array(indices.prefix(count))}
    for _ in 0..<8 {
      var edges:[UInt64:[Side]]=[:]
      for i in stride(from:0,to:mesh.indices.count,by:3) {
        let ids=Array(mesh.indices[i..<i+3])
        for j in 0..<3 {
          let a=ids[j],b=ids[(j+1)%3]
          edges[key(a,b),default:[]].append(Side(triangle:i,a:a,b:b,opposite:ids[(j+2)%3]))
        }
      }
      var touched:Set<Int>=[],addedEdges:Set<UInt64>=[],flips=0
      for edge in edges.keys.sorted() {
        let sides=edges[edge]!
        guard sides.count==2 else {continue}
        let x=sides[0],y=sides[1],a=x.a,b=x.b,c=x.opposite,d=y.opposite
        guard y.a==b && y.b==a && c != d && edges[key(c,d)]==nil && !addedEdges.contains(key(c,d))
          && !touched.contains(x.triangle) && !touched.contains(y.triangle) else {continue}
        let pa=point(a),pb=point(b),pc=point(c),pd=point(d)
        guard [pa,pb,pc,pd].allSatisfy({length_squared(($0-detail.center)/detail.radius)<1}) else {continue}
        let old=min(quality(pa,pb,pc),quality(pb,pa,pd)),next=min(quality(pc,pd,pb),quality(pd,pc,pa))
        guard next>old*1.05+0.005 else {continue}
        let normal=area(pa,pb,pc)+area(pb,pa,pd),first=area(pc,pd,pb),second=area(pd,pc,pa)
        guard dot(first,normal)>1e-20 && dot(second,normal)>1e-20 else {continue}
        var nearField=true
        for center in [(pc+pd+pb)/3,(pd+pc+pa)/3] {
          let sample=field.sample(at:center),gradient=length(sample.gradient)
          if gradient<1e-6 || abs(sample.value)/gradient>maximumDistance {nearField=false;break}
        }
        guard nearField else {continue}
        mesh.indices.replaceSubrange(x.triangle..<x.triangle+3,with:[c,d,b])
        mesh.indices.replaceSubrange(y.triangle..<y.triangle+3,with:[d,c,a])
        touched.insert(x.triangle);touched.insert(y.triangle);addedEdges.insert(key(c,d));flips+=1
      }
      if flips==0 {break}
    }
    mesh.report=nil;return mesh
  }
}
