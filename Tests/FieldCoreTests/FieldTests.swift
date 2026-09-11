import XCTest
import simd
@testable import FieldCore
import SanctuaryContent
@testable import FieldCompiler

final class FieldTests:XCTestCase {
    func testAnalyticDistancesAndTransformUnits() throws {
        let sphere=Shape.sphere(2).sized(3).moved(V3(10,0,0))
        XCTAssertEqual(sphere.value(at:V3(18,0,0)),2,accuracy:0.00001)
        XCTAssertEqual(sphere.value(at:V3(10,0,0)),-6,accuracy:0.00001)
        XCTAssertEqual(Shape.capsule(.zero,.zero,2).value(at:V3(3,0,0)),1,accuracy:0.00001)
        XCTAssertEqual(Shape.box(V3(1,2,3)).value(at:V3(1,2,3)),0,accuracy:0.00001)
    }
    func testInvalidDefinitionsAreRejected() {
        for shape:Shape in [.sphere(-1),.sphere(.nan),.sphere(1).sized(0),.box(V3(1,0,1)),.sphere(1).blended(.sphere(2),radius:0)] {
            XCTAssertThrowsError(try shape.validate())
            XCTAssertThrowsError(try Mesher.compile(shape))
        }
    }
    func testComposedBoundsContainInterior() throws {
        let field=Shape.sphere(1).moved(V3(-0.8,0,0)).blended(.sphere(1).moved(V3(0.8,0,0)),radius:1.4).sized(2)
        let bounds=field.bounds
        var rng=SeededRandom(seed:14)
        for _ in 0..<10000 {
            let p=V3(rng.range(-5,5),rng.range(-5,5),rng.range(-5,5))
            if field.value(at:p)<=0 {XCTAssertTrue(bounds.contains(p),"Missed interior at \(p)")}
        }
        XCTAssertEqual(field.semantics,.distanceBound)
    }
    func testSubtractionProducesAnActualCavity() {
        let shell=Shape.sphere(2).cut(.sphere(1).moved(V3(0,0,1.5)))
        XCTAssertGreaterThan(shell.value(at:V3(0,0,1.5)),0)
        XCTAssertLessThan(shell.value(at:V3(0,0,-1.5)),0)
        XCTAssertEqual(shell.bounds.min,V3(repeating:-2))
    }
    func testNonuniformStretchIsConservativeAndBounded() {
        let shape=Shape.sphere(1).stretched(V3(2,0.5,1))
        XCTAssertEqual(shape.semantics,.distanceBound)
        XCTAssertEqual(shape.value(at:V3(2,0,0)),0,accuracy:0.00001)
        XCTAssertEqual(shape.value(at:V3(0,0.5,0)),0,accuracy:0.00001)
        XCTAssertLessThanOrEqual(shape.value(at:V3(3,0,0)),1)
        XCTAssertEqual(shape.bounds.max,V3(2,0.5,1))
    }
    func testBlendBoundsAccountForConservativeStretch() {
        let thin=Shape.sphere(1).stretched(V3(10,0.1,0.1))
        let blended=thin.blended(thin,radius:0.4)
        let p=V3(19,0,0)
        XCTAssertLessThan(blended.value(at:p),0)
        XCTAssertTrue(blended.bounds.contains(p))
        XCTAssertEqual(blended.bounds.max.x,20,accuracy:0.0001)
    }
    func testMeshApproximatesFieldAndHasOutwardWinding() throws {
        let shape=Shape.sphere(1)
        let mesh=try Mesher.compile(shape,resolution:18)
        XCTAssertGreaterThan(mesh.indices.count,100)
        for vertex in mesh.vertices {
            let p=V3(vertex.position.x,vertex.position.y,vertex.position.z)
            XCTAssertLessThan(abs(shape.value(at:p)),0.009)
            XCTAssertEqual(length(V3(vertex.normal.x,vertex.normal.y,vertex.normal.z)),1,accuracy:0.0001)
        }
        for i in stride(from:0,to:mesh.indices.count,by:3) {
            func point(_ offset:Int)->V3 {let p=mesh.vertices[Int(mesh.indices[i+offset])].position;return V3(p.x,p.y,p.z)}
            let a=point(0),b=point(1),c=point(2)
            XCTAssertGreaterThanOrEqual(dot(cross(b-a,c-a),a+b+c),-0.000001)
        }
    }
    func testTerrainVisualVerticesMatchGroundQueries() {
        let field=Terrain(),mesh=Mesher.terrain(Terrain(),extent:80,resolution:80)
        XCTAssertEqual(field.semantics,.implicit)
        for v in mesh.vertices {
            XCTAssertEqual(field.value(at:V3(v.position.x,v.position.y,v.position.z)),0,accuracy:0.00001)
        }
        // Between mesh vertices the raster surface is planar. Quantify that
        // approximation against authoritative collision height at triangle centers.
        for i in stride(from:0,to:mesh.indices.count,by:3) {
            let a=mesh.vertices[Int(mesh.indices[i])].position
            let b=mesh.vertices[Int(mesh.indices[i+1])].position
            let c=mesh.vertices[Int(mesh.indices[i+2])].position
            let p=(a+b+c)/3
            XCTAssertLessThan(abs(field.height(p.x,p.z)-p.y),0.08)
        }
    }
    func testCutSurfaceEdgesHaveConsistentOrientation() throws {
        let shape=Shape.sphere(1.15).cut(.sphere(0.82).moved(V3(0,0.24,0.85)))
        let mesh=try Mesher.compile(shape,resolution:22)
        var edges:[String:(count:Int,balance:Int)]=[:]
        func key(_ index:UInt32)->String {
            let p=mesh.vertices[Int(index)].position
            return [p.x,p.y,p.z].map{String(Int(($0*10000).rounded()))}.joined(separator:",")
        }
        for i in stride(from:0,to:mesh.indices.count,by:3) {
            let vertices=(0..<3).map{key(mesh.indices[i+$0])}
            for j in 0..<3 {
                let a=vertices[j],b=vertices[(j+1)%3]
                if a==b {continue}
                let k=a<b ? a+"/"+b:b+"/"+a
                let old=edges[k] ?? (0,0)
                edges[k]=(old.count+1,old.balance+(a<b ? 1:-1))
            }
        }
        // Shared edges must have balanced opposing directions. Very small
        // triangles can collapse under the quantization used to weld vertices.
        XCTAssertTrue(edges.values.allSatisfy{$0.balance==0})
        XCTAssertFalse(edges.isEmpty)
    }
    func testSeededGenerationIsReproducible() {
        var a=SeededRandom(seed:42),b=SeededRandom(seed:42),c=SeededRandom(seed:43)
        let sequence=(0..<100).map{_ in a.next()}
        XCTAssertEqual(sequence,(0..<100).map{_ in b.next()})
        XCTAssertNotEqual(sequence,(0..<100).map{_ in c.next()})
    }
}
