import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

/// Numerical checks of the production source and registered performance. No
/// renderer, alternate gait, player save, image threshold or fixture mesh.
final class SunhareSemanticGeometryTests: XCTestCase {
  private typealias Part = LivingWorldPresentation.PartDesign
  private struct Specimen {
    var generator: AssetGenerator
    var animation: AnimationDefinition
    var parts: [Part]
    var joints: [PartJoint]
    var meshes: [String: Mesh]
    var motion: MotionParameters
  }
  private static let compiled: Result<Specimen, Error> = Result {
    guard let generator = LivingWorldPresentation.generators.first(where: { $0.id == "sunhare" }),
      let animation = generator.animation, let compile = generator.compile else {
      throw NSError(domain: "SunhareSemanticGeometryTests", code: 1)
    }
    let parameters = Dictionary(uniqueKeysWithValues: generator.controls.map { ($0.key, $0.initial) })
    let source = AssetSource(id: "sunhare", name: "Sunhare", generator: "sunhare", parameters: parameters)
    let batches = try compile(source)
    return Specimen(generator: generator, animation: animation,
      parts: SanctuarySunhareDesign.parts(parameters),
      joints: generator.parts(parameters).compactMap(\.joint),
      meshes: Dictionary(uniqueKeysWithValues: batches.map { ($0.name, $0.mesh) }),
      motion: MotionParameters(Dictionary(uniqueKeysWithValues: animation.controls.map { ($0.key, $0.initial) })))
  }
  private func point(_ value: SIMD4<Float>) -> V3 { V3(value.x, value.y, value.z) }
  private func finite(_ value: V3) -> Bool { (0..<3).allSatisfy { value[$0].isFinite } }
  private func distance(_ a: simd_float4x4, _ b: simd_float4x4) -> Float {
    (0..<4).map { simd_length(a[$0] - b[$0]) }.max() ?? 0
  }
  private func pose(_ sample: Specimen, clip: String, time: Float, state: BehaviorSnapshot)
    -> (root: simd_float4x4, parts: [String: simd_float4x4]) {
    let values = sample.animation.poses(clip, time, state, sample.motion)
    return (sample.animation.root(clip, time, state, sample.motion),
      PartRig.matrices(sample.joints, poses: values))
  }
  private func soles(_ sample: Specimen,
    _ frame: (root: simd_float4x4, parts: [String: simd_float4x4])) -> [String: Float] {
    var result: [String: Float] = [:]
    for part in sample.parts {
      guard case .leg = part.role, let mesh = sample.meshes[part.name] else { continue }
      let matrix = frame.root * (frame.parts[part.name] ?? matrix_identity_float4x4)
      result[part.name] = mesh.vertices.map { (matrix * $0.position).y }.min()
    }
    return result
  }

  func testSavedCoatStudyChangesOnlyEligibleMaterialIDsAndPreservesActualGeometry() throws {
    let sample = try Self.compiled.get()
    let source = AssetSource(id: "sunhare", name: "Sunhare", generator: "sunhare",
      parameters: ["coatStudy": 1])
    let restored = try JSONDecoder().decode(AssetSource.self, from: JSONEncoder().encode(source))
    XCTAssertEqual(restored.parameters["coatStudy"], 1)
    XCTAssertEqual(sample.generator.controls.first { $0.key == "coatStudy" }?.initial, 0)
    let candidate = SanctuarySunhareDesign.parts(restored.parameters)
    let expected: Set<String> = ["body", "head", "ear-left", "ear-right", "front-left",
      "front-right", "hind-left", "hind-right"]
    XCTAssertEqual(sample.parts.map(\.name), candidate.map(\.name))
    for (before, after) in zip(sample.parts, candidate) {
      let suffix = String(before.name.dropFirst("Wildlife sunhare ".count))
      XCTAssertEqual(after.material, expected.contains(suffix) ? 15 : before.material)
      XCTAssertEqual(before.color, after.color)
      XCTAssertEqual(before.roughness, after.roughness)
      XCTAssertEqual(before.metallic, after.metallic)
      XCTAssertEqual(before.pivot, after.pivot)
      XCTAssertEqual(before.parent, after.parent)
      XCTAssertEqual(before.shape.bounds.min, after.shape.bounds.min)
      XCTAssertEqual(before.shape.bounds.max, after.shape.bounds.max)
    }
    let compile = try XCTUnwrap(sample.generator.compile)
    for batch in try compile(restored) {
      let before = try XCTUnwrap(sample.meshes[batch.name])
      XCTAssertEqual(before.indices, batch.mesh.indices, batch.name)
      XCTAssertEqual(before.vertices.map(\.position), batch.mesh.vertices.map(\.position), batch.name)
      XCTAssertEqual(before.vertices.map(\.normal), batch.mesh.vertices.map(\.normal), batch.name)
      XCTAssertEqual(before.vertices.map(\.color), batch.mesh.vertices.map(\.color), batch.name)
    }
    // Switching back cannot retrieve the candidate material descriptor from the cache.
    XCTAssertEqual(SanctuarySunhareDesign.parts(["coatStudy": 0]).map(\.material), sample.parts.map(\.material))
  }

