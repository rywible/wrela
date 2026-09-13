import simd

/// Source-authored spatial sampling density, in bind metres. This controls the
/// volume discretization before extraction, independently of surface sculpting.
public struct SurfaceSamplingRegion:Codable,Equatable,Sendable {
  public var id:String
  public var center:V3
  public var radius:V3
  public var edgeLength:Float
  public init(id:String,center:V3,radius:V3,edgeLength:Float) {
    self.id=id;self.center=center;self.radius=radius;self.edgeLength=edgeLength
  }
}
public struct SurfaceSampling:Codable,Equatable,Sendable {
  public var baseEdgeLength:Float
  public var regions:[SurfaceSamplingRegion]
  public var maximumTetrahedra:Int
  public init(baseEdgeLength:Float=0.04,regions:[SurfaceSamplingRegion]=[],maximumTetrahedra:Int=500_000) {
    self.baseEdgeLength=baseEdgeLength;self.regions=regions;self.maximumTetrahedra=maximumTetrahedra
  }
  public func validate() throws {
    try CraftError.require(baseEdgeLength.isFinite && (0.005...1).contains(baseEdgeLength)
      && regions.count<=16 && Set(regions.map(\.id)).count==regions.count && (1_000...2_000_000).contains(maximumTetrahedra),
      "sampling","Use a base edge length of 5…1000 mm, at most 16 unique regions and a budget of 1,000…2,000,000 tetrahedra")
    for r in regions {
      try CraftError.require(!r.id.isEmpty && r.id.count<=80 && CraftMath.finite(r.center,limit:100)
        && CraftMath.finite(r.radius,limit:10) && r.radius.min()>=0.005 && r.edgeLength.isFinite
        && (0.001...baseEdgeLength).contains(r.edgeLength),"sampling.\(r.id)",
        "Use finite bind metres, radii of 5 mm…10 m and an edge length of 1 mm up to the base length")
    }
  }
  public func edgeLength(in bounds:Bounds)->Float {
    var length=baseEdgeLength
    for r in regions {
      let closest=max(bounds.min,min(bounds.max,r.center)),q=(closest-r.center)/r.radius
      if length_squared(q)<=1 {length=min(length,r.edgeLength)}
    }
    return length
  }
}
