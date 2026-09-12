import simd

/// Blends rigid skin transforms on SE(3), avoiding the shrinking cross-sections
/// produced by linearly blending rotation matrices. Scale, shear and reflection
/// retain the original affine path; guide surface differentials are not rigid.
///
/// Normalized dual-quaternion blending follows Kavan et al. (TOG 2008). This is
/// geometric skinning, not a muscle model or a guarantee of mesh volume/contact.
public struct SkinningPalette: Sendable {
  public enum Method: String, Sendable { case linear, dualQuaternion }
  public let method: Method
  public let matrices: [simd_float4x4]
  private let duals: [Dual]

  private struct Dual: Sendable {
    var real: SIMD4<Float>
    var dual: SIMD4<Float>
  }

  public init(_ matrices: [simd_float4x4], rigidBlending: Bool = true) {
    self.matrices = matrices
    let rigid = rigidBlending && !matrices.isEmpty && matrices.allSatisfy(Self.isRigid)
    method = rigid ? .dualQuaternion : .linear
    duals = rigid ? matrices.map { m in
      let q = simd_normalize(simd_quatf(m)).vector
      let t = V3(m[3].x, m[3].y, m[3].z), r = V3(q.x, q.y, q.z)
      return Dual(real:q, dual:SIMD4((t * q.w + cross(t, r)) * 0.5, -dot(t, r) * 0.5))
    } : []
  }

  /// The tolerance covers matrix-product rounding only, not authored scaling.
  public static func isRigid(_ m: simd_float4x4) -> Bool {
    guard (0..<4).allSatisfy({ c in (0..<4).allSatisfy { m[c][$0].isFinite } }) else { return false }
    let e:Float = 0.00002
    guard abs(m[0].w)<e, abs(m[1].w)<e, abs(m[2].w)<e, abs(m[3].w-1)<e else { return false }
    let x = V3(m[0].x,m[0].y,m[0].z), y = V3(m[1].x,m[1].y,m[1].z), z = V3(m[2].x,m[2].y,m[2].z)
    return abs(length_squared(x)-1)<e && abs(length_squared(y)-1)<e && abs(length_squared(z)-1)<e
      && abs(dot(x,y))<e && abs(dot(x,z))<e && abs(dot(y,z))<e && dot(cross(x,y),z)>0
  }

  /// A matrix for this vertex's blended rigid transform, or its exact affine
  /// weighted matrix. Audits must use this same blend as the production shader.
  public func matrix(_ skin: SkinWeight) -> simd_float4x4 {
    guard method == .dualQuaternion else { return skin.matrix(matrices) }
    var reference = 0
    for i in 1..<4 where skin.weights[i]>skin.weights[reference] { reference=i }
    let anchor = duals[Int(skin.joints[reference])].real
    var real = SIMD4<Float>.zero, dual = SIMD4<Float>.zero
    for i in 0..<4 {
      let q=duals[Int(skin.joints[i])]
      let w=skin.weights[i] * (dot(q.real,anchor)<0 ? -1:1)
      real += q.real*w;dual += q.dual*w
    }
    let inverseLength = 1 / max(length(real), 1e-8)
    real *= inverseLength;dual *= inverseLength
    let r=V3(real.x,real.y,real.z),d=V3(dual.x,dual.y,dual.z)
    // The dual component parallel to real cancels from this translation formula.
    let translation = 2 * (real.w*d - dual.w*r + cross(r,d))
    var result=simd_float4x4(simd_quatf(vector:real))
    result[3]=SIMD4(translation,1)
    return result
  }

  /// ABI shared with Surface.metal: the high count bit selects quaternion data.
  /// Each 64-byte entry keeps the existing palette binding/limit; columns 0 and
  /// 1 carry quaternion xyzw real/dual components. Affine entries are unchanged.
  public var encodedCount: UInt32 {
    UInt32(matrices.count) | (method == .dualQuaternion ? 0x80000000:0)
  }
  public var encodedMatrices: [simd_float4x4] {
    method == .linear ? matrices : duals.map { simd_float4x4(columns:($0.real,$0.dual,.zero,.zero)) }
  }
}
