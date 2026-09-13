import XCTest
import FieldCore
import FieldCompiler
import simd

final class TriangleMotionTests:XCTestCase {
  func testLargeRotationWithoutCollapse() {
    let source=[V3.zero,V3(1,0,0),V3(0,1,0)],rotation=simd_quatf(angle:Float.pi*5/6,axis:V3(1,0,0))
    let target=source.map{rotation.act($0)}
    XCTAssertLessThan(dot(cross(source[1],source[2]),cross(target[1],target[2])),0)
    XCTAssertTrue(TriangleMotion.preservesArea(from:source,to:target))
    XCTAssertTrue(TriangleMotion.preservesArea(from:source.map{$0*0.001},to:target.map{$0*0.001}))
  }
  func testMatchingEndpointNormalsCannotHideIntermediateCollapse() {
    let source=[V3.zero,V3(1,0,0),V3(0,1,0)],target=[V3.zero,V3(-1,0,0),V3(0,-2,0)]
    XCTAssertGreaterThan(dot(cross(source[1],source[2]),cross(target[1],target[2])),0)
    XCTAssertFalse(TriangleMotion.preservesArea(from:source,to:target))
    XCTAssertFalse(TriangleMotion.preservesArea(from:source,to:[.zero,V3(1,0,0),V3(0,-1,0)]))
    XCTAssertTrue(TriangleMotion.preservesArea(from:source,to:source.map{$0+V3(4,5,6)}))
  }
}