  func testActualSourceNormalsAreFiniteUnitAndOutwardAtEveryVertex() throws {
    let sample = try Self.compiled.get()
    var worstAgreement: Float = 1
    var checked = 0
    for part in sample.parts {
      let mesh = try XCTUnwrap(sample.meshes[part.name])
      for vertex in mesh.vertices {
        let p = point(vertex.position), n = point(vertex.normal)
        XCTAssertTrue(finite(p) && finite(n), part.name)
        XCTAssertEqual(simd_length(n), 1, accuracy: 0.00001, part.name)
        let agreement = dot(n, part.shape.normal(at: p))
        worstAgreement = min(worstAgreement, agreement)
        XCTAssertGreaterThan(agreement, part.proceduralMesh != nil ? 0.999 : 0, part.name)
        checked += 1
      }
    }
    XCTAssertGreaterThan(checked, 1_000)
    print("Sunhare generated normals: \(checked) vertices, minimum outward dot \(worstAgreement)")
  }

  func testSemanticControlCornersKeepNeckAndEarAttachmentsInsideBothSurfaces() throws {
    let sample = try Self.compiled.get()
    let corners: [[String: Float]] = [[:], ["headWidth": 0.88, "cheeks": 1.15],
      ["headWidth": 1.15, "cheeks": 0.85], ["earLength": 0.50, "earSpread": 25],
      ["earLength": 0.28, "earSpread": 8], ["pawSize": 0.85, "bodyRoundness": 1.15]]
    for parameters in corners {
      let parts = SanctuarySunhareDesign.parts(parameters)
      let fields = Dictionary(uniqueKeysWithValues: parts.map { ($0.name, $0.shape) })
      let joints = sample.generator.parts(parameters).compactMap(\.joint)
      try PartRig.validate(joints)
      for part in parts {
        try part.shape.validate()
        XCTAssertTrue(finite(part.shape.bounds.min) && finite(part.shape.bounds.max))
      }
      for joint in joints where joint.id == "Wildlife sunhare head"
        || joint.id == "Wildlife sunhare ear-left" || joint.id == "Wildlife sunhare ear-right" {
        let own = try XCTUnwrap(fields[joint.id])
        let parent = try XCTUnwrap(joint.parent.flatMap { fields[$0] })
        XCTAssertLessThan(own.value(at: joint.pivot), 0, "Own attachment must be inside \(joint.id)")
        XCTAssertLessThan(parent.value(at: joint.pivot), 0, "Parent must enclose \(joint.id) attachment")
      }
      let nose = try XCTUnwrap(fields["Wildlife sunhare nose"])
      let head = try XCTUnwrap(fields["Wildlife sunhare head"])
      XCTAssertLessThan(nose.bounds.min.z, (head.bounds.min.z + head.bounds.max.z) * 0.5,
        "Creature forward is -Z")
      XCTAssertGreaterThan(try XCTUnwrap(fields["Wildlife sunhare ear-left"]).bounds.max.y,
        head.bounds.max.y, "Creature up is +Y")
    }
  }

