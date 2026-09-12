import FieldCore
import simd

/// Exact triangle selection on the rendered representation. A hit retains
/// barycentric correspondence into bind source; no nearest-vertex snapping.
public enum SurfacePicking {
  public struct Hit:Sendable {
    public var triangle:Int
    public var barycentric:V3
    public var distance:Float
    public var position:V3
    public var normal:V3
    public var bindPosition:V3
    public var bindNormal:V3
    public var edgeMetres:Float
  }
  public static func ray(pixel:SIMD2<Float>,size:SIMD2<Float>,inverseVP:simd_float4x4) throws -> (origin:V3,direction:V3) {
    try CraftError.require(pixel.x.isFinite && pixel.y.isFinite && size.x>0 && size.y>0
      && pixel.x>=0 && pixel.y>=0 && pixel.x<size.x && pixel.y<size.y,"pick.pixel","Use pixel centres inside the captured image")
    let x=2*(pixel.x+0.5)/size.x-1,y=1-2*(pixel.y+0.5)/size.y
    let a=inverseVP*SIMD4(x,y,0,1),b=inverseVP*SIMD4(x,y,1,1)
    try CraftError.require(abs(a.w)>1e-9 && abs(b.w)>1e-9,"pick.camera","Singular projection")
    let p=V3(a.x,a.y,a.z)/a.w,q=V3(b.x,b.y,b.z)/b.w
    return (p,normalize(q-p))
  }
  public static func hit(rest:Mesh,posed:Mesh,origin:V3,direction:V3)->Hit? {
    guard rest.vertices.count==posed.vertices.count,rest.indices==posed.indices else {return nil}
    var result:Hit?
    func p(_ m:Mesh,_ i:Int)->V3 {let v=m.vertices[i].position;return V3(v.x,v.y,v.z)}
    func n(_ m:Mesh,_ i:Int)->V3 {let v=m.vertices[i].normal;return V3(v.x,v.y,v.z)}
    for k in stride(from:0,to:posed.indices.count,by:3) {
      let ids=(0..<3).map{Int(posed.indices[k+$0])}
      let a=p(posed,ids[0]),b=p(posed,ids[1]),c=p(posed,ids[2])
      let e=b-a,f=c-a,h=cross(direction,f),det=dot(e,h)
      if abs(det)<1e-10 {continue}
      let inv=1/det,s=origin-a,u=inv*dot(s,h)
      if u < -1e-6 || u > 1.000001 {continue}
      let q=cross(s,e),v=inv*dot(direction,q)
      if v < -1e-6 || u+v>1.000001 {continue}
      let t=inv*dot(f,q)
      if t<0 || t >= (result?.distance ?? .greatestFiniteMagnitude) {continue}
      let bary=V3(1-u-v,u,v)
      let bind=(0..<3).reduce(V3.zero){$0+p(rest,ids[$1])*bary[$1]}
      let normal=(0..<3).reduce(V3.zero){$0+n(posed,ids[$1])*bary[$1]}
      let bindNormal=(0..<3).reduce(V3.zero){$0+n(rest,ids[$1])*bary[$1]}
      result=Hit(triangle:k/3,barycentric:bary,distance:t,position:origin+direction*t,
        normal:normalize(normal),bindPosition:bind,bindNormal:normalize(bindNormal),
        edgeMetres:max(length(b-a),max(length(c-b),length(a-c))))
    }
    return result
  }
}
