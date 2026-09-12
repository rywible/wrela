import FieldCore
import simd

public enum CraftGroom {
  public static func compile(_ design:GroomDesign,rig:SecondaryRig) throws -> (Mesh,[SkinWeight],[String]) {
    if let envelope=design.envelope {
      return try GroomEnvelopeCompiler.compile(design,envelope:envelope,rig:rig)
    }
    let nodes=Dictionary(uniqueKeysWithValues:rig.nodes.map{($0.id,$0.position)})
    try design.validate(nodes:Set(nodes.keys))
    var meshes:[Mesh]=[],weights:[SkinWeight]=[],ids:[String]=[]
    let segments=24,sides=4
    func random(_ seed:UInt32)->Float {
      var x=seed &+ design.seed &* 747796405;x=(x^(x>>16)) &* 2246822519;x=(x^(x>>13)) &* 3266489917
      return Float((x^(x>>16))&0xffffff)/Float(0x1000000)
    }
    for (g,guide) in design.guides.enumerated() {
      let points=guide.map{nodes[$0]!},base=ids.count;ids += guide
      try CraftError.require(zip(points,points.dropFirst()).allSatisfy{length($0.1-$0.0)>0.0001},"groom.\(design.id)","Coincident adjacent guide nodes")
      // Parallel transported frames avoid the old world-up threshold flip.
      var frames:[V3]=[],tangents:[V3]=[]
      for i in 0...64 {
        let t=Float(i)/64,d=GuideGroom.point(points,min(1,t+0.001))-GuideGroom.point(points,max(0,t-0.001))
        let tangent=length_squared(d)>1e-12 ? normalize(d):(tangents.last ?? V3(0,-1,0))
        var frame=frames.last.map{simd_quatf(from:tangents.last!,to:tangent).act($0)}
          ?? normalize(cross(tangent,abs(tangent.y)<0.9 ? V3(0,1,0):V3(1,0,0)))
        frame=normalize(frame-tangent*dot(frame,tangent));frames.append(frame);tangents.append(tangent)
      }
      for strand in 0..<design.fibres {
        let seed=UInt32(strand+g*131),a=random(seed)*2*Float.pi,r=design.width*sqrt(random(seed &+ 191))
        let isFly=random(seed &+ 93)<design.flyaways
        let lengthScale=1-design.lengthVariation*random(seed &+ 781)
        let phase=random(seed &+ 411)*2*Float.pi,group=Float(strand%7)*2*Float.pi/7
        let path:(Float)->V3={t in
          let u=t*lengthScale,f=min(63.999,u*64),i=Int(f),v=f-Float(i)
          let tangent=normalize(tangents[i]*(1-v)+tangents[i+1]*v)
          let normal=normalize(frames[i]*(1-v)+frames[i+1]*v),side=cross(tangent,normal)
          let clump:Float=design.clump*CraftMath.smooth(t)*(isFly ? 0.15:1)
          let spread:V3=(normal*cos(a)+side*sin(a))*r*(1-clump)
          let groupOffset:V3=(normal*cos(group)+side*sin(group))*design.width*0.23*sin(t*Float.pi)*clump
          let curl:Float=design.curl*sin(t*Float.pi)*(isFly ? 1.8:1)
          let angle:Float=t*2*Float.pi*design.frequency+phase
          let wave:V3=(normal*cos(angle)+side*sin(angle))*curl
          let position:V3=GuideGroom.point(points,u)+spread
          return position+groupOffset+wave
        }
        var mesh=ParametricMesh.tube(segments:segments,sides:sides,center:path,radius:{t in
          max(0.00015,design.radius*(0.7+0.3*random(seed &+ 71))*pow(1-t,0.8))
        },color:design.rootColor)
        let shade:Float=0.8+0.2*random(seed &+ 19)
        for i in mesh.vertices.indices {
          let t=min(1,Float(i/(sides+1))/Float(segments))
          mesh.vertices[i].color=SIMD4((design.rootColor+(design.tipColor-design.rootColor)*CraftMath.smooth(t))*shade,1)
          mesh.vertices[i].groom=SIMD4(Float(i%(sides+1))/Float(sides),t*lengthScale,0,0)
          let f=min(t*lengthScale*Float(guide.count-1),Float(guide.count-1)-0.00001),k=Int(f)
          weights.append(SkinWeight(UInt32(base+k),UInt32(base+k+1),blend:f-Float(k)))
        }
        meshes.append(mesh)
      }
    }
    return(ParametricMesh.joined(meshes),weights,ids)
  }
}
