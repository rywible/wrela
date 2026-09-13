import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

final class CanopyGliderDesignTests: XCTestCase {
  private struct Specimen {
    let batches: [SceneBatch]
    let joints: [PartJoint]
    let animation: AnimationDefinition
  }
  private func compile(_ parameters: [String: Float] = [:]) throws -> Specimen {
    let generator = try XCTUnwrap(LivingWorldPresentation.generators.first { $0.id == "canopyGlider" })
    let source = AssetSource(id: "canopyGlider", name: "Canopy Glider", generator: "canopyGlider",
      parameters: parameters)
    let compile = try XCTUnwrap(generator.compile)
    let joints = generator.parts(parameters).compactMap(\.joint)
    try PartRig.validate(joints)
    return Specimen(batches: try compile(source), joints: joints,
      animation: try XCTUnwrap(generator.animation))
  }

  func testActualRegisteredMeshesRetainStablePartsAndBoundedFiniteGeometryAcrossControls() throws {
    for parameters: [String: Float] in [[:],
      ["muzzleWidth": 0.9, "wingCamber": 0.8, "eyeSize": 0.9],
      ["muzzleWidth": 1.1, "wingCamber": 1.15, "eyeSize": 1.1]] {
      let sample = try compile(parameters)
      let names = Set(sample.batches.map(\.name))
      for suffix in ["body", "head", "eyes", "glider mantle", "ribbon tail"] {
        XCTAssertTrue(names.contains(SanctuaryCanopyGliderDesign.prefix + suffix))
      }
      XCTAssertEqual(names.count, sample.batches.count)
      XCTAssertTrue(sample.animation.clips.contains("flight"))
      let triangles = sample.batches.reduce(0) { $0 + $1.mesh.indices.count / 3 }
      let bytes = sample.batches.reduce(0) {
        $0 + $1.mesh.vertices.count * MemoryLayout<Vertex>.stride + $1.mesh.indices.count * 4
      }
      XCTAssertLessThanOrEqual(triangles, 18_000)
      XCTAssertLessThanOrEqual(bytes, 2 * 1_024 * 1_024)
      for batch in sample.batches {
        for v in batch.mesh.vertices {
          for component in 0..<3 {
            XCTAssertTrue(v.position[component].isFinite)
            XCTAssertTrue(v.normal[component].isFinite)
          }
          XCTAssertEqual(length(v.normal), 1, accuracy: 0.002)
          XCTAssertLessThanOrEqual(abs(v.position.x), 0.72)
          XCTAssertLessThanOrEqual(v.position.y, 0.70)
          XCTAssertGreaterThan(v.position.y, 0.14)
        }
      }
      print("CanopyGlider compiled triangles=\(triangles) rawBytes=\(bytes) parameters=\(parameters)")
    }
  }

  func testCompiledEyeLensesStaySeatedOnActualHeadSourceAtSemanticCorners() throws {
    for scale: Float in [0.9, 1, 1.1] {
      let sample = try compile(["muzzleWidth": scale, "eyeSize": scale])
      let eyes = try XCTUnwrap(sample.batches.first { $0.name == "Wildlife canopyGlider eyes" })
      let head = SanctuaryCanopyGliderDesign.headEnvelope(muzzle: scale)
      let joint = try XCTUnwrap(sample.joints.first { $0.id == eyes.name })
      XCTAssertEqual(joint.parent, "Wildlife canopyGlider head")
      for v in eyes.mesh.vertices {
        let support = head.sample(at: V3(v.position.x, v.position.y, v.position.z))
        // This is the source field's local distance estimate, not an exact SDF.
        XCTAssertGreaterThan(support.value, 0)
        XCTAssertLessThan(support.value, 0.006)
        XCTAssertGreaterThan(dot(normalize(support.gradient), V3(v.normal.x, v.normal.y, v.normal.z)), 0.98)
      }
    }
  }