  func testEyeCapsStraddleTheActualHeadSurfaceWithoutBecomingDetachedBeads() throws {
    let sample = try Self.compiled.get()
    let head = try XCTUnwrap(sample.parts.first { $0.name == "Wildlife sunhare head" })
    let eyes = try XCTUnwrap(sample.meshes["Wildlife sunhare eyes"])
    let distances = eyes.vertices.map { head.shape.value(at: point($0.position)) }
    XCTAssertLessThan(try XCTUnwrap(distances.min()), -0.004, "Cap must seat inside the skull")
    XCTAssertGreaterThan(try XCTUnwrap(distances.max()), 0.001, "Cornea must remain visible above fur")
    XCTAssertLessThan(try XCTUnwrap(distances.max()), 0.008, "Shallow cap protrusion exceeded8 mm field bound")
    print("Sunhare eye/head field values: \(distances.min() ?? 0)...\(distances.max() ?? 0) m; conservative field distances, not exact closest distances")
  }

  private struct PositionKey: Hashable {
    var x: UInt32, y: UInt32, z: UInt32
    init(_ p: SIMD4<Float>) { x = p.x.bitPattern; y = p.y.bitPattern; z = p.z.bitPattern }
  }
  func testEarMaterialRegionsShareExactSeamAndInheritOneAnimatedAttachment() throws {
    let sample = try Self.compiled.get()
    let state = sample.animation.initialize(17)
    for side in ["left", "right"] {
      let outerID = "Wildlife sunhare ear-" + side
      let innerID = "Wildlife sunhare inner-ear-" + side
      let outer = try XCTUnwrap(sample.meshes[outerID]), inner = try XCTUnwrap(sample.meshes[innerID])
      var outerVertices: [PositionKey: Vertex] = [:]
      for vertex in outer.vertices { outerVertices[PositionKey(vertex.position)] = vertex }
      func triangleKeys(_ mesh: Mesh) -> Set<[PositionKey]> {
        var result: Set<[PositionKey]> = []
        for start in stride(from: 0, to: mesh.indices.count, by: 3) {
          var keys: [PositionKey] = []
          for offset in 0..<3 {
            let vertex = mesh.vertices[Int(mesh.indices[start + offset])]
            keys.append(PositionKey(vertex.position))
          }
          keys.sort { a, b in
            if a.x != b.x { return a.x < b.x }
            if a.y != b.y { return a.y < b.y }
            return a.z < b.z
          }
          result.insert(keys)
        }
        return result
      }
      XCTAssertTrue(triangleKeys(outer).isDisjoint(with: triangleKeys(inner)),
        "Material regions must not contain overlapping triangles")
      let shared = inner.vertices.compactMap { vertex -> (Vertex, Vertex)? in
        outerVertices[PositionKey(vertex.position)].map { ($0, vertex) }
      }
      XCTAssertGreaterThan(shared.count, 10, "Lining must share a real boundary")
      for (a, b) in shared {
        XCTAssertEqual(a.normal, b.normal)
        XCTAssertEqual(a.color, b.color)
      }
      for time: Float in [0, 0.0935, 0.3995, 0.612, 0.731] {
        let frame = pose(sample, clip: "hop", time: time, state: state)
        XCTAssertLessThan(distance(try XCTUnwrap(frame.parts[outerID]),
          try XCTUnwrap(frame.parts[innerID])), 0.000001,
          "Ear lining must follow the outer ear's complete transform")
      }
    }
  }

  func testActualCompiledSolesStayPlantedThroughHopPreparationAndSettle() throws {
    let sample = try Self.compiled.get()
    let state = sample.animation.initialize(17)
    let duration = sample.animation.duration(sample.motion)
    let bind = soles(sample, pose(sample, clip: "bind", time: 0, state: state))
    XCTAssertEqual(bind.count, 4)
    for (id, height) in bind { XCTAssertEqual(height, 0, accuracy: 0.0005, id) }
    var minimumClearance = Float.greatestFiniteMagnitude
    var maximumStanceChange: Float = 0
    let phases = (0...51).map { Float($0) / 51 } + [0.22, 0.72]
    for phase in phases {
      let time = duration * phase
      let frame = pose(sample, clip: "hop", time: time, state: state)
      for (id, height) in soles(sample, frame) {
        minimumClearance = min(minimumClearance, height)
        XCTAssertGreaterThanOrEqual(height, -0.0005, "Sole below support during hop: \(id), phase \(phase)")
        if phase <= 0.22 || phase >= 0.72 {
          let change = abs(height - (bind[id] ?? 0))
          maximumStanceChange = max(maximumStanceChange, change)
          XCTAssertLessThan(change, 0.00001, "Planted sole moved: \(id), phase \(phase)")
        }
      }
    }
    print("Sunhare actual mesh hop: minimum sole Y \(minimumClearance) m, maximum stance change \(maximumStanceChange) m")
  }

