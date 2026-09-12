import XCTest
import FieldCore
import simd

final class DigitigradeContactTests:XCTestCase {
  func testRaisedHockPreservesRotatedToeContactAcrossCompressionAndSlope() throws {
    let joints=[PartJoint(id:"body",pivot:V3(0,1.5,0)),
      PartJoint(id:"thigh",parent:"body",pivot:V3(0,1.5,0)),
      PartJoint(id:"shin",parent:"thigh",pivot:V3(0,0.9,-0.4)),
      PartJoint(id:"foot",parent:"shin",pivot:V3(0,0.35,0.2))]
    let chain=ContactChain(id:"leg",parent:"body",upper:"thigh",lower:"shin",foot:"foot",
      pole:V3(0,0,-2),sole:0.10,contactOffset:V3(0,-0.25,-0.18))
    try chain.validate(joints:joints)
    func point(_ m:simd_float4x4,_ p:V3)->V3 {let q=m*SIMD4(p,1);return V3(q.x,q.y,q.z)}
    for depth:Float in [0,0.3,0.65] {
      for heading:Float in [0,35,90] {
        let matrices=PartRig.matrices(joints,poses:["body":JointPose(offset:V3(0,-depth,0)),"foot":JointPose(rotation:V3(0,heading,0))])
        let target=V3(0.1,0.1,-0.05),normal=normalize(V3(-0.1,1,0))
        let result=ContactRig.solve(joints:joints,matrices:matrices,chains:[chain],targets:["leg":ContactTarget(target,planted:true)],height:{x,_ in x*0.1},normal:{_,_ in normal})
        let observation=try XCTUnwrap(result.contacts.first),foot=try XCTUnwrap(result.matrices["foot"])
        XCTAssertLessThan(observation.residual,0.00001)
        XCTAssertLessThan(length(point(foot,joints[3].pivot+chain.contactOffset!)-observation.target),0.00001)
        XCTAssertGreaterThan(point(foot,joints[3].pivot).y,observation.target.y+0.20)
        XCTAssertEqual(length(point(foot,joints[3].pivot)-observation.actual),length(chain.contactOffset!),accuracy:0.00001)
        XCTAssertLessThan(abs(observation.clearance),0.00001)
        let disabled=ContactRig.solve(joints:joints,matrices:matrices,chains:[chain],targets:["leg":ContactTarget(target,planted:true)],applying:false)
        XCTAssertLessThan(length(disabled.contacts[0].actual-point(matrices["foot"]!,joints[3].pivot+chain.contactOffset!)),0.00001)
      }
    }
  }
  func testLegacyAndInvalidOffsets() throws {
    let legacy=Data(#"{"id":"leg","parent":"body","upper":"upper","lower":"lower","foot":"foot","pole":[0,0,1],"sole":0.1}"#.utf8)
    let chain=try JSONDecoder().decode(ContactChain.self,from:legacy)
    XCTAssertNil(chain.contactOffset)
    XCTAssertEqual(try JSONDecoder().decode(ContactChain.self,from:JSONEncoder().encode(chain)),chain)
    var invalid=chain;invalid.contactOffset=V3(.nan,0,0)
    let joints=[PartJoint(id:"body"),PartJoint(id:"upper",parent:"body",pivot:V3(0,2,0)),PartJoint(id:"lower",parent:"upper",pivot:V3(0,1,0)),PartJoint(id:"foot",parent:"lower")]
    XCTAssertThrowsError(try invalid.validate(joints:joints))
  }
}
