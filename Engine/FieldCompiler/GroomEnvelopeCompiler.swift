import FieldCore
import simd

/// A guide-following opaque clump represents unresolved interior fibres. Fine
/// curved fibres cover that envelope and break its silhouette. This is an
/// explicit polygonal approximation, not a volumetric hair transport model.
enum GroomEnvelopeCompiler {
  static func compile(_ design: GroomDesign, envelope: GroomEnvelope,
    rig: SecondaryRig) throws -> (Mesh, [SkinWeight], [String]) {
    let nodes = Dictionary(uniqueKeysWithValues: rig.nodes.map { ($0.id,$0.position) })
    try design.validate(nodes: Set(nodes.keys))
    try envelope.validate(id: design.id)
    var meshes: [Mesh] = [], weights: [SkinWeight] = [], ids: [String] = []
    func random(_ seed: UInt32) -> Float {
      var x = seed &+ design.seed &* 747796405
      x = (x ^ (x >> 16)) &* 2246822519; x = (x ^ (x >> 13)) &* 3266489917
      return Float((x ^ (x >> 16)) & 0xffffff) / Float(0x1000000)
    }
    for (g,guide) in design.guides.enumerated() {
      let points = guide.map { nodes[$0]! }, base = ids.count
      ids += guide
      try CraftError.require(zip(points,points.dropFirst()).allSatisfy { length($0.1-$0.0)>0.0001 },
        "groom.\(design.id)", "Coincident adjacent guide nodes")
      var tangents: [V3] = [], normals: [V3] = []
      for i in 0...96 {
        let t = Float(i)/96
        let d = GuideGroom.point(points,min(1,t+0.001))-GuideGroom.point(points,max(0,t-0.001))
        let tangent = length_squared(d)>1e-12 ? normalize(d) : (tangents.last ?? V3(0,-1,0))
        var normal = normals.last.map { simd_quatf(from:tangents.last!,to:tangent).act($0) }
          ?? normalize(cross(tangent,abs(tangent.y)<0.9 ? V3(0,1,0) : V3(1,0,0)))
        normal = normalize(normal-tangent*dot(normal,tangent))
        tangents.append(tangent); normals.append(normal)
      }
      func frame(_ t: Float) -> (V3,V3) {
        let f = min(95.999,max(0,t)*96), i = Int(f), u = f-Float(i)
        let tangent = normalize(tangents[i]*(1-u)+tangents[i+1]*u)
        let normal = normalize(normals[i]*(1-u)+normals[i+1]*u)
        return (normal,cross(tangent,normal))
      }
      func radius(_ t: Float) -> Float {
        let shoulder: Float = 0.84+0.26*sin(t*Float.pi)
        return max(0.00012,design.width*envelope.coverage*shoulder*pow(max(0,1-t),envelope.taper))
      }
      func ridge(_ t: Float,_ angle: Float) -> Float {
        let main = cos(angle*12+sin(t*3)*0.6)
        let detail = cos(angle*23-t*0.8)*0.30
        return 1+envelope.ridge*(main+detail)*sin((0.12+t*0.88)*Float.pi)
      }
      func shellPoint(_ t: Float,_ a: Float) -> V3 {
        let (normal,side) = frame(t)
        let crossSection = normal*cos(a)+side*(sin(a)*envelope.flatten)
        let wave = normal*(design.curl*sin(t*Float.pi)*sin(t*2*Float.pi*design.frequency+Float(g)*0.91))
        return GuideGroom.point(points,t)+crossSection*(radius(t)*ridge(t,a))+wave
      }
      func skin(_ t: Float) -> SkinWeight {
        let f = min(t*Float(guide.count-1),Float(guide.count-1)-0.00001), i = Int(f)
        return SkinWeight(UInt32(base+i),UInt32(base+i+1),blend:f-Float(i))
      }
      func clumpLength(_ c:Int)->Float {
        envelope.clumps==1 ? 1:0.69+0.31*random(UInt32(c+g*17)&+883)
      }
      func clumpPoint(_ t:Float,_ angle:Float,_ c:Int)->V3 {
        guard envelope.clumps>1 else {return shellPoint(t,angle)}
        let (normal,side)=frame(t)
        let phase=Float(c)*2.3999632+Float(g)*0.371
        let spread=sqrt((Float(c)+0.5)/Float(envelope.clumps))*0.78
        let turn=phase+0.16*sin(t*4+phase)*sin(t*Float.pi)
        let drift=(normal*cos(turn)+side*sin(turn)*envelope.flatten)*(radius(t)*spread)
        let center=GuideGroom.point(points,t)
        return center+drift+(shellPoint(t,angle)-center)*(0.86/sqrt(Float(envelope.clumps)))
      }
      let sides = envelope.clumps==1 ? 48:16, segments = envelope.clumps==1 ? 40:32
      let groupShade: Float = 0.88+0.09*random(UInt32(g)&+41)
      let strandCount = Float(max(48,design.fibres*3))
      let phase = Float((design.seed &+ UInt32(g)*131)%4093)
      let rootCoverage = min(0.98,max(0.80,Float(design.fibres)*design.radius*6/(Float.pi*design.width*envelope.coverage)))
      for clump in 0..<envelope.clumps {
       for layer in 0..<envelope.layers {
        let depth = Float(layer)/Float(max(1,envelope.layers-1))
        let radialScale: Float = 1-0.35*depth
        let lengthScale: Float = (1-0.20*depth)*clumpLength(clump)
        // Independent phase keeps gaps between neighbouring layers from lining
        // up. A shorter inner core supplies volume without an opaque outer wedge.
        let position:(Float,Float)->V3 = { a,t in
          let along=t*lengthScale,center=GuideGroom.point(points,along)
          return center+(clumpPoint(along,a*2*Float.pi,clump)-center)*radialScale
        }
        let tint:(Float,Float)->V3 = { a,t in
          let groove: Float = 0.988+0.012*cos(a*2*Float.pi*12+sin(t*3)*0.6)
          let gradient:V3 = design.rootColor+(design.tipColor-design.rootColor)*CraftMath.smooth(t*lengthScale)
          let lockShade:Float = envelope.clumps==1 ? 1:0.88+0.20*random(UInt32(clump+g*17)&+791)
          let shade:Float = groupShade*groove*(1-depth*0.12)*lockShade
          return gradient*shade
        }
        var shell = ParametricMesh.surface(u:sides,v:segments,position:position,tint:tint)
        for i in shell.vertices.indices {
          let u=Float(i%(sides+1))/Float(sides),t=Float(i/(sides+1))/Float(segments)
          let core=envelope.layers>1 && layer==envelope.layers-1
          shell.vertices[i].groom=SIMD4(phase+Float(layer)*37.381+Float(clump)*17.291+u*strandCount*radialScale/sqrt(Float(envelope.clumps)),
            t,core ? 0:min(0.99,rootCoverage+depth*0.28),max(0.12,design.lengthVariation))
        }
        weights += shell.vertices.indices.map { skin(Float($0/(sides+1))/Float(segments)*lengthScale) }
        meshes.append(shell)
       }
      }

      // Golden-angle placement keeps lower fibre counts evenly distributed.
      // Most fibres stay within a millimetric layer of the clump; only the
      // authored flyaway fraction wanders farther from the shared flow.
      let fibreSegments = 28, fibreSides = 4
      for strand in 0..<design.fibres {
        let seed = UInt32(strand+g*131)
        let a = Float(strand)*2.3999632+Float(g)*0.781
        let isFly = random(seed &+ 93)<design.flyaways
        let clump=strand%envelope.clumps
        let lengthScale: Float = (1-design.lengthVariation*random(seed &+ 781))*clumpLength(clump)
        let phase = random(seed &+ 411)*2*Float.pi
        let path: (Float)->V3 = { t in
          let u = t*lengthScale
          let twist: Float = design.clump*0.10*sin(t*Float.pi)*sin(a*3)
          let angle = a+twist
          let (normal,side) = frame(u)
          let outward = normalize(normal*cos(angle)+side*(sin(angle)/envelope.flatten))
          let lift: Float = design.radius*(0.6+0.6*random(seed &+ 19))
          let wander: Float = isFly ? design.width*0.18*sin(t*Float.pi)*sin(t*5+phase) : design.radius*0.7*sin(t*7+phase)
          return clumpPoint(u,angle,clump)+outward*(lift+wander)
        }
        var fibre = ParametricMesh.tube(segments:fibreSegments,sides:fibreSides,center:path,
          radius:{ t in max(0.00012,design.radius*(0.65+0.35*random(seed &+ 71))*pow(1-t,0.9)) },
          color:design.rootColor)
        let shade: Float = 0.94+0.06*random(seed &+ 51)
        for i in fibre.vertices.indices {
          let t = Float(i/(fibreSides+1))/Float(fibreSegments)
          fibre.vertices[i].color = SIMD4((design.rootColor+(design.tipColor-design.rootColor)*CraftMath.smooth(t))*shade,1)
          fibre.vertices[i].groom=SIMD4(Float(i%(fibreSides+1))/Float(fibreSides),t*lengthScale,0,0)
          weights.append(skin(t*lengthScale))
        }
        meshes.append(fibre)
      }
    }
    return (ParametricMesh.joined(meshes),weights,ids)
  }
}
