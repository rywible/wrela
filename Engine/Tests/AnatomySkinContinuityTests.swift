import XCTest
import FieldCore
import FieldCompiler
import simd

final class AnatomySkinContinuityTests: XCTestCase {
  private func source() -> AnatomySource {
    AnatomySource(id:"joint",part:"skin",elements:[
      AnatomyElement(id:"left",region:"upper",center:V3(-0.35,0,0),radius:V3(0.55,0.3,0.3),blend:0.005,color:V3(0.3,0.4,0.5),jointWeights:["upper":1]),
      AnatomyElement(id:"right",region:"lower",center:V3(0.35,0,0),radius:V3(0.55,0.3,0.3),blend:0.005,color:V3(0.6,0.7,0.8),jointWeights:["lower":1])
    ],resolution:32)
  }
  func testIndependentWeightWidthPreservesGeometryMaterialsAndRegionIdentity() throws {
    let sharp=source();var smooth=sharp;smooth.skinBlend=0.2
    var largestSharpJump:Float=0,largestSmoothJump:Float=0
    var previousSharp:Float?,previousSmooth:Float?
    for i in 0...200 {
      let p=V3(Float(i-100)*0.001,0.23,0)
      let a=sharp.sample(at:p),b=smooth.sample(at:p)
      XCTAssertEqual(a.field.value,b.field.value,accuracy:0.0000001)
      XCTAssertEqual(a.field.gradient,b.field.gradient)
      XCTAssertEqual(a.color,b.color)
      XCTAssertEqual(a.region,b.region)
      XCTAssertEqual(a.element,b.element)
      XCTAssertEqual(b.jointWeights.values.reduce(0,+),1,accuracy:0.000001)
      let wa=a.jointWeights["lower"] ?? 0,wb=b.jointWeights["lower"] ?? 0
      if let previousSharp,let previousSmooth {
        largestSharpJump=max(largestSharpJump,abs(wa-previousSharp))
        largestSmoothJump=max(largestSmoothJump,abs(wb-previousSmooth))
      }
      previousSharp=wa;previousSmooth=wb
    }
    XCTAssertGreaterThan(largestSharpJump,0.05)
    XCTAssertLessThan(largestSmoothJump,largestSharpJump*0.05)
    XCTAssertEqual(smooth.sample(at:V3(-0.8,0,0)).jointWeights,["upper":1])
    XCTAssertEqual(smooth.sample(at:V3(0.8,0,0)).jointWeights,["lower":1])
  }
  func testSkinWidthDecodeCompatibilityAndBoundedValidation() throws {
    let original=source();var widened=original;widened.skinBlend=0.2
    let data=try JSONEncoder().encode(widened)
    XCTAssertEqual(try JSONDecoder().decode(AnatomySource.self,from:data),widened)
    var legacy=try JSONSerialization.jsonObject(with:JSONEncoder().encode(original)) as! [String:Any]
    legacy.removeValue(forKey:"skinBlend")
    XCTAssertEqual(try JSONDecoder().decode(AnatomySource.self,from:JSONSerialization.data(withJSONObject:legacy)).skinBlend,0)
    XCTAssertTrue(widened.changes(from:original).rebuildRequired)
    XCTAssertFalse(widened.changes(from:original).rebindRequired)
    widened.skinBlend = -0.01;XCTAssertThrowsError(try widened.validate())
    widened.skinBlend = 1.01;XCTAssertThrowsError(try widened.validate())
    widened.skinBlend = .nan;XCTAssertThrowsError(try widened.validate())
  }
  func testUnweightedCutInheritsContinuousBindingInsteadOfResetting() throws {
    var source=source();source.skinBlend=0.2
    let point=V3(0.02,0.18,0)
    let expected=source.sample(at:point).jointWeights
    source.elements.append(AnatomyElement(id:"socket",region:"cut",operation:.cut,center:point,radius:V3(repeating:0.1)))
    let cut=source.sample(at:point)
    XCTAssertEqual(cut.region,"cut")
    XCTAssertEqual(cut.jointWeights,expected)
  }
}
