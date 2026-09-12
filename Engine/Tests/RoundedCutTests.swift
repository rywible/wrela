import XCTest
import FieldCore
import FieldCompiler
import simd

final class RoundedCutTests:XCTestCase {
  func testRoundedCutGradientEnclosureAndSpecialization() throws {
    let a=Shape.sphere(1).stretched(V3(0.7,1,0.85)),b=Shape.sphere(0.65).moved(V3(0.5,0.2,0.6))
    let shape=a.cut(b,radius:0.12),hard=a.cut(b)
    try shape.validate()
    XCTAssertThrowsError(try a.cut(b,radius:-0.1).validate())
    for z in -5...5 {for y in -5...5 {for x in -5...5 {
      let p=V3(Float(x),Float(y),Float(z))*0.19+V3(0.013,0.023,0.037),s=shape.sample(at:p)
      XCTAssertEqual(s.value,shape.value(at:p),accuracy:0.000001)
      XCTAssertGreaterThanOrEqual(s.value+0.000001,hard.value(at:p)) // Subset of original solid.
      let box=Bounds(p-V3(repeating:0.03),p+V3(repeating:0.03)),interval=shape.valueInterval(in:box)
      XCTAssertLessThanOrEqual(interval.lower,s.value);XCTAssertGreaterThanOrEqual(interval.upper,s.value)
      XCTAssertEqual(shape.specialized(in:box).value(at:p),s.value,accuracy:0.000001)
      for axis in 0..<3 {
        var d=V3.zero;d[axis]=0.0001
        XCTAssertEqual(s.gradient[axis],(shape.value(at:p+d)-shape.value(at:p-d))/0.0002,accuracy:0.002)
      }
    }}}
    XCTAssertEqual(shape.bounds.min,a.bounds.min);XCTAssertEqual(shape.bounds.max,a.bounds.max)
    let mesh=try Mesher.compile(shape,resolution:32)
    XCTAssertFalse(mesh.vertices.isEmpty)
    XCTAssertTrue(try MetalField.source(for:shape).contains("evaluateField"))
  }
  func testAuthoredRoundedCutMatchesFieldWeightsAndResponse() throws {
    let shell=AnatomyElement(id:"head",region:"skin",center:.zero,radius:V3(repeating:1),jointWeights:["head":1])
    var cut=AnatomyElement(id:"eye",region:"socket",operation:.cut,center:V3(0.65,0,0.6),radius:V3(repeating:0.5),color:V3(1,0,0),jointWeights:["eye":1],cutBlend:0.1)
    let source=AnatomySource(id:"head",part:"head",elements:[shell,cut])
    let shape=try source.shape()
    var testedBlend=false
    for i in 0...30 {
      let p=V3(0.65+Float(i)*0.01,0,0.75),sample=source.sample(at:p),explanation=try source.explain(at:p)
      XCTAssertEqual(sample.field.value,shape.value(at:p),accuracy:0.000001)
      XCTAssertLessThan(length(sample.field.gradient-shape.sample(at:p).gradient),0.00001)
      XCTAssertEqual(sample.jointWeights.values.reduce(0,+),1,accuracy:0.000001)
      if let response=explanation.contributors.flatMap(\.controls).first(where:{$0.parameter=="cutBlend"}),abs(response.normalResponse)>0.0001 {
        testedBlend=true
        var candidate=source;candidate.elements[1].cutBlend!+=0.0001
        let difference = -(try candidate.shape().value(at:p)-shape.value(at:p))/(0.0001*explanation.gradientMagnitude)
        XCTAssertEqual(response.normalResponse,difference,accuracy:0.001)
      }
    }
    XCTAssertTrue(testedBlend)
    var craft=CreatureCraft();craft.anatomy=[source]
    let data=try JSONEncoder().encode(craft)
    try CraftSchema.validate(JSONSerialization.jsonObject(with:data))
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:data),craft)
    cut.cutBlend=nil
    let legacy=AnatomySource(id:"legacy",part:"head",elements:[shell,cut])
    XCTAssertEqual(try legacy.shape().value(at:V3(0.8,0,0.7)),shell.shape.cut(cut.shape).value(at:V3(0.8,0,0.7)))
    cut.operation = .add;cut.cutBlend=0.1
    XCTAssertThrowsError(try cut.validate())
  }
}
