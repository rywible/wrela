import Foundation
import simd

/// A compact, ordered deformation field in bind-space metres. The compiled mesh
/// remains disposable. Smooth compact support keeps the untouched region exact.
public struct SurfaceEdit: Codable, Equatable, Sendable {
  public var id: String
  public var targets: [String]?
  public var part: String
  public var center: V3
  public var radius: V3
  public var offset: V3
  public var dilation: V3
  public init(id: String, part: String, center: V3, radius: V3, offset: V3 = .zero, dilation: V3 = .zero) {
    self.id=id; self.part=part; self.center=center; self.radius=radius
    self.offset=offset; self.dilation=dilation
  }
  public func validate() throws {
    guard !id.isEmpty, id.count <= 80, !part.isEmpty,
      [center,radius,offset,dilation].allSatisfy({v in (0..<3).allSatisfy {v[$0].isFinite}}),
      (0..<3).allSatisfy({abs(center[$0]) <= 100 && (0.005...20).contains(radius[$0]) && abs(offset[$0]) <= 2 && abs(dilation[$0]) <= 0.5})
    else {throw SurfaceEditError.invalid(id)}
  }
  public func evaluate(_ p: V3) -> (position: V3, jacobian: simd_float3x3, weight: Float) {
    let q=p-center, d=q/radius, r2=dot(d,d)
    if r2 >= 1 {return (p,matrix_identity_float3x3,0)}
    let a=1-r2, w=a*a*a
    let gradient = -6*a*a*q/(radius*radius)
    let displacement=offset+q*dilation
    var j=matrix_identity_float3x3
    for k in 0..<3 {j[k] += displacement*gradient[k]; j[k][k] += w*dilation[k]}
    return (p+w*displacement,j,w)
  }
}

public enum SurfaceEditError: LocalizedError {
  case invalid(String), folded(String), missed(String)
  public var errorDescription: String? {
    switch self {
    case .invalid(let id): return "Surface layer '\(id)' has invalid bounds or values; use finite bind-space metres and positive radii."
    case .folded(let id): return "Surface layer '\(id)' collapses or reverses sampled geometry; reduce displacement or enlarge its radius."
    case .missed(let id): return "Surface layer '\(id)' touches no vertices on its primary part; use surfaceProbe to locate the region."
    }
  }
}
