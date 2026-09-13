import simd
import FieldCore

/// Conservative Bernstein enclosure of squared triangle area throughout linear
/// vertex motion. Endpoint-normal agreement is not an orientation invariant in
/// 3D: a healthy rotating triangle can exceed 90 degrees, while matching endpoint
/// normals can conceal an intermediate collapse. This checks individual faces,
/// not global surface self-intersection or collisions between adjacent faces.
public enum TriangleMotion {
  public static func preservesArea(from start:[V3],to end:[V3],minimumSquaredRatio:Double=0.0025)->Bool {
    guard start.count==3 && end.count==3 && minimumSquaredRatio>=0 && minimumSquaredRatio<1 else {return false}
    func d(_ p:V3)->SIMD3<Double> {SIMD3(Double(p.x),Double(p.y),Double(p.z))}
    let e=d(start[1])-d(start[0]),f=d(start[2])-d(start[0])
    let de=d(end[1])-d(end[0])-e,df=d(end[2])-d(end[0])-f
    let a=cross(e,f),size=length(a)
    guard size.isFinite && size>1e-14 else {return false}
    let x=a/size,y=(cross(de,f)+cross(e,df))/size,z=cross(de,df)/size
    let c0=dot(x,x)-minimumSquaredRatio,c1=2*dot(x,y),c2=dot(y,y)+2*dot(x,z),c3=2*dot(y,z),c4=dot(z,z)
    let coefficients=[c0,c0+c1/4,c0+c1/2+c2/6,c0+3*c1/4+c2/2+c3/4,c0+c1+c2+c3+c4]
    guard coefficients.allSatisfy(\.isFinite) else {return false}
    // Keep the initial arithmetic allowance through subdivision. A shrinking
    // coefficient range must not erase uncertainty inherited from cancellation.
    let allowance=1e-10*(1+(coefficients.map{abs($0)}.max() ?? 0))
    func certify(_ values:[Double],_ depth:Int)->Bool {
      if values.min()!>allowance {return true}
      if values.max()!<=allowance || depth==0 {return false}
      var row=values,left=[row.first!],right=[row.last!]
      while row.count>1 {row=zip(row,row.dropFirst()).map{($0+$1)*0.5};left.append(row.first!);right.append(row.last!)}
      return certify(left,depth-1) && certify(Array(right.reversed()),depth-1)
    }
    return certify(coefficients,12)
  }
}
