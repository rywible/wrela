import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

/// Drives accepted production travel, then evaluates the registered rig and actual meshes.
/// The open route is fixture support; signal accumulation, poses, IK and skinning are production.
final class MountedCreatureMotionTests: XCTestCase {
  private let id = "moonhart-001"
  private let openRoute: (SIMD2<Float>, SIMD2<Float>) -> Bool = { _, _ in true }
  private struct Specimen {
    let joints: [PartJoint]
    let batches: [SceneBatch]
    let animation: AnimationDefinition
    let motion: MotionParameters
  }
  private static func compile(_ name: String) throws -> Specimen {
    let generator = try XCTUnwrap(LivingWorldPresentation.generators.first { $0.id == name })
    let compile = try XCTUnwrap(generator.compile)
    let animation = try XCTUnwrap(generator.animation)
    let parameters = Dictionary(uniqueKeysWithValues: generator.controls.map { ($0.key, $0.initial) })
    let source = AssetSource(id: name, name: name, generator: name, parameters: parameters)
    let joints = generator.parts(parameters).compactMap(\.joint)
    try PartRig.validate(joints)
    return Specimen(joints: joints, batches: try compile(source), animation: animation,
      motion: MotionParameters(Dictionary(uniqueKeysWithValues: animation.controls.map { ($0.key, $0.initial) })))
  }
  private static let moonhart = Result { try compile("moonhart") }
  private static let ray = Result { try compile("cloudRay") }

  private func attached() throws -> WildlifePopulation {
    var population = WildlifePopulation.initial(seed: 17)
    var visitor = try XCTUnwrap(population.actor(id: id)).position + SIMD2<Float>(0, 6)
    _ = try population.address(.greeting, targetID: id, player: visitor,
      expectedRevision: population.revision, isVisible: { $0.id == self.id })
    for _ in 0..<10 {
      try population.advance(seconds: 1, player: visitor, running: false, garden: HabitatGarden(),
        canTraverse: openRoute, isVisible: { $0.id == self.id })
    }
    for _ in 0..<8 where population.actor(id: id)?.relationship.isWilling(to: .follow) == false {
      visitor = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(0, 2)
      let result = try population.address(.play, targetID: id, player: visitor,
        expectedRevision: population.revision, isVisible: { $0.id == self.id })
      XCTAssertTrue(result.accepted)
      for seconds: Float in [1, 1, 0.5] {
        try population.advance(seconds: seconds, player: visitor, running: false, garden: HabitatGarden(),
          canTraverse: openRoute, isVisible: { $0.id == self.id })
      }
    }
    XCTAssertTrue(population.actor(id: id)?.relationship.isWilling(to: .follow) == true)
    let start = try XCTUnwrap(population.actor(id: id)?.position)
    try population.updateCompanionPosition(id: id, position: start, using: .ride,
      expectedRevision: population.revision, canTraverse: openRoute)
    return population
  }
  private func tick(_ population: inout WildlifePopulation) throws {
    let position = try XCTUnwrap(population.actor(id: id)?.position)
    try population.advance(seconds: 1 / 60, player: position, running: false, garden: HabitatGarden(),
      mountedID: id, canTraverse: openRoute, isVisible: { $0.id == self.id })
  }
  private func move(_ population: inout WildlifePopulation, _ delta: SIMD2<Float>) throws {
    try tick(&population)
    let position = try XCTUnwrap(population.actor(id: id)?.position) + delta
    try population.updateCompanionPosition(id: id, position: position, using: .ride,
      expectedRevision: population.revision, canTraverse: openRoute)
  }
  private func matrices(_ sample: Specimen, _ population: WildlifePopulation,
    yaw: Float = 0, scale: Float = 1, time: Float = 0) throws -> [String: simd_float4x4] {
    let signal = try XCTUnwrap(population.actor(id: id)?.mountedTravel)
    return PartRig.matrices(sample.joints, poses: SanctuaryMountMotion.mountedPoses(
      signal: signal, actorYaw: yaw, morphologyScale: scale, time: time, gaze: 0, mood: "idle"))
  }
  private func error(_ a: simd_float4x4, _ b: simd_float4x4) -> Float {
    (0..<4).map { length(a[$0] - b[$0]) }.max() ?? 0
  }
  private func assertLegsEqual(_ a: [String: simd_float4x4], _ b: [String: simd_float4x4],
    accuracy: Float = 0.00001, file: StaticString = #filePath, line: UInt = #line) throws {
    let names = ["Wildlife moonhart body"] + SanctuaryMountDesign.legs.flatMap {
      [$0.upperName, $0.lowerName, $0.hoofName]
    }
    for name in names {
      XCTAssertLessThan(error(try XCTUnwrap(a[name]), try XCTUnwrap(b[name])), accuracy,
        name, file: file, line: line)
    }
  }

