import FieldCompiler
import FieldCore
import simd

/// Vesper's cut and construction. These surfaces use exactly the same authored
/// parameterization as the cloth guide grid and attachment projection.
enum ProcessionalFinery {
  static func fabricPoint(_ u: Float,_ t: Float,_ width: Float) -> V3 {
    let a: Float = 2.02-u*4.04
    let z: Float = -1.15+t*3.45
    let w: Float = (0.92+0.17*sin(t*Float.pi))*width
    let drape = pow(abs(a)/2.02,3)
    let falling = pow(abs(a)/2.02,0.6)
    // Three broad folds carry the silhouette; smaller creases emerge toward
    // the weighted hem instead of corrugating the complete back uniformly.
    let broad: Float = sin(t*6*Float.pi+abs(a)*0.42)*0.092
    let crease: Float = sin(t*12*Float.pi-abs(a)*0.65)*0.026
    let fold = (broad+crease)*falling
    let sideWeight: Float = a>0 ? 1 : -0.65
    let hem: Float = 0.88+0.08*sin(t*Float.pi)+0.038*sin(t*5*Float.pi+0.3)
    let unevenHem = hem+sideWeight*0.035*sin(t*3*Float.pi+0.8)
    return V3(sin(a)*(w+fold),2.42-t*0.67+cos(a)*(0.55+fold)-drape*unevenHem,z)
  }

  static func mantle(width: Float,cloth: V3,gold: V3) -> Mesh {
    func position(_ u: Float,_ t: Float) -> V3 { fabricPoint(u,t,width) }
    let face = ParametricMesh.surface(u:88,v:128,position:position,tint:{ u,t in
      let edge = min(u,1-u)
      let acrossEnd = min(t,1-t)
      let bodyShade: Float = 0.84+0.055*cos(t*6*Float.pi)+0.028*cos(u*12*Float.pi+t*3)
      if edge<0.028 || acrossEnd<0.023 { return gold*0.73 }
      if edge<0.068 || acrossEnd<0.060 { return cloth*0.42 }
      if edge<0.073 || acrossEnd<0.065 { return gold*0.57 }
      return cloth*bodyShade
    })
    // The turned edge has real thickness and an inward return. All samples
    // still acquire skin bindings through the shared fabric parameterization.
    var meshes = [face]
    for side: Float in [0,1] {
      meshes.append(ParametricMesh.surface(u:3,v:128,position:{ q,t in
        let u = side+(side==0 ? 1 : -1)*q*0.020
        return ParametricMesh.offsetPoint(u:u,v:t,distance:-sin(q*Float.pi)*0.012-0.002,position:position)
      },tint:{ _,_ in gold*0.55 }))
    }
    return ParametricMesh.joined(meshes)
  }

  static func trim(width: Float,gold: V3) -> Mesh {
    func fabric(_ u: Float,_ t: Float) -> V3 { fabricPoint(u,t,width) }
    func point(_ u: Float,_ t: Float,_ lift: Float = 0.006) -> V3 {
      ParametricMesh.offsetPoint(u:u,v:t,distance:lift,position:fabric)
    }
    func line(_ path: @escaping (Float)->V3,_ radius: Float,_ color: V3,
      _ segments: Int = 48) -> Mesh {
      ParametricMesh.tube(segments:segments,sides:5,center:path,radius:{ _ in radius },color:color)
    }
    var meshes: [Mesh] = []
    // A paired couched cord follows each hem; each cord is smaller than the
    // broad woven band, leaving a readable large/medium/small hierarchy.
    for side: Float in [0,1] {
      let sign: Float = side==0 ? 1 : -1
      for d: Float in [0.013,0.052] {
        meshes.append(line({ t in point(side+sign*d,t) },0.005,gold*0.85,128))
      }
      // Sparse stitched bars belong to the border band, not the broad field.
      for i in 0..<38 {
        let t = (Float(i)+0.5)/38
        meshes.append(line({ q in point(side+sign*(0.034+q*0.008),t+(q-0.5)*0.005,0.007) },
          0.0022,gold*0.67,3))
      }
      // Weighted corner tassels, with a bound neck and four fine ends.
      for t: Float in [0.02,0.97] {
        let root = point(side+sign*0.014,t)
        let points = [root,root+V3(-sign*0.010,-0.05,0),root+V3(-sign*0.015,-0.15,0.018)]
        meshes.append(line({ q in GuideGroom.point(points,q) },0.014,gold*0.57,12))
        for strand in 0..<5 {
          let a = Float(strand)*2.39996
          let start = points[1]+V3(cos(a)*0.008,0,sin(a)*0.008)
          meshes.append(ParametricMesh.tube(segments:10,sides:4,
            center:{ q in start+V3(cos(a)*0.014*q,-0.12*q,0.023*q+sin(a)*0.01*q) },
            radius:{ q in 0.0035*(1-q)+0.0004 },color:gold*0.62))
        }
      }
    }
    // Construction seams separate three cut panels. The seam direction flows
    // across the shoulder and down each flank rather than forming round loops.
    for t: Float in [0.29,0.72] {
      meshes.append(line({ u in point(u,t,0.004) },0.0035,gold*0.49,80))
      for i in 0..<52 {
        let u = (Float(i)+0.5)/52
        meshes.append(line({ q in point(u+(q-0.5)*0.007,t+0.006,0.004) },0.0016,gold*0.54,3))
      }
    }
    for t: Float in [0.028,0.055,0.945,0.972] {
      meshes.append(line({ u in point(u,t) },0.0045,gold*0.73,88))
    }
    // One original split-leaf sigil per flank: a filled central blade, two
    // secondary leaves and a quiet stem. Its longitudinal axis follows drape.
    for side: Float in [0.18,0.82] {
      let sign: Float = side<0.5 ? 1 : -1
      let centerT: Float = 0.51
      meshes.append(ParametricMesh.surface(u:12,v:30,position:{ q,v in
        let centerU = side+sign*(v-0.5)*0.16
        let halfWidth: Float = 0.030*pow(sin(v*Float.pi),0.8)
        return point(centerU,centerT+(q*2-1)*halfWidth,0.007)
      },tint:{ q,v in gold*(0.59+0.13*q+0.09*sin(v*Float.pi)) }))
      meshes.append(line({ q in point(side+sign*(q-0.5)*0.20,centerT,0.012) },0.004,gold*0.83,30))
      for direction: Float in [-1,1] {
        meshes.append(ParametricMesh.surface(u:8,v:20,position:{ q,v in
          let u = side+sign*(0.018+v*0.070)
          let center = centerT+direction*(0.010+v*0.058)
          let halfWidth: Float = sin(v*Float.pi)*0.011
          return point(u,center+(q*2-1)*halfWidth,0.007)
        },tint:{ _,v in gold*(0.61+v*0.12) }))
      }
    }
    return ParametricMesh.joined(meshes)
  }
}
