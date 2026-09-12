import FieldCompiler
import FieldCore
import XCTest
import simd

final class RigidSkinningTests: XCTestCase {
  private func rotation(_ angle:Float,_ axis:V3=V3(0,1,0),_ translation:V3 = .zero)->simd_float4x4 {
    var m=simd_float4x4(simd_quatf(angle:angle,axis:normalize(axis)))
    m[3]=SIMD4(translation,1);return m
  }
  private func assertClose(_ a:simd_float4x4,_ b:simd_float4x4,accuracy:Float = 0.000002,file:StaticString = #filePath,line:UInt = #line) {
    for c in 0..<4 {for r in 0..<4 {XCTAssertEqual(a[c][r],b[c][r],accuracy:accuracy,file:file,line:line)}}
  }
  func testStrongTwistKeepsCrossSectionInsteadOfCollapsing() {
    let matrices=[rotation(0),rotation(Float.pi*0.9)]
    let palette=SkinningPalette(matrices),weight=SkinWeight(0,1,blend:0.5)
    XCTAssertEqual(palette.method,.dualQuaternion)
    for i in 0..<32 {
      let a=Float(i)*2*Float.pi/32,p=SIMD4<Float>(cos(a),0.5,sin(a),1)
      let result=palette.matrix(weight)*p,linear=weight.matrix(matrices)*p
      XCTAssertEqual(length(SIMD2(result.x,result.z)),1,accuracy:0.000001)
      XCTAssertLessThan(length(SIMD2(linear.x,linear.z)),0.16)
      XCTAssertEqual(result.y,p.y,accuracy:0.000001)
    }
  }
  func testRigidTransformAndTranslationReproduceBindToPoseMatrix() {
    for angle in stride(from:Float(-3.14),through:3.14,by:0.2) {
      let matrix=rotation(angle,V3(1,2,-3),V3(-0.6,1.2,4.9))
      assertClose(SkinningPalette([matrix]).matrix(SkinWeight(0)),matrix)
    }
    let palette=SkinningPalette([rotation(0),rotation(0,V3(0,1,0),V3(2,-1,3))])
    let result=palette.matrix(SkinWeight(0,1,blend:0.25))
    XCTAssertEqual(result[3],SIMD4(0.5,-0.25,0.75,1))
  }
  func testAntipodalRepresentationsTakeShortArcAndIgnoreZeroWeights() {
    let palette=SkinningPalette([rotation(3.12),rotation(-3.12)])
    let p=palette.matrix(SkinWeight(0,1,blend:0.5))*SIMD4<Float>(1,0,0,1)
    XCTAssertLessThan(p.x,-0.9999)
    XCTAssertEqual(p.z,0,accuracy:0.000001)
    var weight=SkinWeight(0);weight.joints=SIMD4(0,1,0,0);weight.weights=SIMD4(0,1,0,0)
    assertClose(palette.matrix(weight),rotation(-3.12))
  }
  func testDualBlendIsEquivariantUnderChangeOfWorldFrame() {
    let matrices=[rotation(0.9,V3(1,0,1),V3(1,2,3)),rotation(-0.7,V3(0,1,1),V3(-1,0.5,2))]
    let frame=rotation(1.2,V3(1,1,0),V3(4,-2,1)),weight=SkinWeight(0,1,blend:0.37)
    let posed=SkinningPalette(matrices).matrix(weight)
    let moved=SkinningPalette(matrices.map{frame*$0}).matrix(weight)
    assertClose(moved,frame*posed,accuracy:0.000004)
  }
  func testGuideScaleShearAndReflectionRetainExactAffineBlend() {
    var scale=rotation(0.9);scale[0] *= 1.1;scale[1] *= 0.8
    var shear=rotation(-0.4);shear[1] += shear[2]*0.2
    var reflection=rotation(0);reflection[0] *= -1
    var scaled=rotation(0);scaled[2] *= 1.0001
    for matrix in [scale,shear,reflection,scaled] {
      let matrices=[rotation(0.7),matrix],palette=SkinningPalette(matrices)
      XCTAssertEqual(palette.method,.linear)
      assertClose(palette.matrix(SkinWeight(0,1,blend:0.4)),SkinWeight(0,1,blend:0.4).matrix(matrices),accuracy:0)
      XCTAssertEqual(palette.encodedCount,2)
      assertClose(palette.encodedMatrices[1],matrix,accuracy:0)
    }
  }
  func testFourInfluencesAndSparseVertexAuditsUseSameRigidBlend() {
    let matrices=[rotation(0),rotation(1.2),rotation(2.2),rotation(-0.4)]
    let palette=SkinningPalette(matrices)
    var weight=SkinWeight(0);weight.joints=SIMD4(0,1,2,3);weight.weights=SIMD4(0.1,0.4,0.2,0.3)
    let mesh=ParametricMesh.ellipsoid(V3(1,1,0),V3(repeating:0.1),detail:8)
    let weights=Array(repeating:weight,count:mesh.vertices.count)
    let posed=DeformationAudit.deformed(mesh,weights:weights,palette:matrices)
    let expected=mesh.vertices.map{palette.matrix(weight)*$0.position}
    for i in expected.indices {XCTAssertLessThan(length(posed.vertices[i].position-expected[i]),0.000001)}
    let contact=SkinContactAudit.evaluate(mesh:mesh,weights:weights,palette:matrices,
      model:matrix_identity_float4x4,bodies:[],height:{_,_ in 2})
    XCTAssertEqual(contact.penetratingVertices,mesh.vertices.count)
    XCTAssertEqual(contact.position.y,expected[contact.vertex].y,accuracy:0.000001)
    XCTAssertEqual(contact.position.x,expected[contact.vertex].x,accuracy:0.000001)
    XCTAssertEqual(contact.position.z,expected[contact.vertex].z,accuracy:0.000001)
  }
  func testEncodedPalettePreservesExistingSixtyFourJointBudget() {
    let palette=SkinningPalette(Array(repeating:rotation(0.7),count:64))
    XCTAssertEqual(palette.encodedCount,0x80000040)
    XCTAssertEqual(palette.encodedMatrices.count*MemoryLayout<simd_float4x4>.stride,4096)
    XCTAssertEqual(MemoryLayout<SkinWeight>.stride,32)
    XCTAssertEqual(SkinningPalette([]).encodedCount,0)
  }
  func testReversalAuditReportsAreaAndSourceLocation() {
    var mesh=ParametricMesh.surface(u:1,v:1){u,v in V3(u,0,-v)}
    // Add a separate very small patch. Its count must not hide which reversed
    // patch contributes the greatest area or where an author should edit it.
    let tiny=ParametricMesh.surface(u:1,v:1){u,v in V3(2+u*0.001,0,-v*0.001)}
    let firstCount=mesh.vertices.count
    mesh.vertices += tiny.vertices
    mesh.indices += tiny.indices.map{$0+UInt32(firstCount)}
    var posed=mesh
    for i in posed.vertices.indices {posed.vertices[i].position.z *= -1}
    let report=DeformationAudit.inspect(rest:mesh,posed:posed)
    XCTAssertEqual(report.reversedTriangles,4)
    XCTAssertEqual(report.totalRestArea,1.000001,accuracy:0.000001)
    XCTAssertEqual(report.reversedRestArea,report.totalRestArea,accuracy:0.000001)
    XCTAssertLessThan(report.worstReversedTriangle,2)
    XCTAssertLessThan(report.worstReversedRestPosition.x,1)
    XCTAssertLessThan(report.worstReversedRestPosition.z,0)
    XCTAssertGreaterThan(report.worstReversedPosition.z,0)
  }
}
