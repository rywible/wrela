import XCTest
import FieldCore
import FieldEngine
import simd

final class SecondaryPosePlayerTests:XCTestCase {
  private func rig(_ width:Float=0.4)->SecondaryRig {
    SecondaryRig(nodes:[
      SecondaryNode("root",joint:"body",position:V3(0,1,0),inverseMass:0),
      SecondaryNode("tip",joint:"body",position:V3(width,0.6,0),inverseMass:1)
    ],links:[SecondaryLink(0,1,contact:true)])
  }
  private func pose(_ time:Float)->[String:simd_float4x4] {
    var matrix=simd_float4x4(simd_quatf(angle:sin(time)*0.2,axis:V3(0,0,1)))
    matrix.columns.3=SIMD4(sin(time*2)*0.1,0,0,1)
    return ["body":matrix]
  }
  private func evaluate(_ player:SecondaryPosePlayer,_ time:Float,rig:SecondaryRig?=nil,
    settings:SecondarySettings=SecondarySettings())->[String:simd_float4x4] {
    player.evaluate(key:"test",rig:rig ?? self.rig(),settings:settings,time:time,
      sample:pose,colliders:[],height:{_,_ in 0})
  }
  func testEndpointBodyPoseIsSolvedOncePerEvaluation() {
    let player=SecondaryPosePlayer()
    var calls:[Float:Int]=[:]
    func sample(_ t:Float)->[String:simd_float4x4] {calls[t,default:0]+=1;return pose(t)}
    _=player.evaluate(key:"test",rig:rig(),settings:SecondarySettings(),time:0,
      sample:sample,colliders:[],height:{_,_ in 0})
    XCTAssertEqual(calls,[0:1])
    calls=[:]
    _=player.evaluate(key:"test",rig:rig(),settings:SecondarySettings(),time:1/60,
      sample:sample,colliders:[],height:{_,_ in 0})
    XCTAssertEqual(calls,[Float(0):1,Float(1)/60:1])
    XCTAssertEqual(player.simulatedTicks,1)
    calls=[:]
    _=player.evaluate(key:"test",rig:rig(),settings:SecondarySettings(),time:1/60,
      sample:sample,colliders:[],height:{_,_ in 0})
    XCTAssertEqual(calls,[Float(1)/60:1])
    XCTAssertEqual(player.simulatedTicks,0)
  }
  func testForwardReverseAndRepeatedSeekReproduceExactState() {
    let forward=SecondaryPosePlayer()
    for tick in 0...137 {_=evaluate(forward,Float(tick)/60)}
    let expected=forward.positions
    let expectedMatrices=evaluate(forward,137/60)
    let seek=SecondaryPosePlayer()
    _=evaluate(seek,4)
    let actualMatrices=evaluate(seek,137/60)
    XCTAssertEqual(seek.positions,expected)
    for id in expectedMatrices.keys {XCTAssertEqual(actualMatrices[id],expectedMatrices[id])}
    _=evaluate(seek,0.3)
    _=evaluate(seek,137/60)
    XCTAssertEqual(seek.positions,expected)
  }
  func testChangedRigAndSettingsInvalidateCoefficientsEvenWithSameKey() {
    let reused=SecondaryPosePlayer(),fresh=SecondaryPosePlayer()
    _=evaluate(reused,2)
    var settings=SecondarySettings();settings.damping=1;settings.substeps=3;settings.followCompliance=2
    _=evaluate(reused,1,rig:rig(0.7),settings:settings)
    _=evaluate(fresh,1,rig:rig(0.7),settings:settings)
    XCTAssertEqual(reused.positions,fresh.positions)
  }
  func testRemovingGuidesClearsDiagnosticsAndReaddingRestartsReplay() {
    let reused=SecondaryPosePlayer(),fresh=SecondaryPosePlayer()
    _=evaluate(reused,2)
    _=evaluate(reused,2,rig:SecondaryRig())
    XCTAssertTrue(reused.positions.isEmpty)
    XCTAssertEqual(reused.metrics.maximumDisplacement,0)
    XCTAssertEqual(reused.simulatedTicks,0)
    _=evaluate(reused,0.5)
    _=evaluate(fresh,0.5)
    XCTAssertEqual(reused.positions,fresh.positions)
  }
}
