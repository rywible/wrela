import XCTest
import FieldCore
import FieldCompiler
import simd

final class FieldEnclosureTests:XCTestCase {
  func testFieldIntervalsEncloseSamplesAcrossRotatedCSG() throws {
    let sphere=Shape.sphere(0.7).stretched(V3(0.3,1.5,0.6)).rotated(simd_quatf(angle:0.6,axis:V3(0,0,1))).moved(V3(0.3,0.2,-0.1))
    let capsule=Shape.capsule(V3(-0.4,0,0.1),V3(0.2,0.8,-0.2),0.18)
    let shapes=[sphere,capsule,Shape.box(V3(0.3,0.4,0.2)),Shape.torus(0.4,0.1),sphere.blended(capsule,radius:0.16).cut(.sphere(0.1))]
    var random=SeededRandom(seed:219)
    for shape in shapes {
      for _ in 0..<40 {
        let center=V3(random.range(-1,1),random.range(-1,1),random.range(-1,1)),half=V3(repeating:random.range(0.01,0.4))
        let region=Bounds(center-half,center+half),range=shape.valueInterval(in:region)
        for _ in 0..<12 {
          let p=center+half*V3(random.range(-1,1),random.range(-1,1),random.range(-1,1)),value=shape.value(at:p)
          XCTAssertGreaterThanOrEqual(value+0.00001,range.lower)
          XCTAssertLessThanOrEqual(value-0.00001,range.upper)
        }
      }
    }
  }
  func testOccupiedEnclosureRemovesAccumulatedBlendPaddingWithoutClipping() throws {
    var shape=Shape.sphere(0.7)
    for i in 0..<24 {
      let angle=Float(i)*2*Float.pi/24,center=V3(cos(angle)*0.45,Float(i%3)*0.1,sin(angle)*0.45)
      let muscle=Shape.sphere(1).stretched(V3(0.04,0.5,0.12)).rotated(simd_quatf(angle:angle,axis:V3(0,1,0))).moved(center)
      shape=shape.blended(muscle,radius:0.08)
    }
    let loose=shape.bounds,tight=shape.occupiedBounds(depth:6)
    XCTAssertLessThan(length(tight.max-tight.min),length(loose.max-loose.min)*0.4)
    var random=SeededRandom(seed:47)
    for _ in 0..<1000 {
      let p=V3(random.range(-1,1),random.range(-1,1),random.range(-1,1))
      if shape.value(at:p)<=0 {XCTAssertTrue(tight.contains(p))}
    }
    let mesh=try Mesher.compile(shape,resolution:32,bounds:tight)
    XCTAssertFalse(mesh.vertices.isEmpty)
    for vertex in mesh.vertices {
      let p=V3(vertex.position.x,vertex.position.y,vertex.position.z)
      XCTAssertTrue(tight.expanded(0.03).contains(p))
    }
  }
}
