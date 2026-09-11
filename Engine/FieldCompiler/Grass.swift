import simd
import FieldCore

/// 32-byte source descriptor. Positions/dimensions remain FP32; only bounded
/// color is quantized (10 bits/channel, at most 1/2046 absolute error).
public struct GrassBlade:Sendable {
    public var anchorAngle:SIMD4<Float>
    public var size:SIMD2<Float>
    public var packedColor:UInt32
    public var reserved:UInt32=0
    public init(anchor:V3,angle:Float,height:Float,width:Float,color:V3) {
        anchorAngle=SIMD4(anchor,angle);size=SIMD2(height,width)
        func pack(_ x:Float)->UInt32 {UInt32((clamp(x,0,1)*1023).rounded())}
        packedColor=pack(color.x)|(pack(color.y)<<10)|(pack(color.z)<<20)
    }
    /// Indexed/reference path and conservative patch construction use the same
    /// five source vertices as direct mesh-shader expansion.
    public var vertices:[Vertex] {
        let p=V3(anchorAngle.x,anchorAngle.y,anchorAngle.z),angle=anchorAngle.w,h=size.x,w=size.y
        let d=V3(cos(angle)*w,0,sin(angle)*w),n=normalize(V3(sin(angle)*0.3,0.9,-cos(angle)*0.3))
        let bend=V3(cos(angle+0.8),0,sin(angle+0.8))*h*0.32
        let middle=p+V3(0,h*0.55,0)+bend*0.3,tip=p+V3(0,h,0)+bend
        let c=V3(Float(packedColor&1023),Float((packedColor>>10)&1023),Float((packedColor>>20)&1023))/1023
        return [Vertex(p-d,n,c),Vertex(p+d,n,c),Vertex(middle+d*0.55,n,c*1.08,wind:0.4),Vertex(middle-d*0.55,n,c*1.08,wind:0.4),Vertex(tip,n,c*1.18,wind:1)]
    }
    public static let triangleIndices:[UInt32]=[0,1,2,0,2,3,3,2,4]
}
