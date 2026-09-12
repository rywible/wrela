import simd

public enum GuideFrameMode:String,Codable,Sendable {case rotation, surface, curve}
public enum GuideFrame {
  /// Parallel-transport frames for an ordered curve. Using both neighbouring
  /// points matches a smooth curve tangent and avoids independent node roll.
  public static func curve(old:[V3],new:[V3])->[simd_float3x3]? {
    guard old.count==new.count,old.count>=2 else {return nil}
    func tangents(_ points:[V3])->[V3]? {
      var result:[V3]=[]
      for i in points.indices {
        let d=points[min(i+1,points.count-1)]-points[max(0,i-1)]
        guard length_squared(d)>1e-10 else {return nil}
        result.append(normalize(d))
      }
      return result
    }
    guard let a=tangents(old),let b=tangents(new) else {return nil}
    let reference=abs(a[0].y)<0.9 ? V3(0,1,0):V3(1,0,0)
    var n=normalize(cross(a[0],reference))
    var m=simd_quatf(from:a[0],to:b[0]).act(n)
    var result:[simd_float3x3]=[]
    for i in old.indices {
      if i>0 {
        n=simd_quatf(from:a[i-1],to:a[i]).act(n)
        m=simd_quatf(from:b[i-1],to:b[i]).act(m)
      }
      n=normalize(n-a[i]*dot(n,a[i]));m=normalize(m-b[i]*dot(m,b[i]))
      let rest=simd_float3x3(columns:(n,cross(a[i],n),a[i]))
      let moved=simd_float3x3(columns:(m,cross(b[i],m),b[i]))
      result.append(moved*rest.transpose)
    }
    return result
  }
  /// Local affine surface differential. Tangent stretch/shear follows the cage;
  /// the unit normal preserves thickness. Degenerate tangent patches fall back.
  public static func surface(old:[V3],new:[V3])->simd_float3x3? {
    guard old.count==new.count,old.count>=2,let first=old.indices.first(where:{length_squared(old[$0])>1e-8}),
      let second=old.indices.first(where:{length_squared(cross(old[first],old[$0]))>1e-8}) else {return nil}
    let oldCross=cross(old[first],old[second]),newCross=cross(new[first],new[second])
    guard length_squared(newCross)>1e-10 else {return nil}
    let n=normalize(oldCross),m=normalize(newCross)
    func outer(_ a:V3,_ b:V3)->simd_float3x3 {simd_float3x3(columns:(a*b.x,a*b.y,a*b.z))}
    var rest=outer(n,n),deformed=outer(m,n)
    for i in old.indices where length_squared(old[i])>1e-8 {
      let weight:Float=1/length_squared(old[i])
      rest += outer(old[i],old[i])*weight
      deformed += outer(new[i],old[i])*weight
    }
    guard abs(rest.determinant)>1e-8 else {return nil}
    let result=deformed*rest.inverse
    guard abs(result.determinant)>1e-5,(0..<3).allSatisfy({axis in (0..<3).allSatisfy{result[axis][$0].isFinite}}) else {return nil}
    return result
  }
}
