import XCTest
import FieldCore
import simd

final class AnatomyInfluenceTests:XCTestCase {
  func testResponsesPredictComposedFieldChanges() throws {
    let elements=[AnatomyElement(id:"skull",region:"form",center:.zero,radius:V3(0.5,0.4,0.6)),
      AnatomyElement(id:"cheek",region:"form",center:V3(0.25,0,0),radius:V3(0.35,0.3,0.4),rotation:V3(0,0,17),blend:0.2),
      AnatomyElement(id:"buried",region:"internal",center:.zero,radius:V3(repeating:0.01),blend:0)]
    let source=AnatomySource(id:"form",part:"form",elements:elements)
    let p=V3(0.39,0.28,0.12),report=try source.explain(at:p),field=try source.shape()
    XCTAssertEqual(report.fieldValue,field.value(at:p),accuracy:0.000001)
    XCTAssertEqual(report.contributors.count,2)
    XCTAssertEqual(report.inactiveElements,["buried"])
    XCTAssertEqual(report.contributors.reduce(Float(0)){$0+$1.fieldCoefficient},1,accuracy:0.000001)
    for entry in report.contributors {
      let index=source.elements.firstIndex{$0.id==entry.element}!
      for control in entry.controls {
        var lo=source,hi=source
        let pieces=control.parameter.split(separator:"."),axis=["x","y","z"].firstIndex(of:pieces.last.map(String.init) ?? "") ?? 0
        let step:Float=control.unit=="degree" ? 0.02:0.0001
        func change(_ s:inout AnatomySource,_ delta:Float) {
          switch pieces[0] {
          case "center":s.elements[index].center[axis]+=delta
          case "radius":s.elements[index].radius[axis]+=delta
          case "rotation":s.elements[index].rotation[axis]+=delta
          default:s.elements[index].blend+=delta
          }
        }
        change(&lo,-step);change(&hi,step)
        let actual = -(try hi.shape().value(at:p)-lo.shape().value(at:p))/(2*step*report.gradientMagnitude)
        XCTAssertEqual(control.normalResponse,actual,accuracy:0.001,entry.element+"/"+control.parameter)
      }
    }
  }
  func testCutResponseAndInactiveBlendAreExplicit() throws {
    let source=AnatomySource(id:"socket",part:"head",elements:[
      AnatomyElement(id:"shell",region:"head",center:.zero,radius:V3(repeating:1)),
      AnatomyElement(id:"socket",region:"eye",operation:.cut,center:V3(0,0,0.9),radius:V3(repeating:0.3),blend:0.1)])
    let result=try source.explain(at:V3(0,0,0.6))
    XCTAssertEqual(result.contributors.count,1)
    XCTAssertEqual(result.contributors[0].fieldCoefficient,-1)
    XCTAssertFalse(result.contributors[0].controls.contains{$0.parameter=="blend"})
    XCTAssertEqual(result.normal,V3(0,0,1))
    XCTAssertEqual(result.contributors[0].controls.first{$0.parameter=="radius.z"}!.normalResponse,-1,accuracy:0.001)
    XCTAssertThrowsError(try source.explain(at:V3(.nan,0,0)))
    let singular=AnatomySource(id:"singular",part:"form",elements:[AnatomyElement(id:"sphere",region:"form",center:.zero,radius:V3(repeating:1))])
    let center=try singular.explain(at:.zero)
    XCTAssertNil(center.linearizedSurfaceOffset)
    XCTAssertTrue(center.contributors[0].controls.isEmpty)
  }
}
