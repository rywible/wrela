import FieldCore
import FieldCompiler
import simd

/// An original funerary feline mask. Nasal wedge, orbit, cheek arch and muzzle
/// are one field surface. Compact sculpt fields hollow the cheek without slicing
/// a circular flat face into it; tessellation is only the representation.
extension ProcessionalRecipe {
  static func carvedFace(width:Float) throws -> (mask:Mesh,jaw:Mesh,eyes:Mesh) {
    func ell(_ p:V3,_ r:V3,_ rotation:V3 = .zero)->Shape {
      let q=simd_quatf(angle:rotation.z * .pi/180,axis:V3(0,0,1))
        * simd_quatf(angle:rotation.y * .pi/180,axis:V3(0,1,0))
        * simd_quatf(angle:rotation.x * .pi/180,axis:V3(1,0,0))
      return Shape.sphere(1).stretched(r).rotated(q).moved(p)
    }
    func between(_ a:V3,_ b:V3,_ breadth:Float,_ depth:Float)->Shape {
      let q=simd_quatf(from:V3(0,1,0),to:normalize(b-a))
      return Shape.sphere(1).stretched(V3(breadth,length(b-a)*0.62,depth))
        .rotated(q).moved((a+b)*0.5)
    }
    func mesh(_ p:V3,_ r:V3,_ color:V3,_ detail:Int=24)->Mesh {
      ParametricMesh.ellipsoid(p,r,color:color,detail:detail)
    }
    func curve(_ points:[V3],_ radius:Float,_ color:V3,
               _ sides:Int=10,_ taper:Float=0.98)->Mesh {
      let n=Float(points.count-1)
      return ParametricMesh.tube(segments:48,sides:sides,center:{t in
        let f=min(t*n,n-0.000001),i=Int(f),u=f-Float(i)
        let a=points[max(0,i-1)],b=points[i]
        let c=points[min(points.count-1,i+1)],d=points[min(points.count-1,i+2)]
        let linear:V3=(c-a)*u
        let quadratic:V3=(a*2-b*5+c*4-d)*(u*u)
        let cubic:V3=(-a+b*3-c*3+d)*(u*u*u)
        return (b*2+linear+quadratic+cubic)*0.5
      },radius:{t in max(0.0005,radius*pow(1-t*taper,0.82))},color:color)
    }
    func widened(_ input:Mesh)->Mesh {
      var output=input
      for i in output.vertices.indices {
        output.vertices[i].position.x *= width
        let n=output.vertices[i].normal
        output.vertices[i].normal=SIMD4(normalize(V3(n.x/width,n.y,n.z)),n.w)
      }
      return output
    }

    // Crown stays behind the face and supports the horn sockets. It is lower
    // than the old spherical forehead, with a distinct frontal shield.
    var skull=ell(V3(0,3.155,-1.655),V3(0.505,0.425,0.435))
      .blended(ell(V3(0,3.365,-1.845),V3(0.395,0.235,0.235)),radius:0.065)
      .blended(ell(V3(0,3.29,-2.015),V3(0.30,0.245,0.205)),radius:0.055)
    // Continuous tapering bridge. Paired whisker pads sit below it; the chin
    // belongs to the separate mandible, preserving the existing jaw pivot.
    skull=skull.blended(between(V3(0,3.35,-2.085),V3(0,3.025,-2.485),0.17,0.15),radius:0.043)
    skull=skull.blended(ell(V3(0,3.055,-2.425),V3(0.185,0.137,0.177)),radius:0.030)
    skull=skull.blended(ell(V3(0,2.875,-2.235),V3(0.325,0.13,0.275)),radius:0.035)
    for s:Float in [-1,1] {
      // Broad orbital shelf merges into brow and temple. Its inner corner
      // lowers toward the nose instead of opening a round eye.
      skull=skull.blended(ell(V3(s*0.315,3.205,-1.99),V3(0.27,0.25,0.225)),radius:0.053)
      skull=skull.blended(ell(V3(s*0.315,3.337,-2.055),V3(0.285,0.108,0.171),V3(0,0,s*13)),radius:0.055)
      skull=skull.blended(between(V3(s*0.12,3.305,-2.19),V3(s*0.48,3.405,-1.925),0.12,0.125),radius:0.036)
      // Cheekbone runs backward from outer eye and narrows into the hinge.
      // No planar subtraction intersects this outer silhouette.
      skull=skull.blended(ell(V3(s*0.47,3.095,-1.84),V3(0.155,0.276,0.24),V3(-9,s*12,s * -20)),radius:0.055)
      skull=skull.blended(between(V3(s*0.47,3.205,-2.04),V3(s*0.315,2.895,-2.265),0.115,0.12),radius:0.035)
      skull=skull.blended(ell(V3(s*0.186,2.962,-2.343),V3(0.218,0.154,0.258),V3(0,s*7,s*5)),radius:0.025)
      // Lower orbit thickness is part of the same surface, not a pasted tube.
      skull=skull.blended(between(V3(s*0.155,3.161,-2.176),V3(s*0.50,3.236,-2.037),0.063,0.075),radius:0.018)
    }
    for s:Float in [-1,1] {
      // Narrow oblique aperture with a sculpted upper rim.
      skull=skull.cut(ell(V3(s*0.338,3.225,-2.134),V3(0.204,0.060,0.174),V3(0,s * -9,s*13)))
      // Lip groove ends before the cheek arch.
      skull=skull.cut(ell(V3(s*0.238,2.829,-2.448),V3(0.163,0.018,0.148),V3(0,0,s*7)))
    }
    skull=skull.cut(ell(V3(0,2.699,-2.18),V3(0.355,0.150,0.389)))
    skull=skull.cut(ell(V3(0,2.912,-2.582),V3(0.015,0.064,0.075)))
    var shell=try Mesher.compile(skull,resolution:128,color:ivory,bounds:skull.occupiedBounds())
    var sculpt:[SurfaceEdit]=[]
    for s:Float in [-1,1] {
      sculpt.append(SurfaceEdit(id:"malar-hollow-\(s)",part:"mask",
        center:V3(s*0.52,2.996,-1.996),radius:V3(0.215,0.278,0.30),
        offset:V3(s * -0.047,0.002,0.037)))
      sculpt.append(SurfaceEdit(id:"temple-recession-\(s)",part:"mask",
        center:V3(s*0.44,3.315,-1.605),radius:V3(0.235,0.255,0.28),
        offset:V3(s * -0.036,-0.008,0.018)))
      sculpt.append(SurfaceEdit(id:"snout-plane-\(s)",part:"mask",
        center:V3(s*0.275,3.016,-2.479),radius:V3(0.205,0.154,0.19),
        offset:V3(s * -0.008,-0.011,0.006)))
    }
    shell=try SurfaceEditing.apply(sculpt,to:shell)
    // Broad patina remains quiet enough that the form must pass clay review.
    for i in shell.vertices.indices {
      let p=shell.vertices[i].position
      let lower:Float=max(0,min(1,(3.18-p.y)*1.35))
      let side:Float=max(0,min(1,(abs(p.x)-0.33)*2.7))
      let broad:Float=0.967+0.022*sin(p.x*5.3+p.y*3.1+p.z*4.2)
      shell.vertices[i].color=SIMD4(ivory*broad*(1-lower*0.09-side*0.055),1)
    }
    let cavity=V3(0.017,0.015,0.012),enamel=ivory*0.89
    var mask=[shell]
    mask.append(mesh(V3(0,2.747,-2.145),V3(0.305,0.080,0.255),cavity))
    // Broad tapered nose plate seated over the bridge end. Its lower half
    // narrows toward the philtrum instead of forming a protruding black button.
    let nose=ParametricMesh.surface(u:64,v:32,color:V3(0.101,0.078,0.049),position:{u,v in
      let a=u*2*Float.pi,b=(0.0001+v*0.9998)*Float.pi
      let y=cos(b),ring=sin(b),x=ring*cos(a)
      return V3(x*0.197*(0.73+0.27*y),3.035+y*0.073,-2.547+ring*sin(a)*0.068)
    },tint:{u,v in
      let grain:Float=0.97+0.025*sin(u*71)*sin(v*53)
      return V3(0.101,0.078,0.049)*grain
    })
    mask.append(nose)
    for s:Float in [-1,1] {
      mask.append(mesh(V3(s*0.112,3.033,-2.598),V3(0.037,0.019,0.015),cavity,24))
      // Recessed dark backing, with no pale scleral ring around the amber eye.
      mask.append(mesh(V3(s*0.338,3.225,-2.053),V3(0.185,0.054,0.044),cavity,28))
      mask.append(curve([V3(s*0.30,2.866,-2.404),V3(s*0.352,2.675,-2.453),
        V3(s*0.347,2.493,-2.522),V3(s*0.311,2.426+(s<0 ? 0.027:0),-2.554)],
        0.067,enamel,16,1))
      for i in 0..<3 {
        let f=Float(i),uneven:Float=s<0 ? 0.017:0
        mask.append(curve([V3(s*(0.241+f*0.037),2.966-f*0.025,-2.535),
          V3(s*(0.465+f*0.055),2.953-f*0.027,-2.624),
          V3(s*(0.725+f*0.084),2.991-f*0.065+uneven,-2.519)],0.0018,ivory*0.49,5,1))
      }
    }
    // A few short, partly buried incisors replace the regular picket of teeth.
    for (i,x) in [Float(-0.158),-0.06,0.048,0.146].enumerated() {
      let length:Float=[0.057,0.068,0.061,0.045][i]
      mask.append(curve([V3(x,2.824,-2.477),V3(x*1.015,2.824-length,-2.480)],
                        0.030,enamel*0.94,10,0.72))
    }

    // Continuous mandibular rami link substantial hinges to a smaller chin.
    var mandible=ell(V3(0,2.648,-2.293),V3(0.292,0.108,0.222))
    for s:Float in [-1,1] {
      mandible=mandible.blended(between(V3(s*0.235,2.660,-2.355),V3(s*0.36,2.786,-1.902),0.100,0.109),radius:0.032)
      mandible=mandible.blended(ell(V3(s*0.363,2.828,-1.813),V3(0.136,0.177,0.173),V3(-12,s*8,0)),radius:0.045)
    }
    mandible=mandible.cut(ell(V3(0,2.767,-2.174),V3(0.272,0.078,0.309)))
    let jawSurface=try Mesher.compile(mandible,resolution:96,color:ivory*0.79,bounds:mandible.occupiedBounds())
    var jaw=[jawSurface,
             mesh(V3(0,2.737,-2.18),V3(0.248,0.017,0.253),cavity)]
    for (i,x) in [Float(-0.152),-0.052,0.065,0.154].enumerated() {
      let length:Float=[0.045,0.057,0.050,0.037][i]
      jaw.append(curve([V3(x,2.707,-2.425),V3(x*0.96,2.707+length,-2.445)],
                       0.027,enamel*0.94,10,0.76))
    }
    var eyes:[Mesh]=[]
    for s:Float in [-1,1] {
      let center=V3(s*0.333,3.225,-2.111)
      eyes.append(mesh(center,V3(0.088,0.031,0.040),V3(0.115,0.063,0.018),28))
      eyes.append(mesh(center+V3(0,0,-0.035),V3(0.031,0.023,0.011),V3(0.43,0.225,0.045),28))
      eyes.append(mesh(center+V3(0,0,-0.045),V3(0.0065,0.021,0.005),cavity,20))
    }
    return (widened(ParametricMesh.joined(mask)),widened(ParametricMesh.joined(jaw)),
            widened(ParametricMesh.joined(eyes)))
  }
}
