import FieldCore
import FieldCompiler
import FieldEngine
import simd

extension ProcessionalRecipe {
  static func maneGuides(_ p:[String:Float])->[GroomGuide] {
    let width=p["maskWidth"] ?? 1,length=p["maneLength"] ?? 1
    return (0..<16).map {i in
      let a:Float = -0.25+Float(i)*(Float.pi+0.5)/15
      let c=cos(a),s=sin(a)
      let root=V3(c*0.7*width,3.12+s*0.65,-1.40)
      return GroomGuide(points:[root,root+V3(c*0.27,s*0.15,0.2),
        root+V3(c*0.40,-0.33,0.85*length),root+V3(c*0.46,-0.85,1.5*length)],
        ids:(0..<4).map{"mane-guide-\(i)-\($0)"})
    }
  }
  static func beardGuides(_ p:[String:Float])->[GroomGuide] {
    let length=p["maneLength"] ?? 1
    return (0..<4).map {i in
      let x=Float(i)*0.14-0.21,root=V3(x,2.63,-2.2)
      return GroomGuide(points:[root,root+V3(x*0.1,-0.3,-0.12),root+V3(x*0.4,-0.62*length,0),root+V3(x*0.7,-0.95*length,0.27)],ids:(0..<4).map{"beard-guide-\(i)-\($0)"})
    }
  }
  static func fabricPoint(_ u:Float,_ t:Float,_ width:Float)->V3 {
    ProcessionalFinery.fabricPoint(u,t,width)
  }
  static func secondaryRig(_ p:[String:Float])->SecondaryRig {
    var rig=SecondaryRig()
    for (guides,joint) in [(maneGuides(p),"mask"),(beardGuides(p),"jaw")] {
      for guide in guides {
        let base=rig.nodes.count
        for (i,point) in guide.points.enumerated() {
          rig.nodes.append(SecondaryNode(guide.ids[i],joint:joint,position:point,inverseMass:i==0 ? 0:40,radius:0.025,follow:1))
          if i>0 {rig.links.append(SecondaryLink(base+i-1,base+i,contact:true))}
          if i>1 {rig.links.append(SecondaryLink(base+i-2,base+i,compliance:0.004))}
        }
      }
    }
    let base=rig.nodes.count
    let columns=11,rows=5
    for j in 0..<rows {
      for i in 0..<columns {
        let u=Float(i)/Float(columns-1),t=Float(j)/Float(rows-1)
        rig.nodes.append(SecondaryNode("fabric-\(i)-\(j)",joint:j==0 ? "neck":j==rows-1 ? "haunch":"body",
          position:fabricPoint(u,t,p["mantleWidth"] ?? 1),inverseMass:i==columns/2 && (j==1 || j==rows-2) ? 0:20,radius:0.035,follow:0.3))
        let index=base+j*columns+i
        if i>0 {rig.links.append(SecondaryLink(index-1,index,contact:true))}
        if j>0 {rig.links.append(SecondaryLink(index-columns,index,contact:true))}
        if i>0 && j>0 {
          rig.links.append(SecondaryLink(index-columns-1,index,compliance:0.0001))
          rig.links.append(SecondaryLink(index-columns,index-1,compliance:0.0001))
          rig.patches.append(SecondaryPatch("fabric-patch-\(i-1)-\(j-1)",index-columns-1,index-columns,index-1,index))
        }
      }
    }
    return rig
  }
  static func clothWeights(_ mesh:Mesh,parameters:[String:Float])->[SkinWeight] {
    let width=parameters["mantleWidth"] ?? 1
    return mesh.vertices.map {v in
      let p=V3(v.position.x,v.position.y,v.position.z)
      let uv=SurfaceSkinning.project(p,position:{u,t in fabricPoint(u,t,width)})
      return SurfaceSkinning.weights(uv:uv,columns:11,rows:5)
    }
  }
}
