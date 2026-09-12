import FieldCore
import simd

public struct SculptTarget:Codable,Equatable,Sendable {
  public var id:String
  public var point:V3
  public var target:V3
  public var tolerance:Float
  public init(id:String,point:V3,target:V3,tolerance:Float=0.001) {self.id=id;self.point=point;self.target=target;self.tolerance=tolerance}
}
public struct SculptControl:Codable,Equatable,Sendable {
  public var center:V3
  public var radius:Float
  public init(center:V3,radius:Float) {self.center=center;self.radius=radius}
}
public struct SculptResidual:Codable,Sendable {
  public var id:String
  public var actual:V3
  public var target:V3
  public var error:Float
  public var tolerance:Float
}
/// Solve a small authoring intent over the actual selected triangles. Targets
/// may request motion or preservation. Unknowns are compact source fields,
/// never vertex deltas. The nonlinear iteration accounts for ordered strokes.
public enum SurfaceIntentFit {
  public struct Result:Sendable {public var layer:SculptLayer;public var residuals:[SculptResidual];public var iterations:Int;public var maximumDisplacement:Float;public var protectedVertices:Int;public var maximumProtectedDisplacement:Float;public var maximumNormalChangeDegrees:Float}
  public static func fit(id:String,part:String,mesh:Mesh,controls:[SculptControl],targets:[SculptTarget],
    maximumDisplacement:Float=0.15,iterations:Int=24,protect:[SculptProtection]=[],maximumNormalChangeDegrees:Float=180) throws -> Result {
    try CraftError.require((1...12).contains(controls.count) && (1...64).contains(targets.count)
      && Set(targets.map(\.id)).count==targets.count && (1...64).contains(iterations)
      && maximumDisplacement.isFinite && (0.0001...0.5).contains(maximumDisplacement),"sculptFit.budget","Use 1…12 controls, 1…64 unique targets, displacement 0.1…500 mm and 1…64 iterations")
    try CraftError.require(maximumNormalChangeDegrees.isFinite && (0.1...180).contains(maximumNormalChangeDegrees),"sculptFit.normalLimit","Use a maximum normal change of 0.1…180 degrees")
    for c in controls {try CraftError.require(CraftMath.finite(c.center,limit:100) && c.radius.isFinite && (0.02...3).contains(c.radius),"sculptFit.control","Use bind metres and support radius 0.02…3 m")}
    try CraftError.require(protect.count<=32 && Set(protect.map(\.id)).count==protect.count,"sculptFit.protect","Use at most 32 uniquely named protection volumes")
    for mask in protect {try mask.validate()}
    for t in targets {try CraftError.require(!t.id.isEmpty && CraftMath.finite(t.point,limit:100) && CraftMath.finite(t.target,limit:100)
      && t.tolerance.isFinite && (0.00001...0.05).contains(t.tolerance) && length(t.target-t.point)<=maximumDisplacement,"sculptFit.target","Use finite targets, tolerance 0.01…50 mm and bounded requested displacement")}
    struct Sample {var points:[V3];var bary:V3}
    let samples:[Sample]=try targets.map{target in
      var best:Float = .greatestFiniteMagnitude,result:Sample?
      for k in stride(from:0,to:mesh.indices.count,by:3) {
        let p=(0..<3).map{i->V3 in let v=mesh.vertices[Int(mesh.indices[k+i])].position;return V3(v.x,v.y,v.z)}
        let b=CostumeCompiler.closestBarycentric(target.point,p[0],p[1],p[2]),q=p[0]*b.x+p[1]*b.y+p[2]*b.z,d=length_squared(q-target.point)
        if d<best {best=d;result=Sample(points:p,bary:b)}
      }
      guard let result,sqrt(best)<0.003 else {throw CraftError(path:"sculptFit.\(target.id)",reason:"Point is not on the current compiled surface (\(sqrt(best)) m); select from a fresh render")}
      return result
    }
    let n=controls.count*3
    func strokes(_ x:[Float])->[SculptStroke] {
      controls.enumerated().map{i,c in
        let v=V3(x[i*3],x[i*3+1],x[i*3+2]),m=length(v)
        var stroke=SculptStroke(id:id+"-\(i)",part:part,brush:.grab,points:[c.center],radius:c.radius,
          strength:m,direction:m>1e-8 ? v/m:V3(0,1,0))
        if !protect.isEmpty {
          let n=normalize(stroke.direction),t=abs(n.x)<0.8 ? V3(1,0,0):V3(0,1,0)
          stroke.profile=SculptProfile(tangent:t,radii:V3(repeating:c.radius),protect:protect)
        }
        return stroke
      }
    }
    func evaluate(_ x:[Float])->[V3] {
      let fields=strokes(x)
      return samples.map{s in
        var sum=V3.zero
        for i in 0..<3 {var p=s.points[i];for field in fields {p += field.displacement(p)};sum += p*s.bary[i]}
        return sum
      }
    }
    func objective(_ values:[V3])->Float {targets.indices.reduce(0){$0+length_squared((values[$1]-targets[$1].target)/targets[$1].tolerance)}}
    var x=Array(repeating:Float(0),count:n),values=evaluate(Array(repeating:0,count:n)),used=0
    for iteration in 0..<iterations {
      if targets.indices.allSatisfy({length(values[$0]-targets[$0].target)<=targets[$0].tolerance}) {break}
      used=iteration+1
      let e:Float=0.0001
      var jac=Array(repeating:Array(repeating:Float(0),count:n),count:targets.count*3)
      for k in 0..<n {
        var a=x,b=x;a[k]+=e;b[k]-=e
        let va=evaluate(a),vb=evaluate(b)
        for i in targets.indices {let d=(va[i]-vb[i])/(2*e*targets[i].tolerance);for axis in 0..<3 {jac[i*3+axis][k]=d[axis]}}
      }
      var a=Array(repeating:Array(repeating:Float(0),count:n+1),count:n)
      for i in 0..<n {
        for j in 0..<n {a[i][j]=jac.indices.reduce(0){$0+jac[$1][i]*jac[$1][j]}}
        a[i][i]+=1 // metre-space regularization favours the smaller source edit.
        for row in jac.indices {a[i][n] += jac[row][i]*(targets[row/3].target[row%3]-values[row/3][row%3])/targets[row/3].tolerance}
      }
      var singular=false
      for k in 0..<n {
        let pivot=(k..<n).max{abs(a[$0][k])<abs(a[$1][k])}!
        if abs(a[pivot][k])<1e-8 {singular=true;break}
        a.swapAt(k,pivot);let divisor=a[k][k]
        for j in k...n {a[k][j]/=divisor}
        for i in 0..<n where i != k {let factor=a[i][k];for j in k...n {a[i][j]-=factor*a[k][j]}}
      }
      if singular {break}
      let previous=objective(values);var accepted=false
      for step:Float in [1,0.5,0.25,0.125,0.0625] {
        var candidate=x
        for i in 0..<n {candidate[i] += step*a[i][n]}
        // Bound individual brush offsets; full candidate still passes the
        // normal native compiler's fold/collapse validation before publication.
        if controls.indices.contains(where:{i in length(V3(candidate[i*3],candidate[i*3+1],candidate[i*3+2]))>maximumDisplacement}) {continue}
        let result=evaluate(candidate)
        if objective(result)<previous {x=candidate;values=result;accepted=true;break}
      }
      if !accepted {break}
    }
    let residuals=targets.indices.map{SculptResidual(id:targets[$0].id,actual:values[$0],target:targets[$0].target,error:length(values[$0]-targets[$0].target),tolerance:targets[$0].tolerance)}
    let failed=residuals.filter{$0.error>$0.tolerance}
    try CraftError.require(failed.isEmpty,"sculptFit.conflict",failed.map{"\($0.id): residual \($0.error) m > \($0.tolerance) m"}.joined(separator:"; ")+". Broaden/add controls, reduce the requested change, or revise conflicting preservation targets.")
    let layer=SculptLayer(id:id,strokes:strokes(x).filter{$0.strength>1e-8})
    let compiled=try SculptCompiler.apply(layer.activeStrokes,to:mesh)
    let maximum=mesh.vertices.indices.reduce(Float(0)){max($0,length(compiled.vertices[$1].position-mesh.vertices[$1].position))}
    try CraftError.require(maximum<=maximumDisplacement,"sculptFit.displacement","Combined edit exceeds the full-surface displacement limit: \(maximum) m")
    let protected=mesh.vertices.indices.filter {i in let p=mesh.vertices[i].position;return protect.contains{$0.retention(V3(p.x,p.y,p.z))==0}}
    let protectedMaximum=protected.reduce(Float(0)){max($0,length(compiled.vertices[$1].position-mesh.vertices[$1].position))}
    let normalChange=mesh.vertices.indices.reduce(Float(0)){value,i in
      let a=mesh.vertices[i].normal,b=compiled.vertices[i].normal
      return max(value,acos(clamp(dot(normalize(V3(a.x,a.y,a.z)),normalize(V3(b.x,b.y,b.z))),-1,1))*180 / .pi)
    }
    try CraftError.require(normalChange<=maximumNormalChangeDegrees,"sculptFit.normalChange",
      "Targets fit, but the surface turns \(normalChange) degrees, above the authored \(maximumNormalChangeDegrees) degree limit. Broaden the feather, reduce the displacement or revise protection; no candidate was published.")
    return Result(layer:layer,residuals:residuals,iterations:used,maximumDisplacement:maximum,protectedVertices:protected.count,maximumProtectedDisplacement:protectedMaximum,maximumNormalChangeDegrees:normalChange)
  }
}
