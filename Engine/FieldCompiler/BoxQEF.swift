import simd
import FieldCore

/// The regularized quadratic is positive definite. Its interior minimizer is
/// also the constrained minimum; only exterior solutions need active-set search.
enum BoxQEF {
    static func solve(_ A:simd_float3x3,_ b:V3,mass:V3,lo:V3,hi:V3,exhaustive:Bool=false)->V3 {
        let interior=mass+A.inverse*b
        if !exhaustive && all(interior .>= lo) && all(interior .<= hi) {return interior}
        var best=simd_clamp(mass,lo,hi),bestError=Float.greatestFiniteMagnitude
        for sx in -1...1 {for sy in -1...1 {for sz in -1...1 {
            let state=SIMD3(sx,sy,sz);var M=A,rhs=b,q=V3.zero
            for axis in 0..<3 where state[axis] != 0 {q[axis]=(state[axis]<0 ? lo[axis]:hi[axis])-mass[axis]}
            for axis in 0..<3 where state[axis] != 0 {
                for row in 0..<3 where state[row]==0 {rhs[row]-=A[axis][row]*q[axis]}
                for j in 0..<3 {M[axis][j]=0;M[j][axis]=0}
                M[axis][axis]=1;rhs[axis]=q[axis]
            }
            let candidate=mass+M.inverse*rhs
            guard all(candidate .>= lo-V3(repeating:1e-6)),all(candidate .<= hi+V3(repeating:1e-6)),candidate.x.isFinite,candidate.y.isFinite,candidate.z.isFinite else {continue}
            let d=candidate-mass,error=dot(d,A*d)-2*dot(d,b)
            if error<bestError {best=candidate;bestError=error}
        }}}
        return best
    }
}
