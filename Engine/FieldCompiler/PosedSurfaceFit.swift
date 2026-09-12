import FieldCore
import simd

/// A visible posed surface point maps through one triangle's material
/// coordinates to a compact source corrective. No deformed vertex positions are
/// saved as a replacement mesh. The selected barycentric point remains fixed
/// during the solve, so the fit cannot jump to a nearby limb or opposite skin.
public enum PosedSurfaceFit {
  public struct Result: Sendable {
    public var field:SurfaceEdit
    public var posedPoint:V3
    public var correctedPoint:V3
    public var selectionDistance:Float
    public var residual:Float
    public var triangle:Int
  }

  public static func fit(id:String,part:String,mesh:Mesh,weights:[SkinWeight],
    palette:[simd_float4x4],rigid:simd_float4x4=matrix_identity_float4x4,
    point:V3,offset:V3,radius:Float,activation:Float=1,maximumProjection:Float=0.1) throws -> Result {
    try CraftError.require(CraftMath.finite(point,limit:100) && CraftMath.finite(offset,limit:0.5)
      && radius.isFinite && (0.02...3).contains(radius) && activation.isFinite && (0.05...1).contains(activation)
      && maximumProjection.isFinite && (0.001...0.5).contains(maximumProjection),
      "posedSurface.values","Use finite posed metre coordinates, offset within ±0.5 m, radius 0.02…3 m, activation ≥0.05 and projection 0.001…0.5 m")
    try CraftError.require(!mesh.indices.isEmpty && (weights.isEmpty || weights.count==mesh.vertices.count)
      && weights.allSatisfy{$0.validate(count:palette.count)},"posedSurface.binding","Surface needs valid compiled topology and bindings")
    let skin=SkinningPalette(palette)
    let matrices=mesh.vertices.indices.map{weights.isEmpty ? rigid:skin.matrix(weights[$0])}
    let rest=mesh.vertices.map{V3($0.position.x,$0.position.y,$0.position.z)}
    let posed=rest.indices.map{CraftMath.point(matrices[$0],rest[$0])}
    var best:Float = .greatestFiniteMagnitude,triangle = -1,bary=V3.zero,selected=V3.zero
    for k in stride(from:0,to:mesh.indices.count,by:3) {
      let a=posed[Int(mesh.indices[k])],b=posed[Int(mesh.indices[k+1])],c=posed[Int(mesh.indices[k+2])]
      let w=CostumeCompiler.closestBarycentric(point,a,b,c)
      let p=a*w.x+b*w.y+c*w.z,d=length_squared(p-point)
      if d<best {best=d;triangle=k/3;bary=w;selected=p}
    }
    try CraftError.require(triangle>=0 && sqrt(best)<=maximumProjection,"posedSurface.selection",
      "Nearest posed surface is \(sqrt(best)) m away; select a point on the visible part or explicitly enlarge maximumProjection")
    let indices=(0..<3).map{Int(mesh.indices[triangle*3+$0])}
    let center=(0..<3).reduce(V3.zero){$0+rest[indices[$1]]*bary[$1]}
    var field=SurfaceEdit(id:id,part:part,center:center,radius:V3(repeating:radius))
    var response=simd_float3x3(0)
    for k in 0..<3 {
      let m=matrices[indices[k]],weight=field.evaluate(rest[indices[k]]).weight*bary[k]*activation
      for column in 0..<3 {response[column] += V3(m[column].x,m[column].y,m[column].z)*weight}
    }
    try CraftError.require(abs(response.determinant)>0.00001,"posedSurface.singular",
      "Selected skin response collapses this control; revise skin weights or choose a larger supported region")
    field.offset=response.inverse*offset
    try field.validate()
    // For w=(1-r²)^3, max |dw/dr|=96/(25√5). This pure
    // translation field has J=I+offset⊗∇w, so this bounds det J
    // everywhere, including between vertices of a coarse preview mesh.
    let derivativeBound:Float=96/(25*sqrt(5))
    try CraftError.require(1-derivativeBound*length(field.offset/field.radius)>0.05,"posedSurface.fold",
      "Full corrective displacement is too large for its radius; enlarge the brush or use a smaller posed displacement")
    // Validate the full-strength source, not only the active preview. Existing
    // global field/fold checks remain in the asset's normal compile path too.
    _ = try SurfaceEditing.apply([field],to:mesh)
    var corrected=V3.zero
    var active=field;active.offset *= activation
    for k in 0..<3 {corrected += CraftMath.point(matrices[indices[k]],active.evaluate(rest[indices[k]]).position)*bary[k]}
    let residual=length(corrected-(selected+offset))
    try CraftError.require(residual<0.0001,"posedSurface.residual","Fit residual \(residual) m exceeds 0.1 mm")
    return Result(field:field,posedPoint:selected,correctedPoint:corrected,
      selectionDistance:sqrt(best),residual:residual,triangle:triangle)
  }
}
