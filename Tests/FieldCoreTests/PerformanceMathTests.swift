import XCTest
import simd
@testable import FieldCore
import SanctuaryContent
@testable import FieldCompiler

final class PerformanceMathTests:XCTestCase {
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
    func testLeafLODRetainsEveryLeafBoundary() throws {
        let canopy=try Mesher.compile(.sphere(1),resolution:16)
        let full=Mesher.leaves(on:canopy,count:64),flat=Mesher.leaves(on:canopy,count:64,flattened:true)
        XCTAssertEqual(full.indices.count,64*12);XCTAssertEqual(flat.indices.count,64*6)
        let vertices=Set(full.vertices.map(\.position))
        XCTAssertTrue(flat.vertices.allSatisfy {vertices.contains($0.position)})
        XCTAssertGreaterThanOrEqual(flat.geometricError,0.035)
    }
    func testWeatherCheckpointKeysAndRewindPreserveExactReplay() {
        var replay=WeatherReplay(),reference=MoistWeather(seed:4)
        reference.advance(to:240,sun:0.6)
        XCTAssertEqual(replay.advance(to:240,seed:4,sun:0.6).cells,reference.cells)
        _=replay.advance(to:240,seed:4,sun:0.2)
        XCTAssertEqual(replay.advance(to:240,seed:4,sun:0.6).cells,reference.cells)
        XCTAssertEqual(replay.replayedSteps,0)
        var earlier=MoistWeather(seed:4);earlier.advance(to:132,sun:0.6)
        XCTAssertEqual(replay.advance(to:132,seed:4,sun:0.6).cells,earlier.cells)
        XCTAssertLessThan(replay.replayedSteps,30)
        var other=MoistWeather(seed:9);other.advance(to:132,sun:0.6)
        XCTAssertEqual(replay.advance(to:132,seed:9,sun:0.6).cells,other.cells)
    }
    func testRegionSpecializationPreservesValuesAndGradients() {
        let shape=Shape.sphere(1).blended(.sphere(0.7).moved(V3(2,0,0)),radius:0.4).joined(.box(V3(repeating:0.5)).moved(V3(-4,0,0))).stretched(V3(2,0.5,1))
        var rng=SeededRandom(seed:91),pruned=0
        for _ in 0..<100 {
            let lo=V3(rng.range(-9,5),rng.range(-1,1),rng.range(-1,1)),region=Bounds(lo,lo+V3(repeating:0.25))
            let local=shape.specialized(in:region);pruned+=shape.operationCount-local.operationCount
            for _ in 0..<20 {
                let p=lo+V3(rng.range(0,0.25),rng.range(0,0.25),rng.range(0,0.25)),a=shape.sample(at:p),b=local.sample(at:p)
                XCTAssertEqual(a.value,b.value,accuracy:0.000002);XCTAssertLessThan(length(a.gradient-b.gradient),0.000002)
            }
        }
        XCTAssertGreaterThan(pruned,0)
    }
    func testGrassDescriptorPreservesBladeGeometry() {
        XCTAssertEqual(MemoryLayout<GrassBlade>.stride,32)
        var rng=SeededRandom(seed:11)
        for _ in 0..<100 {
            let p=V3(rng.range(-76,76),rng.range(-3,8),rng.range(-108,54)),a=rng.range(0,6.28),h=rng.range(0.18,0.48),w=rng.range(0.018,0.045),c=V3(0.341,0.527,0.219)
            let blade=GrassBlade(anchor:p,angle:a,height:h,width:w,color:c),v=blade.vertices
            let d=V3(cos(a)*w,0,sin(a)*w),bend=V3(cos(a+0.8),0,sin(a+0.8))*h*0.32
            XCTAssertEqual(v[0].position,SIMD4(p-d,1));XCTAssertEqual(v[4].position,SIMD4(p+V3(0,h,0)+bend,1))
            XCTAssertEqual(v[0].normal.w,0);XCTAssertEqual(v[4].normal.w,1)
            XCTAssertLessThan(length(V3(v[0].color.x,v[0].color.y,v[0].color.z)-c),0.00085)
            XCTAssertEqual(GrassBlade.triangleIndices.count,9)
        }
    }
    func testBroadPhaseIncludesBoundsCrossingNegativeCellEdges() {
        let bounds=[Bounds(V3(-17,-1,-1),V3(-15,1,1)),Bounds(V3(-1,-1,-1),V3(1,1,1)),Bounds(V3(50,-1,50),V3(60,1,60))]
        let grid=SpatialBoundsGrid(bounds:bounds)
        XCTAssertTrue(grid.candidates(at:V3(-16.5,0,0)).contains(0))
        XCTAssertTrue(grid.candidates(at:V3(-15.5,0,0)).contains(0))
        XCTAssertTrue(grid.candidates(at:V3(-0.5,0,0)).contains(1))
        XCTAssertFalse(grid.candidates(at:.zero).contains(2))
        XCTAssertEqual(grid.candidates(at:.zero),grid.candidates(at:V3(0,100,0)))
    }
    func testFusedGradientsMatchIndependentCentralDifferences() {
        let shapes:[Shape]=[.sphere(1),.box(V3(1,0.7,0.8)),.capsule(V3(-1,0,0),V3(0.3,1,0.2),0.3),.torus(1,0.2),
            .sphere(1).stretched(V3(2,0.3,1)).moved(V3(0.2,0.1,-0.4)),
            .sphere(1).blended(.sphere(0.7).moved(V3(0.7,0.3,0)),radius:0.4),
            .sphere(1).cut(.sphere(0.7).moved(V3(0,0,0.7))),.sphere(1).sized(0.0035)]
        var rng=SeededRandom(seed:917)
        for shape in shapes {for _ in 0..<100 {
            let p=V3(rng.range(-1.4,1.4),rng.range(-1.4,1.4),rng.range(-1.4,1.4)),s=shape.sample(at:p),e:Float=0.0005
            let g=V3(shape.value(at:p+V3(e,0,0))-shape.value(at:p-V3(e,0,0)),shape.value(at:p+V3(0,e,0))-shape.value(at:p-V3(0,e,0)),shape.value(at:p+V3(0,0,e))-shape.value(at:p-V3(0,0,e)))/(2*e)
            XCTAssertEqual(s.value,shape.value(at:p),accuracy:0.000001)
            XCTAssertLessThan(length(s.gradient-g),0.003)
        }}
        XCTAssertEqual(Shape.box(V3(repeating:1)).sample(at:V3(1,1,0)).gradient,V3(1,0,0))
        XCTAssertEqual(Shape.sphere(1).sample(at:.zero).gradient,.zero)
    }
    func testQEFFastPathMatchesExhaustiveOracle() {
        var rng=SeededRandom(seed:401)
        for i in 0..<1200 {
            func vector()->V3 {V3(rng.range(-1,1),rng.range(-1,1),rng.range(-1,1))}
            let M=simd_float3x3(columns:(vector(),i%3==0 ? .zero:vector(),i%7==0 ? .zero:vector()))
            let A=M.transpose*M+matrix_identity_float3x3*0.0001,b=vector(),mass=vector(),lo=V3(repeating:-0.5),hi=V3(repeating:0.5)
            let fast=BoxQEF.solve(A,b,mass:mass,lo:lo,hi:hi),oracle=BoxQEF.solve(A,b,mass:mass,lo:lo,hi:hi,exhaustive:true)
            XCTAssertLessThan(length(fast-oracle),0.0001)
        }
    }
    func testSharedEdgesAreSolvedOnce() {
        let (_,report)=SurfaceCompiler.compile(.sphere(1),resolution:32,color:V3(repeating:1))
        XCTAssertGreaterThan(report.solvedEdges,0)
        XCTAssertEqual(report.edgeCacheHits,report.solvedEdges*3)
        XCTAssertTrue(report.manifold)
    }
    func testSaturationDerivativeAndClamp() {
        for t in stride(from:Float(-24),through:29,by:0.25) {
            let s=MoistWeather.saturationSample(t),e:Float=0.01
            let reference=(MoistWeather.saturation(t+e)-MoistWeather.saturation(t-e))/(2*e)
            XCTAssertEqual(s.slope,reference,accuracy:0.0007)
        }
        for t:Float in [-40,-25,30,50] {XCTAssertEqual(MoistWeather.saturationSample(t).slope,0)}
    }
    func testCompatibleWindProjectionRemovesManufacturedDivergence() {
        var wind=WindSimulation()
        for y in 0..<32 {for x in 0..<32 {wind.flow[y*32+x]=SIMD2(sin(Float(x)*Float.pi/2),0)}}
        wind.projectFlow(iterations:24)
        var residual:Float=0
        for y in 0..<32 {for x in 0..<32 {residual=max(residual,abs((wind.flow[y*32+((x+1)&31)].x-wind.flow[y*32+((x+31)&31)].x)*0.5))}}
        XCTAssertLessThan(residual,0.00001)
    }
}
