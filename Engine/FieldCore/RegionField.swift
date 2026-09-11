import simd

extension Shape {
    /// All supported fields are at most 1-Lipschitz, including conservative
    /// nonuniform stretches. Only remove a CSG branch when its entire regional
    /// value interval loses, with an outward floating-point allowance.
    public func specialized(in region:Bounds)->Shape {
        let center=(region.min+region.max)/2,radius=length(region.max-region.min)/2
        func separated(_ a:Shape,_ b:Shape,band:Float)->Int {
            let x=a.value(at:center),y=b.value(at:center)
            let allowance:Float=1e-5*(1+abs(x)+abs(y)+radius+band)
            if x+radius+band+allowance < y-radius {return -1}
            if y+radius+band+allowance < x-radius {return 1}
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
        case let .translated(a,t):return .translated(a.specialized(in:Bounds(region.min-t,region.max-t)),t)
        case let .scaled(a,s):return .scaled(a.specialized(in:Bounds(region.min/s,region.max/s)),s)
        case let .stretched(a,s):return .stretched(a.specialized(in:Bounds(region.min/s,region.max/s)),s)
        default:return self
        }
    }
    public var operationCount:Int {
        switch self {
        case let .union(a,b),let .subtract(a,b),let .smoothUnion(a,b,_):return 1+a.operationCount+b.operationCount
        case let .translated(a,_),let .scaled(a,_),let .stretched(a,_):return 1+a.operationCount
        default:return 1
        }
    }
}
