import Foundation
import simd
import FieldCore

/// Packed in aligned float4s so the Swift / Metal ABI is explicit.
public struct Vertex: Sendable {
    public var position: SIMD4<Float>
    public var normal: SIMD4<Float>
    public var color: SIMD4<Float>
    /// Procedural strand phase, guide distance fraction, root coverage and
    /// strand length variation. A zero root coverage disables this channel.
    /// Separate from color.w, which remains authored ambient occlusion.
    public var groom: SIMD4<Float> = .zero
    public init(_ p: V3, _ n: V3, _ c: V3, wind: Float = 0) {
        position = SIMD4(p,1); normal = SIMD4(n,wind); color = SIMD4(c,1)
    }
}

public struct Mesh: Sendable {
    public var vertices: [Vertex] = []
    public var indices: [UInt32] = []
    public var geometricError:Float = 0
    public var report: SurfaceReport? = nil
    public init() {}
    public mutating func triangle(_ a: Vertex, _ b: Vertex, _ c: Vertex) {
        let i = UInt32(vertices.count)
        vertices += [a,b,c]; indices += [i,i+1,i+2]
    }
}

public enum Mesher {
    public static func compile(_ shape: Shape, resolution: Int = 24, color: V3 = V3(repeating:1), bounds:Bounds? = nil) throws -> Mesh {
        try shape.validate()
        guard (4...128).contains(resolution) else {throw FieldError.invalidParameter}
        var n=8;while n<resolution {n*=2}
        let (candidate,report)=SurfaceCompiler.compile(shape,resolution:n,color:color,bounds:bounds)
        if report.manifold {var mesh=candidate;mesh.report=report;return MeshProcessing.optimize(MeshProcessing.finish(mesh,shape:shape))}
        // Ambiguous cells can contain multiple surface components. Preserve the
        // reference extractor until multi-component dual cells are implemented.
        let fallbackStart=Date()
        var result=MeshProcessing.optimize(MeshProcessing.finish(try reference(shape,resolution:n,color:color,bounds:bounds),shape:shape))
        var fallbackReport=report;fallbackReport.usedReferenceFallback=true
        fallbackReport.fallbackMilliseconds=Date().timeIntervalSince(fallbackStart)*1000
        result.report=fallbackReport
        return result
    }
    /// Marching tetrahedra: a deliberately small reference compiler. The same
    /// tetrahedral split is used in every cell, keeping shared-face edges coherent.
    public static func reference(_ shape: Shape, resolution: Int = 24, color: V3 = V3(repeating:1), bounds suppliedBounds:Bounds? = nil) throws -> Mesh {
        try shape.validate()
        guard (4...128).contains(resolution) else { throw FieldError.invalidParameter }
        let bounds = (suppliedBounds ?? shape.bounds).expanded(0.03), n = resolution
        let step = (bounds.max-bounds.min)/Float(n)
        let corners = [SIMD3<Int>(0,0,0),SIMD3(1,0,0),SIMD3(1,1,0),SIMD3(0,1,0),SIMD3(0,0,1),SIMD3(1,0,1),SIMD3(1,1,1),SIMD3(0,1,1)]
        let tetrahedra = [[0,5,1,6],[0,1,2,6],[0,2,3,6],[0,3,7,6],[0,7,4,6],[0,4,5,6]]
        let edges = [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)]
        var values = [Float](repeating:0,count:(n+1)*(n+1)*(n+1))
        func offset(_ x: Int,_ y: Int,_ z: Int) -> Int { (z*(n+1)+y)*(n+1)+x }
        for z in 0...n { for y in 0...n { for x in 0...n {
            values[offset(x,y,z)] = shape.value(at:bounds.min+V3(Float(x),Float(y),Float(z))*step)
        }}}
        var mesh = Mesh()
        // Canonical lattice edges produce bit-identical positions in adjacent
        // tetrahedra, including the reference fallback's diagonal edges.
        var intersections:[UInt64:V3]=[:]
        for z in 0..<n { for y in 0..<n { for x in 0..<n {
            let ps = corners.map { bounds.min+V3(Float(x+$0.x),Float(y+$0.y),Float(z+$0.z))*step }
            let ds = corners.map { values[offset(x+$0.x,y+$0.y,z+$0.z)] }
            if ds.allSatisfy({$0 >= 0}) || ds.allSatisfy({$0 < 0}) { continue }
            for t in tetrahedra {
                var hits: [V3] = []
                for (i,j) in edges {
                    let a=t[i], b=t[j]
                    if (ds[a]<0) != (ds[b]<0) {
                        let ia=offset(x+corners[a].x,y+corners[a].y,z+corners[a].z),ib=offset(x+corners[b].x,y+corners[b].y,z+corners[b].z)
                        let key=UInt64(min(ia,ib))<<32 | UInt64(max(ia,ib))
                        if let p=intersections[key] {hits.append(p)}
                        else {let p=ps[a]+(ps[b]-ps[a])*(ds[a]/(ds[a]-ds[b]));intersections[key]=p;hits.append(p)}
                    }
                }
                guard hits.count >= 3 else { continue }
                let center = hits.reduce(V3.zero,+)/Float(hits.count)
                // The polygon is the zero plane of this tetrahedron's linear
                // interpolant. At a CSG crease, the exact field's gradient at the
                // polygon center can face the other way, incorrectly flipping it.
                let basis=simd_float3x3(columns:(ps[t[1]]-ps[t[0]],ps[t[2]]-ps[t[0]],ps[t[3]]-ps[t[0]]))
                let gradient=basis.transpose.inverse*V3(ds[t[1]]-ds[t[0]],ds[t[2]]-ds[t[0]],ds[t[3]]-ds[t[0]])
                let normal=normalize(gradient)
                let u = normalize(abs(normal.y)<0.9 ? cross(normal,V3(0,1,0)) : cross(normal,V3(1,0,0)))
                let v = cross(normal,u)
                hits.sort { atan2(dot($0-center,v),dot($0-center,u)) < atan2(dot($1-center,v),dot($1-center,u)) }
                for i in 1..<(hits.count-1) {
                    let points=[hits[0],hits[i],hits[i+1]]
                    let verts=points.map { Vertex($0,shape.normal(at:$0),color) }
                    mesh.triangle(verts[0],verts[1],verts[2])
                }
            }
        }}}
        return mesh
    }

    public static func terrain(_ field: some HeightField, extent: Float = 155, resolution: Int = 260) -> Mesh {
        var mesh=Mesh()
        for z in 0...resolution { for x in 0...resolution {
            let px = (Float(x)/Float(resolution)*2-1)*extent
            let pz = (Float(z)/Float(resolution)*2-1)*extent
            let sample=field.sample(px,pz)
            mesh.vertices.append(Vertex(V3(px,sample.height,pz),sample.normal,V3(0.37,0.54,0.24)))
        }}
        for z in 0..<resolution { for x in 0..<resolution {
            let a=UInt32(z*(resolution+1)+x), b=a+1, c=a+UInt32(resolution+1), d=c+1
            mesh.indices += [a,c,b,b,c,d]
        }}
        return mesh
    }
}
