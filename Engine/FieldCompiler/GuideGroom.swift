import FieldCore
import simd

/// A guide is authored source. Dense curved fibre meshes and their two-node
/// bindings are disposable representations, with deterministic clump variation.
public struct GroomGuide: Sendable {
  public var points:[V3]
  public var ids:[String]
  public init(points:[V3],ids:[String]) {self.points=points;self.ids=ids}
}
public enum GuideGroom {
  public static func point(_ points:[V3],_ t:Float)->V3 {
    let n=points.count-1,f=min(Float(n)*t,Float(n)-0.00001),i=Int(f),u=f-Float(i)
    let a=points[max(0,i-1)],b=points[i],c=points[min(n,i+1)],d=points[min(n,i+2)]
    let u2:Float=u*u,u3:Float=u*u*u
    let wa:Float=(-u+2*u2-u3)*0.5
    let wb:Float=(2-5*u2+3*u3)*0.5
    let wc:Float=(u+4*u2-3*u3)*0.5
    let wd:Float=(-u2+u3)*0.5
    let left:V3=a*wa+b*wb
    let right:V3=c*wc+d*wd
    return left+right
  }
  public static func compile(_ guides:[GroomGuide],fibres:Int = 64,width:Float = 0.07,
    radius:Float = 0.006,curl:Float = 0.025,color:V3)->(mesh:Mesh,weights:[SkinWeight],joints:[String]) {
    precondition(!guides.isEmpty && fibres>0 && guides.allSatisfy{$0.points.count>=2 && $0.points.count==$0.ids.count})
    var meshes:[Mesh]=[],weights:[SkinWeight]=[],joints:[String]=[]
    let segments=24,sides=4
    for (g,guide) in guides.enumerated() {
      let base=joints.count;joints+=guide.ids
      for strand in 0..<fibres {
        let seed=Float(strand+g*131),a=seed*2.3999632,r=width*sqrt((Float(strand)+0.5)/Float(fibres))
        let variation=0.88+0.12*sin(seed*12.73),phase=seed*4.13
        let lengthScale:Float=0.70+0.30*(0.5+0.5*sin(seed*7.17))
        let center:(Float)->V3 = {t in
          let p=point(guide.points,t*lengthScale)
          let tangent=normalize(point(guide.points,min(1,t+0.001)*lengthScale)-point(guide.points,max(0,t-0.001)*lengthScale))
          let normal=normalize(cross(tangent,abs(tangent.y)<0.9 ? V3(0,1,0):V3(1,0,0)))
          let side=cross(tangent,normal)
          let spread:Float=0.6+0.8*t
          let waveAmount:Float=curl*sin(t*18+phase)*sin(t*Float.pi)
          let radial:V3=(normal*cos(a)+side*sin(a))*(r*spread)
          let wave:V3=normal*waveAmount+side*(0.035*t*t*sin(phase))
          return p+radial+wave
        }
        let shade:Float=0.72+0.28*(0.5+0.5*sin(seed*8.17))
        var mesh=ParametricMesh.tube(segments:segments,sides:sides,center:center,radius:{t in max(0.00035,radius*variation*pow(1-t,0.7))},color:color*shade)
        for i in mesh.vertices.indices {
          let t=Float(i/(sides+1))/Float(segments),f=min(t*lengthScale*Float(guide.points.count-1),Float(guide.points.count-1)-0.00001),k=Int(f)
          mesh.vertices[i].groom=SIMD4(Float(i%(sides+1))/Float(sides),t*lengthScale,0,0)
          weights.append(SkinWeight(UInt32(base+k),UInt32(base+k+1),blend:f-Float(k)))
        }
        meshes.append(mesh)
      }
    }
    return (ParametricMesh.joined(meshes),weights,joints)
  }
}
