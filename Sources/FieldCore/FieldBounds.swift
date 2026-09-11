import Foundation

/// Convex-hull certificates for a polynomial represented in Bernstein form.
/// A same-sign coefficient hull excludes roots over the entire interval.
public struct BernsteinPolynomial {
    public let coefficients:[Float]
    public init(_ coefficients:[Float]) {precondition(!coefficients.isEmpty);self.coefficients=coefficients}
    public var range: ClosedRange<Float> {coefficients.min()!...coefficients.max()!}
    public var excludesZero:Bool {range.lowerBound>0 || range.upperBound<0}
    public func split()->(BernsteinPolynomial,BernsteinPolynomial) {
        var row=coefficients,left=[row.first!],right=[row.last!]
        while row.count>1 {row=zip(row,row.dropFirst()).map{($0+$1)*0.5};left.append(row.first!);right.append(row.last!)}
        return (BernsteinPolynomial(left),BernsteinPolynomial(right.reversed()))
    }
}

private struct Interval {
    var lo:Float;var hi:Float
    init(_ lo:Float,_ hi:Float) {self.lo=lo;self.hi=hi}
    static func + (a:Self,b:Self)->Self {Self(a.lo+b.lo,a.hi+b.hi)}
    static func * (a:Self,b:Float)->Self {b>=0 ? Self(a.lo*b,a.hi*b):Self(a.hi*b,a.lo*b)}
    static func * (a:Self,b:Self)->Self {let v=[a.lo*b.lo,a.lo*b.hi,a.hi*b.lo,a.hi*b.hi];return Self(v.min()!,v.max()!)}
    var squared:Self {Self(lo<=0 && hi>=0 ? 0:min(lo*lo,hi*hi),max(lo*lo,hi*hi))}
    var exp:Self {Self(Foundation.exp(lo),Foundation.exp(hi))}
    var sin:Self {
        if hi-lo>=2*Float.pi {return Self(-1,1)}
        var a=min(Foundation.sin(lo),Foundation.sin(hi)),b=max(Foundation.sin(lo),Foundation.sin(hi))
        if ceil((lo-Float.pi/2)/(2*Float.pi)) <= floor((hi-Float.pi/2)/(2*Float.pi)) {b=1}
        if ceil((lo+Float.pi/2)/(2*Float.pi)) <= floor((hi+Float.pi/2)/(2*Float.pi)) {a = -1}
        return Self(a,b)
    }
    var cos:Self {(self+Self(Float.pi/2,Float.pi/2)).sin}
    var magnitude:Float {max(abs(lo),abs(hi))}
}

extension Terrain {
    /// Certified height and derivative ranges for one patch. The polynomial
    /// flank uses Bernstein coefficients; transcendental terms use intervals.
    public func patchCertificate(x:ClosedRange<Float>,z:ClosedRange<Float>)->(height:ClosedRange<Float>,dx:Float,dz:Float) {
        let X=Interval(x.lowerBound,x.upperBound),Z=Interval(z.lowerBound,z.upperBound)
        let phase=X*0.055+Z*0.019,phase2=X*0.13+Z*(-0.08)
        let hx=X+Interval(34,34),hz=Z+Interval(45,45),rz=Z+Interval(122,122)
        let H=(hx.squared*(-1/700)+hz.squared*(-1/1400)).exp
        let R=(rz.squared*(-1/450)).exp
        let wave=Interval(0.55,0.55)+(X*0.066+Interval(0.7,0.7)).sin*0.45
        let flank=BernsteinPolynomial([x.lowerBound*x.lowerBound/375,x.lowerBound*x.upperBound/375,x.upperBound*x.upperBound/375]).range
        let height=phase.sin*0.8+(Z*0.045).sin*1.25+phase2.cos*0.42+Interval(max(0,flank.lowerBound),flank.upperBound)+H*8+R*wave.squared*17
        let dx=phase.cos.magnitude*0.044+phase2.sin.magnitude*0.0546+X.magnitude*(2/375)+(hx*H).magnitude*(16/700)+(R*wave*(X*0.066+Interval(0.7,0.7)).cos).magnitude*(17*0.9*0.066)
        let dz=phase.cos.magnitude*0.0152+(Z*0.045).cos.magnitude*0.05625+phase2.sin.magnitude*0.0336+(hz*H).magnitude*(16/1400)+(rz*R*wave.squared).magnitude*(34/450)
        // Outward allowance covers Float evaluation and interval arithmetic rounding.
        return ((height.lo-0.002)...(height.hi+0.002),dx*1.001+0.0001,dz*1.001+0.0001)
    }
}
