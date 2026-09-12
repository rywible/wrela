import Foundation
import simd

/// Ordered bind-space finish fields; source stays independent of topology/UVs.
public struct SurfaceLayer: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var center:V3
  public var radius:V3
  public var tint:V3
  public var amount:Float
  public var frequency:Float
  public var roughness:Float
  public var metallic:Float
  public var relief:Float
  public var pattern:Int
  public var seed:Float
  public init(id:String,part:String,center:V3,radius:V3,tint:V3,amount:Float=1,frequency:Float=20,
    roughness:Float = -1,metallic:Float = -1,relief:Float=0,pattern:Int=0,seed:Float=0) {
    self.id=id;self.part=part;self.center=center;self.radius=radius;self.tint=tint;self.amount=amount
    self.frequency=frequency;self.roughness=roughness;self.metallic=metallic;self.relief=relief;self.pattern=pattern;self.seed=seed
  }
  public func validate() throws {
    guard !id.isEmpty,id.count<=80,!part.isEmpty,(0..<3).allSatisfy({center[$0].isFinite && abs(center[$0])<=100 && radius[$0].isFinite && (0.005...100).contains(radius[$0]) && tint[$0].isFinite && (0...1).contains(tint[$0])}),
      [amount,frequency,roughness,metallic,relief,seed].allSatisfy(\.isFinite),(0...1).contains(amount),
      (0.1...1000).contains(frequency),(-1...1).contains(roughness),(-1...1).contains(metallic),
      abs(relief)<=0.03,(0...3).contains(pattern),abs(seed)<=1000 else {throw RigError.invalid}
  }
  public func envelope(_ point:V3)->Float {let q=(point-center)/radius;let w=max(0,1-dot(q,q));return w*w}
  public var uniform:SurfaceLayerUniform {
    SurfaceLayerUniform(center:SIMD4(center,seed),shape:SIMD4(1/radius,frequency),color:SIMD4(tint,amount),finish:SIMD4(roughness,metallic,relief,Float(pattern)))
  }
}
/// Matches SurfaceLayerUniform in Surface.metal: four aligned float4 values.
public struct SurfaceLayerUniform: Sendable {
  public var center:SIMD4<Float>
  public var shape:SIMD4<Float>
  public var color:SIMD4<Float>
  public var finish:SIMD4<Float>
}
