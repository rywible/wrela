import simd

extension Shape {
    /// All supported fields are at most 1-Lipschitz, including conservative
    /// nonuniform stretches. Only remove a CSG branch when its entire regional
    /// value interval loses, with an outward floating-point allowance.
    public func specialized(in region:Bounds)->Shape {
        func separated(_ a:Shape,_ b:Shape,band:Float)->Int {
            let x=a.valueInterval(in:region),y=b.valueInterval(in:region)
            let allowance:Float=1e-5*(1+abs(x.lower)+abs(x.upper)+abs(y.lower)+abs(y.upper)+band)
            if x.upper+band+allowance < y.lower {return -1}
            if y.upper+band+allowance < x.lower {return 1}
            return 0
        }
        switch self {
        case let .union(a,b):
            switch separated(a,b,band:0) {
            case -1:return a.specialized(in:region)
            case 1:return b.specialized(in:region)
            default:return .union(a.specialized(in:region),b.specialized(in:region))
            }
        case let .smoothUnion(a,b,k):
            switch separated(a,b,band:k) {
            case -1:return a.specialized(in:region)
            case 1:return b.specialized(in:region)
            default:return .smoothUnion(a.specialized(in:region),b.specialized(in:region),k)
            }
        case let .subtract(a,b):return .subtract(a.specialized(in:region),b.specialized(in:region))
        case let .smoothSubtract(a,b,k):return .smoothSubtract(a.specialized(in:region),b.specialized(in:region),k)
        case let .translated(a,t):return .translated(a.specialized(in:Bounds(region.min-t,region.max-t)),t)
        case let .scaled(a,s):return .scaled(a.specialized(in:Bounds(region.min/s,region.max/s)),s)
        case let .stretched(a,s):return .stretched(a.specialized(in:Bounds(region.min/s,region.max/s)),s)
        case let .rotated(a,q):
            let inverse=q.inverse,center=inverse.act((region.min+region.max)/2),half=(region.max-region.min)/2,m=simd_float3x3(inverse)
            let extent=abs(m[0])*half.x+abs(m[1])*half.y+abs(m[2])*half.z
            return .rotated(a.specialized(in:Bounds(center-extent,center+extent)),q)
        default:return self
        }
    }
    public var operationCount:Int {
        switch self {
        case let .union(a,b),let .subtract(a,b),let .smoothUnion(a,b,_),let .smoothSubtract(a,b,_):return 1+a.operationCount+b.operationCount
        case let .translated(a,_),let .scaled(a,_),let .stretched(a,_),let .rotated(a,_):return 1+a.operationCount
        default:return 1
        }
    }
}
