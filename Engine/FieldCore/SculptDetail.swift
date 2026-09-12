import simd

/// A source-authored local tessellation budget. This adds deformation capacity;
/// it does not invent shape detail or change the authoritative field anatomy.
public struct SculptDetail:Codable,Equatable,Sendable {
  public var id:String
  public var part:String
  public var center:V3
  public var radius:V3
  public var edgeLength:Float
  public var maximumNewVertices:Int
  public init(id:String,part:String,center:V3,radius:V3,edgeLength:Float,maximumNewVertices:Int=50_000) {
    self.id=id;self.part=part;self.center=center;self.radius=radius;self.edgeLength=edgeLength;self.maximumNewVertices=maximumNewVertices
  }
  public func validate() throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && !part.isEmpty && CraftMath.finite(center,limit:100)
      && CraftMath.finite(radius,limit:10) && radius.min()>=0.005 && edgeLength.isFinite && (0.001...1).contains(edgeLength)
      && (1...250_000).contains(maximumNewVertices),"detail.\(id)","Use bind metres, radii 0.005…10 m, edge length 0.001…1 m and a budget of 1…250,000 added vertices")
  }
  public func intersects(_ a:V3,_ b:V3)->Bool {
    let p=(a-center)/radius,d=(b-a)/radius,t=clamp(-dot(p,d)/max(dot(d,d),1e-20),0,1)
    return length_squared(p+d*t)<=1
  }
}
