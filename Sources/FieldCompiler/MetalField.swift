import Foundation
import FieldCore

/// Emits the same typed graph used by meshing and collision as Metal functions.
/// The graph is shared by compute verification and procedural intersections.
public enum MetalField {
    public static func source(for shape:Shape) throws -> String {
        let lib = try library(for:shape,prefix:"field")
        return "#include <metal_stdlib>\nusing namespace metal;\n"+lib.source+"\nkernel void evaluateField(const device float4 *points [[buffer(0)]], device float *values [[buffer(1)]], uint id [[thread_position_in_grid]]) { values[id]=\(lib.root)(points[id].xyz); }\n"
    }
    public static func library(for shape:Shape,prefix:String) throws -> (source:String,root:String) {
        try shape.validate()
        var functions:[String]=[],nextID=0
        func f(_ x:Float)->String {String(x)}
        func vector(_ p:V3)->String {"float3(\(f(p.x)),\(f(p.y)),\(f(p.z)))"}
        func emit(_ node:Shape)->String {
            let name="\(prefix)_\(nextID)";nextID+=1
            let body:String
            switch node {
            case let .sphere(r):body="return length(p)-\(f(r));"
            case let .box(b):body="float3 q=abs(p)-\(vector(b)); return length(max(q,0.0))+min(max(q.x,max(q.y,q.z)),0.0);"
            case let .capsule(a,b,r):body="float3 pa=p-\(vector(a)), ba=\(vector(b-a)); float len=dot(ba,ba); float h=len>0 ? clamp(dot(pa,ba)/len,0.0,1.0):0.0; return length(pa-ba*h)-\(f(r));"
            case let .torus(r,t):body="return length(float2(length(p.xz)-\(f(r)),p.y))-\(f(t));"
            case let .translated(a,p):body="return \(emit(a))(p-\(vector(p)));"
            case let .scaled(a,s):body="return \(emit(a))(p/\(f(s)))*\(f(s));"
            case let .stretched(a,s):body="return \(emit(a))(p/\(vector(s)))*\(f(min(s.x,min(s.y,s.z))));"
            case let .union(a,b):body="return min(\(emit(a))(p),\(emit(b))(p));"
            case let .subtract(a,b):body="return max(\(emit(a))(p),-\(emit(b))(p));"
            case let .smoothUnion(a,b,k):body="float x=\(emit(a))(p), y=\(emit(b))(p); float h=clamp(0.5+0.5*(y-x)/\(f(k)),0.0,1.0); return y+(x-y)*h-\(f(k))*h*(1-h);"
            }
            functions.append("float \(name)(float3 p) { \(body) }")
            return name
        }
        let root=emit(shape)
        return (functions.joined(separator:"\n"),root)
    }
}