  func testProductionPosedCameraEnvelopeAndReplayRemainClearAtFlightTimes() throws {
    let sample = try compile()
    let attachment = SanctuaryMountAttachment.recipe(for: .canopyGlider, mode: .flying)
    let state = sample.animation.initialize(17)
    let motion = MotionParameters(Dictionary(uniqueKeysWithValues:
      sample.animation.controls.map { ($0.key, $0.initial) }))
    let mantle = try XCTUnwrap(sample.batches.first { $0.name == "Wildlife canopyGlider glider mantle" })
    let rootVertices = mantle.mesh.vertices.filter { abs($0.position.x) < 0.046 }
    XCTAssertFalse(rootVertices.isEmpty)
    let torso = SanctuaryCanopyGliderDesign.bodyEnvelope()
    for scale: Float in [0.62, 1] {
      let eye = attachment.eyeInBindSpace * scale + V3(0, attachment.eyeClearance, 0)
      for frame in 0..<24 {
        let time = Float(frame) * 0.43
        XCTAssertEqual(sample.animation.poses("flight", time, state, motion),
          SanctuaryCanopyGliderDesign.poses(time: time, gaze: 0, mood: "idle"))
        let poses = SanctuaryCanopyGliderDesign.poses(time: time, gaze: frame % 2 == 0 ? -16 : 16,
          mood: "idle")
        XCTAssertEqual(poses, SanctuaryCanopyGliderDesign.poses(time: time,
          gaze: frame % 2 == 0 ? -16 : 16, mood: "idle"))
        let matrices = PartRig.matrices(sample.joints, poses: poses)
        let mantleMatrix = try XCTUnwrap(matrices[mantle.name])
        for vertex in rootVertices {
          let p = mantleMatrix * vertex.position
          XCTAssertLessThan(torso.sample(at: V3(p.x, p.y, p.z)).value, -0.005,
            "Posed membrane root must stay at least 5 mm inside source torso")
        }
        for batch in sample.batches {
          let matrix = try XCTUnwrap(matrices[batch.name])
          let points = batch.mesh.vertices.map { v -> V3 in
            let p = matrix * v.position
            return V3(p.x, p.y, p.z) * scale
          }
          for p in points {
            XCTAssertLessThan(p.y, eye.y - 0.16)
            XCTAssertTrue(p.x.isFinite && p.y.isFinite && p.z.isFinite)
          }
          for pitch: Float in [0, -0.08, -0.38] {
            let direction = V3(0, sin(pitch), -cos(pitch))
            for index in stride(from: 0, to: batch.mesh.indices.count, by: 3) {
              let a = points[Int(batch.mesh.indices[index])]
              let b = points[Int(batch.mesh.indices[index + 1])]
              let c = points[Int(batch.mesh.indices[index + 2])]
              XCTAssertFalse(intersects(eye, direction, a, b, c),
                "Camera ray hit \(batch.name), t=\(time), pitch=\(pitch)")
            }
          }
        }
      }
    }
  }
  func testClosedLidCreasesRemainOutsideSkullButBehindUnchangedOpenLenses() throws {
    for eyeSize: Float in [0.9, 1, 1.1] {
      let sample = try compile(["eyeSize": eyeSize])
      let eyes = try XCTUnwrap(sample.batches.first { $0.name == "Wildlife canopyGlider eyes" })
      let creases = try XCTUnwrap(sample.batches.first { $0.name == "Wildlife canopyGlider eyelid creases" })
      let head = SanctuaryCanopyGliderDesign.headEnvelope()
      let creaseJoint = try XCTUnwrap(sample.joints.first { $0.id == creases.name })
      XCTAssertEqual(creaseJoint.parent, "Wildlife canopyGlider head")
      let closed = SanctuaryCanopyGliderDesign.poses(time: 3.85, gaze: 0, mood: "idle")
      XCTAssertLessThan(try XCTUnwrap(closed[eyes.name]).scale.y, 0.07)
      XCTAssertNil(closed[creases.name], "Creases inherit only the head; never squash them into the skull")
      let matrices = PartRig.matrices(sample.joints, poses: closed)
      let headMatrix = try XCTUnwrap(matrices["Wildlife canopyGlider head"])
      let creaseMatrix = try XCTUnwrap(matrices[creases.name])
      var minimumOpenOcclusion: Float = .infinity
      for vertex in creases.mesh.vertices {
        let point = V3(vertex.position.x, vertex.position.y, vertex.position.z)
        let n = normalize(head.sample(at: point).gradient)
        XCTAssertGreaterThan(head.sample(at: point).value, 0.0006)
        let posed = headMatrix.inverse * creaseMatrix * vertex.position
        XCTAssertLessThan(length(V3(posed.x, posed.y, posed.z) - point), 0.000001)
        let origin = point + n * 0.02
        var firstHit: Float = .infinity
        for index in stride(from: 0, to: eyes.mesh.indices.count, by: 3) {
          func p(_ offset: Int) -> V3 {
            let v = eyes.mesh.vertices[Int(eyes.mesh.indices[index + offset])].position
            return V3(v.x, v.y, v.z)
          }
          if let hit = rayDistance(origin, -n, p(0), p(1), p(2)) { firstHit = min(firstHit, hit) }
        }
        XCTAssertLessThan(firstHit, 0.0199, "Open lens must occlude the new crease by at least 0.1 mm")
        minimumOpenOcclusion = min(minimumOpenOcclusion, 0.02 - firstHit)
      }
      print("CanopyGlider eyelid open occlusion minimum=\(minimumOpenOcclusion)m eyeSize=\(eyeSize)")
    }
  }

