import Foundation
import simd

public typealias V3 = SIMD3<Float>

public struct Bounds: Sendable {
    public var min: V3
    public var max: V3
    public init(_ min: V3, _ max: V3) { self.min = min; self.max = max }
    public func union(_ other: Bounds) -> Bounds { Bounds(simd_min(min, other.min), simd_max(max, other.max)) }
    public func expanded(_ d: Float) -> Bounds { Bounds(min - V3(repeating: d), max + V3(repeating: d)) }
    public func contains(_ p: V3) -> Bool { all(p .>= min) && all(p .<= max) }
}

/// Exact primitives become conservative estimates under composition. Terrain is
/// explicitly a general implicit field and must never be used as a march distance.
public enum FieldSemantics: String, Sendable { case exactDistance, distanceBound, implicit }

public indirect enum Shape: Sendable {
    case sphere(Float)
    case box(V3)
    case capsule(V3, V3, Float)
    case torus(Float, Float)
    case translated(Shape, V3)
    case scaled(Shape, Float)
    case stretched(Shape, V3)
    case union(Shape, Shape)
    case subtract(Shape, Shape)
    case smoothUnion(Shape, Shape, Float)

    public func moved(_ p: V3) -> Shape { .translated(self, p) }
    public func sized(_ s: Float) -> Shape { .scaled(self, s) }
    public func stretched(_ s: V3) -> Shape { .stretched(self, s) }
    public func joined(_ s: Shape) -> Shape { .union(self, s) }
    public func blended(_ s: Shape, radius: Float) -> Shape { .smoothUnion(self, s, radius) }
    public func cut(_ s: Shape) -> Shape { .subtract(self, s) }

    public var semantics: FieldSemantics {
        switch self {
        case .sphere, .box, .capsule, .torus: return .exactDistance
        case let .translated(a, _), let .scaled(a, _): return a.semantics
        default: return .distanceBound
        }
    }

    /// Lower ratio between field value and distance outside its enclosing box.
    /// Nonuniform stretching makes the value conservative, so a blend's field-
    /// space radius must be expanded by this ratio when computing spatial bounds.
    public var exteriorDistanceScale:Float {
        switch self {
        case let .translated(a,_),let .scaled(a,_),let .subtract(a,_):return a.exteriorDistanceScale
        case let .stretched(a,s):return a.exteriorDistanceScale*Swift.min(s.x,Swift.min(s.y,s.z))/Swift.max(s.x,Swift.max(s.y,s.z))
        case let .union(a,b),let .smoothUnion(a,b,_):return Swift.min(a.exteriorDistanceScale,b.exteriorDistanceScale)
        default:return 1
        }
    }

    public func validate() throws {
        func positive(_ x: Float) throws { if !x.isFinite || x <= 0 { throw FieldError.invalidParameter } }
        func finite(_ p: V3) throws { if !p.x.isFinite || !p.y.isFinite || !p.z.isFinite { throw FieldError.invalidParameter } }
        switch self {
        case let .sphere(r): try positive(r)
        case let .box(b): try positive(b.x); try positive(b.y); try positive(b.z)
        case let .capsule(a,b,r): try finite(a); try finite(b); try positive(r)
        case let .torus(r,t): try positive(r); try positive(t)
        case let .translated(a,p): try a.validate(); try finite(p)
        case let .scaled(a,s): try a.validate(); try positive(s)
        case let .stretched(a,s): try a.validate(); try positive(s.x); try positive(s.y); try positive(s.z)
        case let .union(a,b), let .subtract(a,b): try a.validate(); try b.validate()
        case let .smoothUnion(a,b,k): try a.validate(); try b.validate(); try positive(k)
        }
    }

    public func value(at p: V3) -> Float {
        switch self {
        case let .sphere(r): return length(p) - r
        case let .box(b):
            let q = abs(p) - b
            return length(simd_max(q, .zero)) + Swift.min(Swift.max(q.x, Swift.max(q.y,q.z)), 0)
        case let .capsule(a,b,r):
            let ba = b-a, pa = p-a
            let h = simd_length_squared(ba) > 0 ? clamp(dot(pa,ba)/dot(ba,ba), 0, 1) : 0
            return length(pa - ba*h) - r
        case let .torus(r,t): return length(SIMD2<Float>(length(SIMD2<Float>(p.x,p.z))-r,p.y))-t
        case let .translated(a,t): return a.value(at: p-t)
        case let .scaled(a,s): return a.value(at: p/s)*s
        case let .stretched(a,s): return a.value(at:p/s)*Swift.min(s.x,Swift.min(s.y,s.z))
        case let .union(a,b): return Swift.min(a.value(at:p), b.value(at:p))
        case let .subtract(a,b): return Swift.max(a.value(at:p), -b.value(at:p))
        case let .smoothUnion(a,b,k):
            let x = a.value(at:p), y = b.value(at:p)
            let h = clamp(0.5 + 0.5*(y-x)/k, 0, 1)
            return y + (x-y)*h - k*h*(1-h)
        }
    }

    public func normal(at p: V3) -> V3 {
        let e: Float = 0.002
        let n = V3(value(at:p+V3(e,0,0))-value(at:p-V3(e,0,0)),
                   value(at:p+V3(0,e,0))-value(at:p-V3(0,e,0)),
                   value(at:p+V3(0,0,e))-value(at:p-V3(0,0,e)))
        return length_squared(n) > 1e-10 ? normalize(n) : V3(0,1,0)
    }

    public var bounds: Bounds {
        switch self {
        case let .sphere(r): return Bounds(V3(repeating:-r), V3(repeating:r))
        case let .box(b): return Bounds(-b,b)
        case let .capsule(a,b,r): return Bounds(simd_min(a,b),simd_max(a,b)).expanded(r)
        case let .torus(r,t): return Bounds(V3(-r-t,-t,-r-t),V3(r+t,t,r+t))
        case let .translated(a,t): return Bounds(a.bounds.min+t,a.bounds.max+t)
        case let .scaled(a,s): return Bounds(a.bounds.min*s,a.bounds.max*s)
        case let .stretched(a,s): return Bounds(a.bounds.min*s,a.bounds.max*s)
        case let .union(a,b): return a.bounds.union(b.bounds)
        case let .subtract(a,_): return a.bounds
        case let .smoothUnion(a,b,k): return a.bounds.union(b.bounds).expanded(k/(4*Swift.min(a.exteriorDistanceScale,b.exteriorDistanceScale)))
        }
    }
}

