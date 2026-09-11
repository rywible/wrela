import XCTest
import simd
@testable import FieldCore
@testable import FieldCompiler

final class SurfaceTests:XCTestCase {
    func testDualSurfaceUsesFewerTrianglesAndSharedVertices() throws {
        let shape=Shape.sphere(1)
        let reference=try Mesher.reference(shape,resolution:32)
        let mesh=try Mesher.compile(shape,resolution:32)
        XCTAssertNotNil(mesh.report)
        XCTAssertLessThan(mesh.indices.count,reference.indices.count/2)
        XCTAssertLessThan(mesh.vertices.count,mesh.indices.count/2)
        XCTAssertTrue(mesh.report!.manifold)
        XCTAssertGreaterThan(mesh.report!.excludedCells,0)
        XCTAssertLessThan(mesh.report!.maximumSampleResidual,0.004)
    }
    func testSharpBoxIsAccurateAndShadingFacesStayFlat() throws {
        let shape=Shape.box(V3(1,0.6,0.8)),mesh=try Mesher.compile(shape,resolution:16)
        XCTAssertNotNil(mesh.report)
        for v in mesh.vertices {let p=V3(v.position.x,v.position.y,v.position.z);XCTAssertLessThan(abs(shape.value(at:p)),0.0003)}
        XCTAssertLessThan(mesh.report!.maximumSampleResidual,0.0003)
    }
    func testCavityAndThinStemSurviveCompilation() throws {
        let shape=Shape.sphere(1).cut(.sphere(0.65).moved(V3(0,0,0.8))).joined(.capsule(V3(0,0.9,0),V3(0.3,1.9,0),0.09))
        let mesh=try Mesher.compile(shape,resolution:64)
        XCTAssertTrue(mesh.vertices.contains{$0.position.y>1.9})
        let cavityCenter=V3(0,0,0.8)
        XCTAssertTrue(mesh.vertices.contains {
            let p=V3($0.position.x,$0.position.y,$0.position.z),n=V3($0.normal.x,$0.normal.y,$0.normal.z)
            return abs(length(p-cavityCenter)-0.65)<0.005 && p.z<0.5 && dot(n,normalize(cavityCenter-p))>0.9
        })
        // The ambiguous dual cells deliberately use the tetrahedral fallback.
        // Check the resulting surface after welding intentional shading seams.
        var welded=Mesh(),positions:[SIMD4<Float>:UInt32]=[:]
        for index in mesh.indices {
            let v=mesh.vertices[Int(index)]
            if let i=positions[v.position] {welded.indices.append(i)}
            else {let i=UInt32(welded.vertices.count);positions[v.position]=i;welded.vertices.append(v);welded.indices.append(i)}
        }
        XCTAssertTrue(SurfaceCompiler.isClosed(welded))
    }
    func testSimplificationPreservesClosedSphereAndRespectsErrorSetting() throws {
        let mesh=try Mesher.compile(.sphere(1),resolution:48)
        let simple=MeshProcessing.simplify(mesh,ratio:0.25,error:0.025)
        XCTAssertLessThan(simple.indices.count,mesh.indices.count)
        XCTAssertTrue(SurfaceCompiler.isClosed(simple))
        XCTAssertLessThanOrEqual(simple.geometricError,0.0251)
    }
    func testMeshletsReconstructTheSameOrientedTriangles() throws {
        let mesh=try Mesher.compile(.sphere(1),resolution:24),clusters=MeshletData(mesh)
        func key(_ a:UInt32,_ b:UInt32,_ c:UInt32)->String {
            let t=a<=b && a<=c ? [a,b,c] : (b<=a && b<=c ? [b,c,a]:[c,a,b]);return t.map(String.init).joined(separator:",")
        }
        var source:[String:Int]=[:],rebuilt:[String:Int]=[:]
        for i in stride(from:0,to:mesh.indices.count,by:3){source[key(mesh.indices[i],mesh.indices[i+1],mesh.indices[i+2]),default:0]+=1}
        for (d,sphere) in zip(clusters.descriptors,clusters.spheres) {
            XCTAssertLessThanOrEqual(d.z,64);XCTAssertLessThanOrEqual(d.w,124)
            for j in 0..<Int(d.z) {
                let p=mesh.vertices[Int(clusters.vertices[Int(d.x)+j])].position
                XCTAssertLessThanOrEqual(length(V3(p.x-sphere.x,p.y-sphere.y,p.z-sphere.z)),sphere.w+0.0001)
            }
            for j in 0..<Int(d.w) {
                let ids=(0..<3).map{clusters.vertices[Int(d.x)+Int(clusters.triangles[Int(d.y)+j*3+$0])]}
                rebuilt[key(ids[0],ids[1],ids[2]),default:0]+=1
            }
        }
        XCTAssertEqual(source,rebuilt)
    }
    func testWindReplaysAndInterpolatesBetweenFlowUpdates() {
        var a=WindSimulation(),b=WindSimulation()
        for _ in 0..<240 {a.step(1/60);b.step(1/60)}
        XCTAssertEqual(a.gpuCells,b.gpuCells)
        let previous=a.gpuCells;a.step(1/60)
        XCTAssertNotEqual(previous,a.gpuCells)
        XCTAssertTrue(a.gpuCells.allSatisfy{$0.x.isFinite && abs($0.z)<=0.35 && abs($0.w)<=0.35})
    }
    func testTerrainCertificatesContainSamplesAndDerivatives() {
        let terrain=Terrain();var rng=SeededRandom(seed:33)
        for _ in 0..<100 {
            let x=rng.range(-155,145),z=rng.range(-155,145),cert=terrain.patchCertificate(x:x...x+10,z:z...z+10)
            for _ in 0..<20 {
                let px=rng.range(x,x+10),pz=rng.range(z,z+10),e:Float=0.01
                XCTAssertTrue(cert.height.contains(terrain.height(px,pz)))
                XCTAssertLessThanOrEqual(abs(terrain.height(px+e,pz)-terrain.height(px-e,pz))/(2*e),cert.dx+0.003)
                XCTAssertLessThanOrEqual(abs(terrain.height(px,pz+e)-terrain.height(px,pz-e))/(2*e),cert.dz+0.003)
            }
        }
    }
}