  func testDeclaredFlightEnvelopeContainsActualRegisteredRootAndArticulatedMeshes() throws {
    for parameters: [String: Float] in [[:],
      ["muzzleWidth": 0.9, "wingCamber": 0.8, "eyeSize": 0.9],
      ["muzzleWidth": 1.1, "wingCamber": 1.15, "eyeSize": 1.1]] {
      let sample = try compile(parameters)
      let envelope = try XCTUnwrap(sample.animation.motionEnvelope)
      let state = sample.animation.initialize(17)
      let motion = MotionParameters(Dictionary(uniqueKeysWithValues:
        sample.animation.controls.map { ($0.key, $0.initial) }))
      for clip in ["bind", "flight"] {
        for frame in 0..<24 {
          let time = Float(frame) * 0.43
          let root = sample.animation.root(clip, time, state, motion)
          let poses = sample.animation.poses(clip, time, state, motion)
          let matrices = PartRig.matrices(sample.joints, poses: poses)
          for batch in sample.batches {
            let matrix = root * (matrices[batch.name] ?? matrix_identity_float4x4)
            for vertex in batch.mesh.vertices {
              let point = matrix * vertex.position
              for axis in 0..<3 {
                XCTAssertGreaterThanOrEqual(point[axis], envelope.min[axis])
                XCTAssertLessThanOrEqual(point[axis], envelope.max[axis])
              }
            }
          }
        }
      }
    }
  }

  private func intersects(_ origin: V3, _ ray: V3, _ a: V3, _ b: V3, _ c: V3) -> Bool {
    rayDistance(origin, ray, a, b, c) != nil
  }
  private func rayDistance(_ origin: V3, _ ray: V3, _ a: V3, _ b: V3, _ c: V3) -> Float? {
    let e1 = b - a, e2 = c - a, p = cross(ray, e2), determinant = dot(e1, p)
    guard abs(determinant) > 1e-8 else { return nil }
    let t = origin - a, u = dot(t, p) / determinant
    guard u >= 0, u <= 1 else { return nil }
    let q = cross(t, e1), v = dot(ray, q) / determinant
    guard v >= 0 && u + v <= 1 else { return nil }
    let distance = dot(e2, q) / determinant
    return distance > 0 ? distance : nil
  }
}