  func testCardinalAcceptedTravelKeepsActualStanceVerticesStationaryForEveryRootHeading() throws {
    let sample = try Self.moonhart.get(), initial = try attached()
    let directions: [SIMD2<Float>] = [SIMD2(0, -1), SIMD2(1, 0), SIMD2(0, 1), SIMD2(-1, 0)]
    let hoof = try XCTUnwrap(sample.batches.first { $0.name == SanctuaryMountDesign.legs[0].hoofName })
    let sole = try XCTUnwrap(hoof.mesh.vertices.min { $0.position.y < $1.position.y }).position
    for direction in directions {
      var population = initial
      let origin = try XCTUnwrap(population.actor(id: id)?.position)
      try move(&population, direction * 0.125)
      let before = population
      try move(&population, direction * 0.03125)
      for yaw: Float in [0, .pi / 2, .pi, -.pi / 2] {
        for scale: Float in [0.62, 1] {
          func worldSole(_ population: WildlifePopulation) throws -> SIMD4<Float> {
            let delta = try XCTUnwrap(population.actor(id: id)?.position) - origin
            let root = transform(V3(delta.x, 0, delta.y), V3(repeating: scale), yaw)
            let pose = try matrices(sample, population, yaw: yaw, scale: scale)
            return root * (try XCTUnwrap(pose[hoof.name])) * sole
          }
          let a = try worldSole(before), b = try worldSole(population)
          XCTAssertLessThan(length(a - b), 0.0005, "Stance skates for root yaw \(yaw), travel \(direction)")
          XCTAssertEqual(b.y, 0, accuracy: 0.0005)
        }
      }
    }
  }

  func testAcceptedStrideSweepHasFiniteRigAndAtLeastTwoActualGroundContacts() throws {
    let sample = try Self.moonhart.get(), initial = try attached()
    let hooves = sample.batches.filter { $0.name.hasPrefix("Wildlife moonhart hoof ") }
    XCTAssertEqual(hooves.count, 4)
    for direction: SIMD2<Float> in [SIMD2(0, -1), SIMD2(1, 0), SIMD2(0, 1), SIMD2(-1, 0)] {
      var population = initial
      for _ in 0..<32 {
        try move(&population, direction * 0.03125)
        for scale: Float in [0.62, 0.81, 1] {
          let poses = try matrices(sample, population, scale: scale)
          for (name, matrix) in poses {
            XCTAssertTrue((0..<4).allSatisfy { column in
              (0..<4).allSatisfy { matrix[column][$0].isFinite }
            }, name)
          }
          var contacts = 0
          for hoof in hooves {
            let matrix = try XCTUnwrap(poses[hoof.name])
            let minimum = try XCTUnwrap(hoof.mesh.vertices.map { (matrix * $0.position).y * scale }.min())
            XCTAssertGreaterThanOrEqual(minimum, -0.0005, hoof.name)
            if abs(minimum) < 0.0005 { contacts += 1 }
          }
          XCTAssertGreaterThanOrEqual(contacts, 2, "Missed support at \(population.actor(id: id)?.mountedTravel.cycleDistance ?? -1) m")
        }
      }
    }
  }

  func testStopReopenRejectedMoveAndRestartUseSavedAcceptedPhase() throws {
    let sample = try Self.moonhart.get()
    var population = try attached()
    try move(&population, SIMD2(0, -0.625))
    var restored = try JSONDecoder().decode(WildlifePopulation.self, from: JSONEncoder().encode(population))
    try assertLegsEqual(matrices(sample, population), matrices(sample, restored))
    for _ in 0..<3 { try tick(&population); try tick(&restored) }
    XCTAssertFalse(try XCTUnwrap(population.actor(id: id)?.mountedTravel).isMoving)
    try assertLegsEqual(matrices(sample, population, time: 0), matrices(sample, population, time: 103))
    let stopped = try matrices(sample, population)
    for hoof in sample.batches where hoof.name.hasPrefix("Wildlife moonhart hoof ") {
      let matrix = try XCTUnwrap(stopped[hoof.name])
      let sole = try XCTUnwrap(hoof.mesh.vertices.map { (matrix * $0.position).y }.min())
      XCTAssertEqual(sole, 0, accuracy: 0.0005, "Stopped hoof must plant: \(hoof.name)")
    }
    let prior = population
    let position = try XCTUnwrap(population.actor(id: id)?.position)
    XCTAssertThrowsError(try population.updateCompanionPosition(id: id, position: position + SIMD2(1, 0),
      using: .ride, expectedRevision: population.revision, canTraverse: { _, _ in false }))
    XCTAssertEqual(population, prior)
    try assertLegsEqual(matrices(sample, population), matrices(sample, prior))
    try move(&population, SIMD2(0, -0.125)); try move(&restored, SIMD2(0, -0.125))
    XCTAssertTrue(try XCTUnwrap(population.actor(id: id)?.mountedTravel).isMoving)
    try assertLegsEqual(matrices(sample, population), matrices(sample, restored))
  }

