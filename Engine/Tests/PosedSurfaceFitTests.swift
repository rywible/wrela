import XCTest
import FieldCore
import FieldCompiler
import simd

final class PosedSurfaceFitTests:XCTestCase {
  func testVisibleEditInvertsTheActualBlendedSkinAndKeepsSourceLocal() throws {
    let mesh=ParametricMesh.surface(u:8,v:8,position:{u,v in V3(u-0.5,v-0.5,0)})
    let weights=mesh.vertices.map{SkinWeight(0,1,blend:$0.position.x+0.5)}
    var rotation=simd_float4x4(simd_quatf(angle:1.2,axis:V3(0,1,0)))
    rotation[3]=SIMD4(0.2,0.1,0.3,1)
    let palette=[matrix_identity_float4x4,rotation]
    let deformed=DeformationAudit.deformed(mesh,weights:weights,palette:palette)
    let v=deformed.vertices[40].position,point=V3(v.x,v.y,v.z),offset=V3(0.015,0.01,0.03)
    let fit=try PosedSurfaceFit.fit(id:"cheek",part:"skin",mesh:mesh,weights:weights,palette:palette,
      point:point,offset:offset,radius:0.4,activation:0.7)
    XCTAssertLessThan(fit.residual,0.00001)
    XCTAssertLessThan(length(fit.correctedPoint-(point+offset)),0.00001)
    XCTAssertEqual(fit.field.evaluate(V3(4,4,4)).position,V3(4,4,4))
    XCTAssertGreaterThan(length(fit.field.offset-offset),0.005,"Posed displacement must be converted into source coordinates")
  }
  func testMissInactiveDriverAndFoldRejectWithoutChangingInput() throws {
    let mesh=ParametricMesh.surface(u:8,v:8,position:{u,v in V3(u-0.5,v-0.5,0)})
    let original=mesh.vertices.map(\.position)
    for (point,offset,radius,activation):(V3,V3,Float,Float) in [
      (V3(0,0,2),V3(0,0,0.02),0.3,1),
      (.zero,V3(0,0,0.02),0.3,0),
      (.zero,V3(0.5,0,0),0.1,1)] {
      XCTAssertThrowsError(try PosedSurfaceFit.fit(id:"bad",part:"skin",mesh:mesh,weights:[],palette:[],
        point:point,offset:offset,radius:radius,activation:activation))
    }
    XCTAssertEqual(mesh.vertices.map(\.position),original)
  }
}
