import FieldCore
import simd

/// One bounded degree of freedom can link several source parameters, e.g. two
/// mirrored radii or opposing endpoint translations. Bounds are deltas, not
/// absolute parameter values. Terms in a control must use the same unit.
public struct AnatomyFitTerm:Codable,Sendable {
  public var element:String
  public var parameter:String
  public var scale:Float
  public init(element:String,parameter:String,scale:Float=1) {self.element=element;self.parameter=parameter;self.scale=scale}
}
public struct AnatomyFitControl:Codable,Sendable {
  public var id:String
  public var terms:[AnatomyFitTerm]
  public var minimum:Float
  public var maximum:Float
  public init(id:String,terms:[AnatomyFitTerm],minimum:Float,maximum:Float) {self.id=id;self.terms=terms;self.minimum=minimum;self.maximum=maximum}
}
public struct AnatomySurfaceTarget:Codable,Sendable {
  public var id:String
  public var position:V3
  public var tolerance:Float
  public init(id:String,position:V3,tolerance:Float=0.001) {self.id=id;self.position=position;self.tolerance=tolerance}
}
public struct AnatomyFitResidual:Codable,Sendable {
  public var id:String
  public var initialLinearizedDistance:Float
  public var linearizedDistance:Float
  public var tolerance:Float
}
public struct AnatomyFitChange:Codable,Sendable {
  public var id:String
  public var delta:Float
  public var atBound:Bool
}
/// Fit the authoritative level set to explicit surface targets. This changes
/// named anatomy parameters, never extracted vertex deltas. Surface membership
/// is not material-point correspondence or an aesthetic/collision certificate.
public enum AnatomyIntentFit {
  public struct Result:Sendable {
    public var source:AnatomySource
    public var residuals:[AnatomyFitResidual]
    public var changes:[AnatomyFitChange]
    public var iterations:Int
  }
  public static func fit(_ source:AnatomySource,controls:[AnatomyFitControl],targets:[AnatomySurfaceTarget],iterations:Int=24) throws -> Result {
    try source.validate()
    try CraftError.require((1...12).contains(controls.count) && (1...64).contains(targets.count) && (1...64).contains(iterations),"anatomyFit.budget","Use 1…12 controls, 1…64 surface targets and 1…64 iterations")
    try CraftError.require(Set(controls.map(\.id)).count==controls.count && Set(targets.map(\.id)).count==targets.count
      && controls.allSatisfy{!$0.id.isEmpty} && targets.allSatisfy{!$0.id.isEmpty},"anatomyFit.ids","Use unique nonempty control and target IDs")
    struct Term {var index:Int;var family:String;var axis:Int;var section:Int?;var scale:Float}
    var used:Set<String>=[]
    let terms:[[Term]]=try controls.map{c in
      try CraftError.require((1...8).contains(c.terms.count) && c.minimum.isFinite && c.maximum.isFinite && c.minimum<=0 && c.maximum>=0 && c.minimum<c.maximum,"anatomyFit.\(c.id)","Use 1…8 linked terms and finite delta bounds containing zero")
      var unit:String?
      return try c.terms.map{t in
        guard let index=source.elements.firstIndex(where:{$0.id==t.element}),source.elements[index].role == .surface else {
          throw CraftError(path:"anatomyFit.\(c.id)",reason:"Unknown surface element \(t.element)")
        }
        try CraftError.require(!t.parameter.isEmpty && t.parameter.count<=120,"anatomyFit.parameter","Use a named source parameter")
        let element=source.elements[index],pieces=t.parameter.split(separator:"."),family=String(pieces[0]),axis=pieces.count==2 ? ["x","y","z"].firstIndex(of:String(pieces[1])):nil
        let sectionIndex=pieces.count>=3 && family=="section" && element.primitive == .loft ? element.sections?.firstIndex{$0.id==String(pieces[1])}:nil
        let sectionFamily=pieces.count>=3 ? String(pieces[2]):""
        let sectionAxis=pieces.count==4 ? ["x","y"].firstIndex(of:String(pieces[3])):nil
        let sectionAllowed=sectionIndex != nil && ((pieces.count==3 && sectionFamily=="z") || (sectionAxis != nil && ["center","radius"].contains(sectionFamily)))
        let allowed=sectionAllowed || (pieces.count==2 && axis != nil && (family=="center" || ((element.primitive == .ellipsoid && family=="radius") || (element.primitive != .capsule && family=="rotation")) || (element.primitive == .capsule && family=="end")))
          || (pieces.count==1 && ((family=="radius" && element.primitive == .capsule) || (family=="blend" && element.operation == .add) || (family=="cutBlend" && element.operation == .cut)))
        let nextUnit=family=="rotation" ? "degree":"metre",limit:Float=nextUnit=="degree" ? 45:0.5
        try CraftError.require(allowed && pieces.joined(separator:".")==t.parameter && t.scale.isFinite && abs(t.scale)>0 && abs(t.scale)<=4 && max(abs(c.minimum),abs(c.maximum))*abs(t.scale)<=limit
          && (unit==nil || unit==nextUnit),"anatomyFit.\(c.id)","Use supported source parameters, one unit per linked control, scale up to ±4, and per-parameter changes within ±0.5 m / ±45 degrees")
        unit=nextUnit
        try CraftError.require(used.insert(t.element+"/"+t.parameter).inserted,"anatomyFit.\(c.id)","Each source parameter may belong to only one control")
        return Term(index:index,family:sectionAllowed ? "section."+sectionFamily:family,axis:sectionAxis ?? axis ?? 0,section:sectionIndex,scale:t.scale)
      }
    }
    for t in targets {try CraftError.require(CraftMath.finite(t.position,limit:100) && t.tolerance.isFinite && (0.00001...0.01).contains(t.tolerance),"anatomyFit.\(t.id)","Use finite bind positions and 0.01…10 mm tolerance")}
    let n=controls.count,scales=controls.map{max(abs($0.minimum),abs($0.maximum))}
    func candidate(_ x:[Float]) throws -> AnatomySource {
      var next=source
      for i in controls.indices {for term in terms[i] {
        let delta=x[i]*scales[i]*term.scale,k=term.index,a=term.axis
        switch term.family {
        case "section.z":next.elements[k].sections![term.section!].z+=delta
        case "section.center":next.elements[k].sections![term.section!].center[a]+=delta
        case "section.radius":next.elements[k].sections![term.section!].radius[a]+=delta
        case "center":next.elements[k].center[a]+=delta
        case "end":next.elements[k].end[a]+=delta
        case "rotation":next.elements[k].rotation[a]+=delta
        case "radius":
          if next.elements[k].primitive == .capsule {next.elements[k].radius += V3(repeating:delta)}
          else {next.elements[k].radius[a]+=delta}
        case "blend":next.elements[k].blend+=delta
        default:next.elements[k].cutBlend=(source.elements[k].cutBlend ?? 0)+delta
        }
      }}
      try next.validate();return next
    }
    func evaluate(_ x:[Float]) throws -> [Float] {
      let field=try candidate(x).shape()
      return try targets.map{t in
        let sample=field.sample(at:t.position),g=length(sample.gradient)
        try CraftError.require(g>0.00001,"anatomyFit.\(t.id)","Singular field gradient at surface target; choose an unambiguous boundary point")
        return sample.value/g
      }
    }
    func objective(_ values:[Float])->Float {targets.indices.reduce(0){$0+pow(values[$1]/targets[$1].tolerance,2)}}
    var x=Array(repeating:Float(0),count:n),values=try evaluate(Array(repeating:0,count:n)),usedIterations=0
    let initial=values
    try CraftError.require(values.allSatisfy{abs($0)<=0.25},"anatomyFit.targets","Surface targets must start within 0.25 m linearized distance of the source")
    for iteration in 0..<iterations {
      if targets.indices.allSatisfy({abs(values[$0])<=targets[$0].tolerance}) {break}
      usedIterations=iteration+1
      var jac=Array(repeating:Array(repeating:Float(0),count:n),count:targets.count)
      for k in 0..<n {
        var lo=x,hi=x
        lo[k]=max(controls[k].minimum/scales[k],x[k]-0.001)
        hi[k]=min(controls[k].maximum/scales[k],x[k]+0.001)
        let a=(try? evaluate(lo)),b=(try? evaluate(hi))
        for j in targets.indices {
          if let a,let b,hi[k]>lo[k] {jac[j][k]=(b[j]-a[j])/(hi[k]-lo[k])/targets[j].tolerance}
          else if let b,hi[k]>x[k] {jac[j][k]=(b[j]-values[j])/(hi[k]-x[k])/targets[j].tolerance}
          else if let a,x[k]>lo[k] {jac[j][k]=(values[j]-a[j])/(x[k]-lo[k])/targets[j].tolerance}
        }
      }
      var a=Array(repeating:Array(repeating:Float(0),count:n+1),count:n)
      for i in 0..<n {
        for j in 0..<n {a[i][j]=targets.indices.reduce(0){$0+jac[$1][i]*jac[$1][j]}}
        a[i][i]+=0.001
        a[i][n] = -targets.indices.reduce(Float(0)){$0+jac[$1][i]*values[$1]/targets[$1].tolerance}-0.001*x[i]
      }
      for k in 0..<n {
        let pivot=(k..<n).max{abs(a[$0][k])<abs(a[$1][k])}!;a.swapAt(k,pivot)
        let divisor=a[k][k];for j in k...n {a[k][j]/=divisor}
        for i in 0..<n where i != k {let f=a[i][k];for j in k...n {a[i][j]-=f*a[k][j]}}
      }
      var accepted=false
      for step:Float in [1,0.5,0.25,0.125,0.0625,0.03125] {
        let next=x.indices.map{clamp(x[$0]+step*a[$0][n],controls[$0].minimum/scales[$0],controls[$0].maximum/scales[$0])}
        if let result=try? evaluate(next),objective(result)<objective(values) {x=next;values=result;accepted=true;break}
      }
      if !accepted {break}
    }
    let changes=controls.indices.map{i in AnatomyFitChange(id:controls[i].id,delta:x[i]*scales[i],atBound:min(abs(x[i]*scales[i]-controls[i].minimum),abs(x[i]*scales[i]-controls[i].maximum))<scales[i]*0.0001)}
    let residuals=targets.indices.map{i in AnatomyFitResidual(id:targets[i].id,initialLinearizedDistance:abs(initial[i]),linearizedDistance:abs(values[i]),tolerance:targets[i].tolerance)}
    let failed=residuals.filter{$0.linearizedDistance>$0.tolerance}
    try CraftError.require(failed.isEmpty,"anatomyFit.conflict",failed.map{"\($0.id): linearized residual \($0.linearizedDistance) m > \($0.tolerance) m"}.joined(separator:"; ")+". Controls at bounds: \(changes.filter(\.atBound).map(\.id).joined(separator:", ")). Revise conflicting targets, choose active source controls or explicitly broaden bounds; source unchanged.")
    return Result(source:try candidate(x),residuals:residuals,changes:changes,iterations:usedIterations)
  }
}
