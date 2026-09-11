import Foundation
import simd
import FieldCore
import CGeometry

public enum MeshProcessing {
    public static func finish(_ input:Mesh,shape:Shape)->Mesh {
        var mesh=input
        // Keep shading discontinuities at hard CSG edges without disconnecting geometry.
        for i in stride(from:0,to:input.indices.count,by:3) {
            let ids=(0..<3).map{Int(input.indices[i+$0])},p=ids.map{V3(input.vertices[$0].position.x,input.vertices[$0].position.y,input.vertices[$0].position.z)}
            let face=shape.normal(at:(p[0]+p[1]+p[2])/3)
            for j in 0..<3 {
                let v=input.vertices[ids[j]],normal=V3(v.normal.x,v.normal.y,v.normal.z)
                if dot(face,normal)<0.8 {var split=v;split.normal=SIMD4(face,v.normal.w);mesh.indices[i+j]=UInt32(mesh.vertices.count);mesh.vertices.append(split)}
            }
        }
        return mesh
    }

    public static func optimize(_ input:Mesh)->Mesh {
        guard !input.indices.isEmpty else{return input}
        var mesh=input;let count=mesh.vertices.count,indexCount=mesh.indices.count
        let used=mesh.vertices.withUnsafeMutableBytes {v in mesh.indices.withUnsafeMutableBufferPointer {i in sgOptimize(v.baseAddress!,count,i.baseAddress!,indexCount)}}
        mesh.vertices.removeLast(mesh.vertices.count-used);return mesh
    }
    public static func simplify(_ input:Mesh,ratio:Float,error:Float)->Mesh {
        var mesh=input,indices=Array(repeating:UInt32(0),count:input.indices.count),measured:Float=0
        let count=input.vertices.withUnsafeBytes {v in input.indices.withUnsafeBufferPointer {i in sgSimplify(&indices,i.baseAddress!,i.count,v.baseAddress!,input.vertices.count,ratio,error,&measured)}}
        mesh.indices=Array(indices.prefix(count));mesh.geometricError=measured
        return optimize(mesh)
    }
}

public struct MeshletData {
    public var descriptors:[SIMD4<UInt32>]=[]
    public var spheres:[SIMD4<Float>]=[]
    public var vertices:[UInt32]
    public var triangles:[UInt8]
    public init(_ mesh:Mesh) {
        let bound=sgMeshletBound(mesh.indices.count)
        var out=Array(repeating:SGMeshlet(),count:bound)
        vertices=Array(repeating:0,count:bound*64);triangles=Array(repeating:0,count:bound*124*3)
        let count=mesh.vertices.withUnsafeBytes {p in mesh.indices.withUnsafeBufferPointer {i in sgMeshlets(&out,&vertices,&triangles,i.baseAddress!,i.count,p.baseAddress!,mesh.vertices.count)}}
        for m in out.prefix(count) {descriptors.append(SIMD4(m.vertexOffset,m.triangleOffset,m.vertexCount,m.triangleCount));spheres.append(SIMD4(m.x,m.y,m.z,m.radius))}
        if let m=out.prefix(count).last {vertices=Array(vertices.prefix(Int(m.vertexOffset+m.vertexCount)));triangles=Array(triangles.prefix(Int(m.triangleOffset+m.triangleCount*3)))}
    }
}
