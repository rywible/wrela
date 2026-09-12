import XCTest
import FieldCore
import simd

final class CurveFrameTests:XCTestCase {
  func testBentCurveFollowsCentralTangentsWithoutIndependentRoll() throws {
    let rest=[V3(0,2,0),V3(0,1.4,0),V3(0,0.7,0),V3(0,0,0)]
    let bent=[V3(0,2,0),V3(0.4,1.4,0),V3(0.55,0.7,0),V3(0.4,0,0)]
    let frames=try XCTUnwrap(GuideFrame.curve(old:rest,new:bent))
    for i in rest.indices {
      let a=normalize(rest[min(i+1,3)]-rest[max(0,i-1)])
      let b=normalize(bent[min(i+1,3)]-bent[max(0,i-1)])
      XCTAssertLessThan(length(frames[i]*a-b),0.00001)
      XCTAssertLessThan(length(frames[i]*V3(0,0,1)-V3(0,0,1)),0.00001)
      XCTAssertEqual(frames[i].determinant,1,accuracy:0.00001)
    }
    let unchanged=try XCTUnwrap(GuideFrame.curve(old:bent,new:bent))
    for matrix in unchanged {for axis in 0..<3 {
      XCTAssertLessThan(length(matrix[axis]-matrix_identity_float3x3[axis]),0.00001)
    }}
    XCTAssertNil(GuideFrame.curve(old:rest,new:[V3.zero]))
    XCTAssertNil(GuideFrame.curve(old:rest,new:Array(repeating:.zero,count:4)))
  }
}
