import Foundation
import simd

public struct SkinField: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var center:V3
  public var radius:V3
  public var jointWeights:[String:Float]
  public var strength:Float
  public init(id:String,part:String,center:V3,radius:V3,jointWeights:[String:Float],strength:Float=1) {
    self.id=id;self.part=part;self.center=center;self.radius=radius;self.jointWeights=jointWeights;self.strength=strength
  }
  public func validate(joints:Set<String>) throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && CraftMath.finite(center,limit:100) && CraftMath.finite(radius,limit:20)
      && (0..<3).allSatisfy{radius[$0]>=0.005} && strength.isFinite && (0...1).contains(strength)
      && (1...4).contains(jointWeights.count) && jointWeights.keys.allSatisfy(joints.contains)
      && jointWeights.values.allSatisfy{$0.isFinite && $0>=0} && abs(jointWeights.values.reduce(0,+)-1)<0.0001,
      "skinField.\(id)","Use existing joints, normalized nonnegative weights and finite spatial bounds")
  }
  public func coverage(_ p:V3)->Float {pow(max(0,1-length_squared((p-center)/radius)),3)*strength}
}

/// Compact corrective applied before skinning. Weight is driven by a local
/// authored joint angle, with a smooth activation interval. The field's Jacobian
/// also corrects normals; no per-frame remeshing or editor-only deformation.
public struct PoseCorrective: Codable, Equatable, Sendable {
  public var id:String
  public var driver:String
  public var axis:Int
  public var start:Float
  public var end:Float
  public var field:SurfaceEdit
  public init(id:String,driver:String,axis:Int,start:Float,end:Float,field:SurfaceEdit) {
    self.id=id;self.driver=driver;self.axis=axis;self.start=start;self.end=end;self.field=field
  }
  public func activation(_ pose:JointPose)->Float {CraftMath.smooth((pose.rotation[axis]-start)/(end-start))}
  public func validate(joints:Set<String>) throws {
    try field.validate()
    try CraftError.require(!id.isEmpty && id.count<=80 && joints.contains(driver) && (0...2).contains(axis)
      && start.isFinite && end.isFinite && abs(start)<=360 && abs(end)<=360 && abs(end-start)>=0.1,
      "corrective.\(id)","Use an existing driver, axis 0…2 and a nonzero angular interval")
  }
}
/// Four float4s (64 bytes), synchronized with Surface.metal. Offset/dilation
/// already include activation so the GPU applies exactly the CPU field.
public struct CorrectiveUniform: Sendable {
  public var center:SIMD4<Float>
  public var radius:SIMD4<Float>
  public var offset:SIMD4<Float>
  public var dilation:SIMD4<Float>
  public init(_ field:SurfaceEdit,weight:Float) {
    center=SIMD4(field.center,0);radius=SIMD4(field.radius,0)
    offset=SIMD4(field.offset*weight,0);dilation=SIMD4(field.dilation*weight,0)
  }
}
