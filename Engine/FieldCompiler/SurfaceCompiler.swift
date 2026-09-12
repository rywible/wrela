import Foundation
import simd
import FieldCore

public struct SurfaceReport: Sendable {
    public var fieldEvaluations=0
    public var specializedRegions=0
    public var prunedOperations=0
    public var edgeCacheHits=0
    public var solvedEdges=0
    public var gradientEvaluations=0
    public var usedReferenceFallback=false
    public var fallbackMilliseconds:Double=0
    public var excludedCells=0
    public var surfaceCells=0
    public var vertices=0
    public var triangles=0
    public var maximumSampleResidual:Float=0
    public var manifold=false
    public var milliseconds:Double=0
}

/// Sparse dual contouring. Empty octants are excluded using the graph's global
/// 1-Lipschitz bound; all surviving leaves share a lattice, avoiding T junctions.
/// Different detail levels are compiled separately and never joined spatially.
public enum SurfaceCompiler {
    private struct Edge:Hashable {var a:Int;var b:Int;init(_ a:Int,_ b:Int){self.a=min(a,b);self.b=max(a,b)}}
    public static func compile(_ shape:Shape,resolution:Int,color:V3,bounds suppliedBounds:Bounds? = nil)->(Mesh,SurfaceReport) {
        let start=Date(),n=resolution,stride=n+1
        let raw=suppliedBounds ?? shape.bounds,extent=raw.max-raw.min
        let padding=max(extent.x,max(extent.y,extent.z))*0.0137
        let bounds=raw.expanded(padding),step=(bounds.max-bounds.min)/Float(n)
        var samples:[Int:Float]=[:],cells:[Int:UInt32]=[:],edges:[Edge:(point:V3,normal:V3)]=[:],mesh=Mesh(),report=SurfaceReport()
        func key(_ p:SIMD3<Int>)->Int {(p.z*stride+p.y)*stride+p.x}
        func grid(_ i:Int)->SIMD3<Int>{SIMD3(i%stride,(i/stride)%stride,i/(stride*stride))}
        func point(_ p:SIMD3<Int>)->V3 {bounds.min+V3(Float(p.x),Float(p.y),Float(p.z))*step}
        func value(_ p:SIMD3<Int>,_ field:Shape)->Float {
            let k=key(p);if let v=samples[k]{return v}
            let v=field.value(at:point(p));samples[k]=v;report.fieldEvaluations+=1;return v
        }
        let corners=[SIMD3<Int>(0,0,0),SIMD3(1,0,0),SIMD3(1,1,0),SIMD3(0,1,0),SIMD3(0,0,1),SIMD3(1,0,1),SIMD3(1,1,1),SIMD3(0,1,1)]
        let pairs=[(0,1),(1,2),(2,3),(3,0),(4,5),(5,6),(6,7),(7,4),(0,4),(1,5),(2,6),(3,7)]
        func leaf(_ p:SIMD3<Int>,_ field:Shape) {
            let ds=corners.map{value(p &+ $0,field)}
            guard ds.contains(where:{$0<0}) && ds.contains(where:{$0>=0}) else{return}
            var hits:[V3]=[],normals:[V3]=[]
            for (a,b) in pairs where (ds[a]<0) != (ds[b]<0) {
                let ga=p &+ corners[a],gb=p &+ corners[b]
                let edge=Edge(key(ga),key(gb))
                if let hit=edges[edge] {
                    hits.append(hit.point);normals.append(hit.normal);report.edgeCacheHits+=1
                    continue
                }
                // Canonical orientation makes shared-edge solving independent
                // of which incident cell reaches the edge first.
                var lo=point(grid(edge.a)),hi=point(grid(edge.b)),dlo=value(grid(edge.a),field),dhi=value(grid(edge.b),field)
                let tolerance=length(hi-lo)*1e-5
                for _ in 0..<12 {
                    if length(hi-lo)<=tolerance {break}
                    let t=clamp(dlo/(dlo-dhi),0.1,0.9),q=lo+(hi-lo)*t,v=field.value(at:q)
                    report.fieldEvaluations+=1
                    if (v<0)==(dlo<0){lo=q;dlo=v}else{hi=q;dhi=v}
                }
                let hit=lo+(hi-lo)*clamp(dlo/(dlo-dhi),0,1),normal=field.normal(at:hit)
                hits.append(hit);normals.append(normal);edges[edge]=(hit,normal)
                report.solvedEdges+=1;report.gradientEvaluations+=1
            }
            let mass=hits.reduce(V3.zero,+)/Float(hits.count),lo=point(p),hi=lo+step
            var A=simd_float3x3(0),b=V3.zero
            for (q,norm) in zip(hits,normals) {
                A += simd_float3x3(columns:(norm*norm.x,norm*norm.y,norm*norm.z));b+=norm*dot(norm,q-mass)
            }
            // A small mass-point regularizer resolves planar/rank-deficient cells.
            let lambda:Float=0.0001
            A += matrix_identity_float3x3*lambda
            let best=BoxQEF.solve(A,b,mass:mass,lo:lo,hi:hi)
            report.gradientEvaluations+=1
            cells[key(p)]=UInt32(mesh.vertices.count)
            mesh.vertices.append(Vertex(best,field.normal(at:best),color));report.surfaceCells+=1
        }
        func visit(_ p:SIMD3<Int>,_ width:Int,_ field:Shape) {
            let center=point(p)+step*Float(width)*0.5,radius=length(step*Float(width))*0.5
            report.fieldEvaluations+=1
            if abs(field.value(at:center))>radius*1.00001+1e-6 {report.excludedCells+=1;return}
            if width==1 {leaf(p,field);return}
            let regional:Shape
            if width==8 {
                regional=field.specialized(in:Bounds(point(p),point(p)+step*Float(width)).expanded(1e-5))
                report.specializedRegions+=1;report.prunedOperations+=field.operationCount-regional.operationCount
            } else {regional=field}
            let half=width/2
            for c in corners {visit(p &+ c &* half,half,regional)}
        }
        visit(.zero,n,shape)
        for e in edges.keys.sorted(by:{$0.a==$1.a ? $0.b<$1.b:$0.a<$1.a}) {
            let p=grid(e.a),q=grid(e.b),delta=q &- p
            let ring:[SIMD3<Int>]
            if delta.x != 0 {ring=[p &+ SIMD3(0,-1,-1),p &+ SIMD3(0,0,-1),p,p &+ SIMD3(0,-1,0)]}
            else if delta.y != 0 {ring=[p &+ SIMD3(-1,0,-1),p &+ SIMD3(-1,0,0),p,p &+ SIMD3(0,0,-1)]}
            else {ring=[p &+ SIMD3(-1,-1,0),p &+ SIMD3(0,-1,0),p,p &+ SIMD3(-1,0,0)]}
            var ids=ring.compactMap{cells[key($0)]};guard ids.count==4 else{continue}
            if value(p,shape)>=0 {ids.reverse()}
            let v=ids.map{mesh.vertices[Int($0)].position}
            if simd_length_squared(v[0]-v[2])<=simd_length_squared(v[1]-v[3]) {mesh.indices += [ids[0],ids[1],ids[2],ids[0],ids[2],ids[3]]}
            else {mesh.indices += [ids[0],ids[1],ids[3],ids[1],ids[2],ids[3]]}
        }
        report.manifold=isClosed(mesh)
        for v in mesh.vertices {report.maximumSampleResidual=max(report.maximumSampleResidual,abs(shape.value(at:V3(v.position.x,v.position.y,v.position.z))))}
        report.vertices=mesh.vertices.count;report.triangles=mesh.indices.count/3;report.milliseconds=Date().timeIntervalSince(start)*1000
        return (mesh,report)
    }
    public static func isClosed(_ mesh:Mesh)->Bool {
        var edges:[Edge:(Int,Int)]=[:]
        for i in stride(from:0,to:mesh.indices.count,by:3) {
            let ids=[Int(mesh.indices[i]),Int(mesh.indices[i+1]),Int(mesh.indices[i+2])]
            for j in 0..<3 {let a=ids[j],b=ids[(j+1)%3],key=Edge(a,b),old=edges[key] ?? (0,0);edges[key]=(old.0+1,old.1+(a<b ? 1:-1))}
        }
        return !edges.isEmpty && edges.values.allSatisfy{$0.0==2 && $0.1==0}
    }
}
