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
        if report.manifold && report.invertedTriangles==0 {var mesh=candidate;mesh.report=report;return MeshProcessing.optimize(MeshProcessing.finish(mesh,shape:shape))}
        // Ambiguous cells can contain multiple surface components. Preserve the
        // reference extractor until multi-component dual cells are implemented.
        // Cell-clamped QEF vertices can also fold a quad even when its indexed
        // edges are closed; sampled orientation is a separate rejection gate.
        let fallbackStart=Date()
        let recovered=try referenceRecovery(shape,resolution:n,color:color,bounds:bounds)
        var result=MeshProcessing.optimize(MeshProcessing.finish(recovered.mesh,shape:shape))
        var fallbackReport=report;fallbackReport.usedReferenceFallback=true
        fallbackReport.referenceRefinedVertices=recovered.refinedVertices
        fallbackReport.referenceHeldVertices=recovered.heldVertices
        fallbackReport.fallbackMilliseconds=Date().timeIntervalSince(fallbackStart)*1000
        result.report=fallbackReport
        return result
    }
    /// Resample a continuous source at a spatially authored volume resolution.
    /// The conforming tetrahedral grid shares the reference polygon extractor.
    public static func sampled(_ shape:Shape,policy:SurfaceSampling,color:V3=V3(repeating:1),bounds:Bounds?=nil) throws -> Mesh {
        try sampledResult(shape,policy:policy,color:color,bounds:bounds).mesh
    }
    public static func sampledResult(_ shape:Shape,policy:SurfaceSampling,color:V3=V3(repeating:1),bounds:Bounds?=nil) throws -> SampledSurface {
        let start=Date(),extractionBounds=(bounds ?? shape.bounds).expanded(0.03)
        var recovery:[SurfaceSamplingRegion]=[]
        for attempt in 0...2 {
            let grid=try AdaptiveTetrahedra.build(shape,bounds:extractionBounds,policy:policy,recovery:recovery)
            var surface=TetrahedralSurface(field:shape,color:color,refine:true)
            for cell in grid.cells {
                let ids=(0..<4).map{cell.vertices[$0]}
                surface.append(ids:ids,points:ids.map{grid.points[$0]},values:ids.map{grid.values[$0]})
            }
            let result=surface.finish(),cleaned=SamplingGeometry.removeZeroAreaTriangles(result.mesh)
            let mesh=MeshProcessing.optimize(MeshProcessing.finish(cleaned.mesh,shape:shape))
            let audit=SamplingGeometry.audit(mesh,field:shape)
            if audit.degenerateTriangles==0 && audit.orientedEdgeIncidence && audit.inwardSamples.isEmpty {
                let report=SamplingReport(volumeVertices:grid.points.count,tetrahedra:grid.cells.count,
                    cellSubdivisions:grid.subdivisions,transitionFaces:grid.transitionFaces,recoveryPasses:attempt,
                    zeroClassifiedVertices:grid.zeroClassifiedVertices,maximumLinearizedZeroDistance:grid.maximumLinearizedZeroDistance,
                    surfaceVertices:mesh.vertices.count,triangles:mesh.indices.count/3,
                    refinedRoots:result.refinedVertices,heldRoots:result.heldVertices,
                    orientedEdgeIncidence:audit.orientedEdgeIncidence,degenerateTriangles:audit.degenerateTriangles,
                    removedDegenerateTriangles:cleaned.removed,
                    sampledInvertedTriangles:audit.inwardSamples.count,milliseconds:Date().timeIntervalSince(start)*1000)
                return SampledSurface(mesh:mesh,report:report)
            }
            // A sampled orientation defect requests finer source evaluation near
            // its actual location. Keep authorial fields/density bounds intact;
            // each recovery remains under the original explicit volume budget.
            let locations=audit.inwardSamples+audit.badEdgeSamples
            if attempt<2 && audit.degenerateTriangles==0 && !locations.isEmpty && locations.count<=64 {
                for point in locations {
                    let edge=policy.edgeLength(in:Bounds(point,point)),scale:Float=attempt==0 ? 0.5:0.25
                    recovery.append(SurfaceSamplingRegion(id:"recovery-\(recovery.count)",center:point,
                        radius:V3(repeating:edge*1.5),edgeLength:max(0.001,edge*scale)))
                }
                continue
            }
            throw CraftError(path:"sampling.geometry",reason:"Surface extraction after \(attempt) recovery passes has \(audit.degenerateTriangles) degenerate triangles, closed oriented geometric edges: \(audit.orientedEdgeIncidence), \(audit.inwardSamples.count) sampled inward faces at \(Array(audit.inwardSamples.prefix(8))). Bad edge samples: \(Array(audit.badEdgeSamples.prefix(8))). Refine these regions or correct the source; no partial mesh was installed")
        }
        throw CraftError(path:"sampling.geometry",reason:"Source resampling did not converge")
    }

    /// Marching tetrahedra: a deliberately small reference compiler. The same
    /// tetrahedral split is used in every cell, keeping shared-face edges coherent.
    public static func reference(_ shape: Shape, resolution: Int = 24, color: V3 = V3(repeating:1), bounds suppliedBounds:Bounds? = nil) throws -> Mesh {
        try referenceExtraction(shape,resolution:resolution,color:color,bounds:suppliedBounds,refine:false).mesh
    }
    public struct ReferenceRecovery:Sendable {
        public var mesh:Mesh
        public var refinedVertices:Int
        public var heldVertices:Int
    }
    /// Refine canonical lattice-edge intersections against the actual field.
    /// Every copy of an edge point moves together. Restore offending points to
    /// the linear reference if a triangle would collapse or reverse. This is a
    /// bounded representation recovery, not adaptive extraction or topology repair.
    public static func referenceRecovery(_ shape:Shape,resolution:Int=24,color:V3=V3(repeating:1),bounds:Bounds?=nil) throws -> ReferenceRecovery {
        try referenceExtraction(shape,resolution:resolution,color:color,bounds:bounds,refine:true)
    }
    static func referenceExtraction(_ shape:Shape,resolution:Int,color:V3,bounds suppliedBounds:Bounds?,refine:Bool,sparse:Bool=true) throws -> ReferenceRecovery {
        try shape.validate()
        guard (4...128).contains(resolution) else { throw FieldError.invalidParameter }
        let bounds = (suppliedBounds ?? shape.bounds).expanded(0.03), n = resolution
        let step = (bounds.max-bounds.min)/Float(n)
        let corners = [SIMD3<Int>(0,0,0),SIMD3(1,0,0),SIMD3(1,1,0),SIMD3(0,1,0),SIMD3(0,0,1),SIMD3(1,0,1),SIMD3(1,1,1),SIMD3(0,1,1)]
        let tetrahedra = [[0,5,1,6],[0,1,2,6],[0,2,3,6],[0,3,7,6],[0,7,4,6],[0,4,5,6]]
        var values = [Float](repeating:.nan,count:(n+1)*(n+1)*(n+1))
        func offset(_ x: Int,_ y: Int,_ z: Int) -> Int { (z*(n+1)+y)*(n+1)+x }
        func gridPoint(_ x:Int,_ y:Int,_ z:Int)->V3 {bounds.min+V3(Float(x),Float(y),Float(z))*step}
        // Exclude only blocks whose conservative field interval cannot contain
        // zero. Surviving blocks retain the identical global tetrahedral lattice,
        // canonical edge IDs and visitation order of the dense reference.
        let blockSize=8,blockCount=(n+blockSize-1)/blockSize
        var blocks:Set<Int>=[]
        func blockKey(_ x:Int,_ y:Int,_ z:Int)->Int {(z*blockCount+y)*blockCount+x}
        for z in 0..<blockCount {for y in 0..<blockCount {for x in 0..<blockCount {
            let region=Bounds(gridPoint(x*blockSize,y*blockSize,z*blockSize),
                gridPoint(min(n,(x+1)*blockSize),min(n,(y+1)*blockSize),min(n,(z+1)*blockSize))).expanded(1e-5)
            let interval=shape.valueInterval(in:region),allowance:Float=1e-6*(1+abs(interval.lower)+abs(interval.upper))
            if sparse && (interval.lower>allowance || interval.upper < -allowance) {continue}
            blocks.insert(blockKey(x,y,z))
        }}}
        func value(_ x:Int,_ y:Int,_ z:Int,_ field:Shape)->Float {
            let index=offset(x,y,z)
            if values[index].isNaN {values[index]=field.value(at:gridPoint(x,y,z))}
            return values[index]
        }
        var surface=TetrahedralSurface(field:shape,color:color,refine:refine)
        for z in 0..<n {for y in 0..<n {for x in 0..<n {
            guard blocks.contains(blockKey(x/blockSize,y/blockSize,z/blockSize)) else {continue}
            let ps=corners.map{gridPoint(x+$0.x,y+$0.y,z+$0.z)}
            let ds=corners.map{value(x+$0.x,y+$0.y,z+$0.z,shape)}
            if ds.allSatisfy({$0>=0}) || ds.allSatisfy({$0<0}) {continue}
            let ids=corners.map{offset(x+$0.x,y+$0.y,z+$0.z)}
            for t in tetrahedra {surface.append(ids:t.map{ids[$0]},points:t.map{ps[$0]},values:t.map{ds[$0]})}
        }}}
        return surface.finish()
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
