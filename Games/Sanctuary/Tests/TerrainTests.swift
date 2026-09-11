import XCTest
import FieldCore
import FieldCompiler
import SanctuaryContent
import simd
final class TerrainTests:XCTestCase {
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
    func testFusedTerrainPreservesHeightsAndMatchesDerivatives() {
        let terrain=Terrain();var rng=SeededRandom(seed:801)
        for _ in 0..<200 {
            let x=rng.range(-155,155),z=rng.range(-155,155),s=terrain.sample(x,z),e:Float=0.05
            XCTAssertEqual(s.height,terrain.height(x,z),accuracy:0.00001)
            let g=V3(terrain.height(x-e,z)-terrain.height(x+e,z),2*e,terrain.height(x,z-e)-terrain.height(x,z+e))
            XCTAssertLessThan(length(s.normal-normalize(g)),0.0005)
        }
        let tiny=Shape.sphere(1).sized(0.00001)
        XCTAssertLessThan(length(tiny.normal(at:V3(0.000006,0.000008,0))-V3(0.6,0.8,0)),0.00001)
    }
}