  func testExactCycleWrapRetainsActiveStanceAndNextStrideMatchesShortTravel() throws {
    let sample = try Self.moonhart.get(), initial = try attached()
    var long = initial
    for _ in 0..<8 { try move(&long, SIMD2(0, -8)) }
    XCTAssertEqual(try XCTUnwrap(long.actor(id: id)?.mountedTravel).cycleDistance, 0)
    for _ in 0..<3 { try tick(&long) }
    try assertLegsEqual(matrices(sample, initial), matrices(sample, long))
    // Phase zero includes two swing legs: stopping must plant them, and must not
    // raise the body into the unrelated idle bind pose at the exact wrap.
    let wrapped = try matrices(sample, long)
    XCTAssertLessThan(try XCTUnwrap(wrapped["Wildlife moonhart body"]).columns.3.y, -0.07)
    var short = initial
    try move(&short, SIMD2(0, -0.125)); try move(&long, SIMD2(0, -0.125))
    try assertLegsEqual(matrices(sample, short), matrices(sample, long))
  }

  func testInvalidCallerScaleOrHeadingFallsBackToFiniteAuthoredPose() throws {
    let sample = try Self.moonhart.get()
    var population = try attached()
    try move(&population, SIMD2(0, -0.125))
    for (yaw, scale): (Float, Float) in [(.nan, 1), (.infinity, 1), (0, .nan), (0, 0), (0, 1.01)] {
      for (name, matrix) in try matrices(sample, population, yaw: yaw, scale: scale) {
        XCTAssertTrue((0..<4).allSatisfy { column in
          (0..<4).allSatisfy { matrix[column][$0].isFinite }
        }, name)
      }
    }
  }

  /// Geometric ray/triangle test of compiled, posed surfaces; no image or lighting threshold.
  private func rayHit(_ origin: V3, _ direction: V3, _ a: V3, _ b: V3, _ c: V3) -> Float? {
    let e1 = b - a, e2 = c - a, p = cross(direction, e2), determinant = dot(e1, p)
    guard abs(determinant) > 0.0000001 else { return nil }
    let t = origin - a, u = dot(t, p) / determinant
    guard u >= 0, u <= 1 else { return nil }
    let q = cross(t, e1), v = dot(direction, q) / determinant
    guard v >= 0, u + v <= 1 else { return nil }
    let distance = dot(e2, q) / determinant
    return distance > 0 ? distance : nil
  }
  func testActualMountedCameraForwardRaysClearPosedAnatomyIncludingSkinnedWings() throws {
    let moon = try Self.moonhart.get(), ray = try Self.ray.get()
    var population = try attached()
    for index in 0..<12 {
      try move(&population, SIMD2(0, -0.0625))
      let time = Float(index) * 0.727
      for (sample, cameraY, isRay) in [(moon, Float(2.5), false), (ray, Float(0.42), true)] {
        let state = sample.animation.initialize(17)
        let poses: [String: simd_float4x4]
        var root = matrix_identity_float4x4
        if isRay {
          poses = PartRig.matrices(sample.joints, poses: SanctuaryMountMotion.poses(
            for: .cloudRay, time: time, phase: nil, gaze: 0, mood: "idle"))
          root = sample.animation.root("idle", time, state, sample.motion)
          // Mounted attachment supplies translation; retain the same authored rotation.
          root.columns.3 = SIMD4(0, 0, 0, 1)
        } else { poses = try matrices(sample, population, time: time) }
        for batch in sample.batches {
          let palette = batch.skinJoints.map { poses[$0] ?? matrix_identity_float4x4 }
          XCTAssertTrue(batch.skinWeights.isEmpty || batch.skinWeights.count == batch.mesh.vertices.count)
          let points: [V3] = batch.mesh.vertices.enumerated().map { index, vertex in
            let matrix = batch.skinWeights.isEmpty
              ? poses[batch.name] ?? matrix_identity_float4x4 : batch.skinWeights[index].matrix(palette)
            let point = root * matrix * vertex.position
            return V3(point.x, point.y, point.z)
          }
          for pitch: Float in [0, -0.08] {
            let origin = V3(0, cameraY, 0), direction = V3(0, sin(pitch), -cos(pitch))
            var nearest: Float?
            for triangle in stride(from: 0, to: batch.mesh.indices.count, by: 3) {
              let a = points[Int(batch.mesh.indices[triangle])]
              let b = points[Int(batch.mesh.indices[triangle + 1])]
              let c = points[Int(batch.mesh.indices[triangle + 2])]
              if let distance = rayHit(origin, direction, a, b, c) {
                nearest = min(nearest ?? distance, distance)
              }
            }
            XCTAssertNil(nearest, "Mounted center view occluded by \(batch.name), pitch \(pitch), t \(time)")
          }
        }
      }
    }
  }
}