public enum FieldError: Error { case invalidParameter }

public func clamp(_ x: Float, _ a: Float, _ b: Float) -> Float { Swift.min(b, Swift.max(a,x)) }
public func mix(_ a: Float, _ b: Float, _ t: Float) -> Float { a+(b-a)*t }

public struct Terrain: Sendable {
    public init() {}
    public var semantics: FieldSemantics { .implicit }
    public func pathX(_ z: Float) -> Float { sin(z*0.048)*9 + sin(z*0.105)*2 }
    public func height(_ x: Float, _ z: Float) -> Float {
        let base = 0.8*sin(x*0.055+z*0.019) + 1.25*sin(z*0.045) + 0.42*cos(x*0.13-z*0.08)
        let flank = pow(abs(x)/75, 2)*15
        let hill = 8*exp(-((x+34)*(x+34)/700 + (z+45)*(z+45)/1400))
        let ridge = 17*exp(-((z+122)*(z+122)/450))*pow(0.55+0.45*sin(x*0.066+0.7),2)
        return base + flank + hill + ridge
    }
    public func value(at p: V3) -> Float { p.y-height(p.x,p.z) }
    public func normal(_ x: Float, _ z: Float) -> V3 {
        let e: Float = 0.08
        return normalize(V3(height(x-e,z)-height(x+e,z), 2*e, height(x,z-e)-height(x,z+e)))
    }
}

public struct SeededRandom: Sendable, Codable {
    private var state: UInt64
    public init(seed: UInt64) { state = seed }
    public mutating func next() -> Float {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        return Float(UInt32(truncatingIfNeeded: state >> 32)) / Float(UInt32.max)
    }
    public mutating func range(_ a: Float, _ b: Float) -> Float { mix(a,b,next()) }
}