  func testProductionBrainSweepReplaysAndKeepsSocialSolesAttached() throws {
    let sample = try Self.compiled.get()
    let scenarios = ["greeting-side", "greeting-rear", "refused-play", "accepted-wait"]
    var maximumReplayError: Float = 0
    var minimumSocialSole = Float.greatestFiniteMagnitude
    for scenario in scenarios {
      var state = sample.animation.initialize(17)
      var replay = state
      for tick in 0...180 {
        if tick > 0 {
          let stimulus = sample.animation.input(scenario, Float(tick - 1) / 60)
          state = sample.animation.step(state, stimulus, sample.motion)
          replay = sample.animation.step(replay, stimulus, sample.motion)
        }
        if tick == 45 {
          replay = try JSONDecoder().decode(BehaviorSnapshot.self,
            from: JSONEncoder().encode(replay))
        }
        let time = Float(tick) / 60
        let frame = pose(sample, clip: "behavior", time: time, state: state)
        let restored = pose(sample, clip: "behavior", time: time, state: replay)
        XCTAssertEqual(state.data, replay.data, "Serialized production brain diverged")
        maximumReplayError = max(maximumReplayError, distance(frame.root, restored.root))
        for joint in sample.joints {
          let a = try XCTUnwrap(frame.parts[joint.id]), b = try XCTUnwrap(restored.parts[joint.id])
          maximumReplayError = max(maximumReplayError, distance(a, b))
          for column in 0..<4 { XCTAssertTrue((0..<4).allSatisfy { a[column][$0].isFinite }) }
        }
        // No departure/travel is requested by these scenarios. The real brain
        // may turn to the speaker, but its stance root must not gain fake lift.
        if !state.hopping {
          for (id, y) in soles(sample, frame) {
            minimumSocialSole = min(minimumSocialSole, y)
            XCTAssertGreaterThanOrEqual(y, -0.0005, "Social sole below support: \(scenario), \(id)")
            XCTAssertLessThan(abs(y), 0.0005, "Social acting lifted stance sole: \(scenario), \(id)")
          }
        }
      }
      XCTAssertEqual(state.fear, 0, "A social performance must not invent a fear penalty")
    }
    XCTAssertEqual(maximumReplayError, 0)
    print("Sunhare production social sweep: exact replay matrix error \(maximumReplayError), minimum stance sole \(minimumSocialSole) m")
  }

  func testIdleBlinkClosesEyesWithoutChangingHeadOrSupportRig() throws {
    let sample = try Self.compiled.get(), state = sample.animation.initialize(17)
    let bind = soles(sample, pose(sample, clip: "bind", time: 0, state: state))
    let eyeJoint = try XCTUnwrap(sample.joints.first { $0.id == "Wildlife sunhare eyes" })
    var minimumLidScale: Float = 1
    for tick in 0...360 {
      let time = Float(tick) / 60
      let values = sample.animation.poses("idle", time, state, sample.motion)
      if let eye = values["Wildlife sunhare eyes"] { minimumLidScale = min(minimumLidScale, eye.scale.y) }
      if tick % 6 == 0 {
        let frame = pose(sample, clip: "idle", time: time, state: state)
        let eyes = try XCTUnwrap(frame.parts[eyeJoint.id])
        let head = try XCTUnwrap(frame.parts["Wildlife sunhare head"])
        XCTAssertLessThan(simd_length((eyes - head) * SIMD4(eyeJoint.pivot, 1)), 0.000001,
          "Blinking must not detach the eye anchor from its head")
        for (id, y) in soles(sample, frame) {
          XCTAssertEqual(y, try XCTUnwrap(bind[id]), accuracy: 0.00001)
        }
      }
    }
    XCTAssertLessThan(minimumLidScale, 0.2, "The real idle clip must include a full blink")
    XCTAssertGreaterThanOrEqual(minimumLidScale, 0.05, "The eyelid cannot invert")
  }
}
