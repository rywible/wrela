import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import SimulationCore
import XCTest
import simd
@testable import SanctuaryProject

/// Actual compiled anatomy and production pose functions, evaluated at the production
/// attachment. These are geometric view checks, not a substitute for the native view.
final class MountViewClearanceTests: XCTestCase {
  private struct Specimen {
    let batches: [SceneBatch]
    let designs: [String: LivingWorldPresentation.PartDesign]
    let joints: [PartJoint]
  }
  private struct Surface {
    let name: String
    let points: [V3]
    let indices: [UInt32]
    let lower: V3
    let upper: V3
  }
  private static let species: [WildlifeSpecies] = [
    .moonhart, .cloudstepper, .saltback, .canopyGlider, .cloudRay,
  ]
  private static let specimens: Result<[WildlifeSpecies: Specimen], Error> = Result {
    var result: [WildlifeSpecies: Specimen] = [:]
    for species in MountViewClearanceTests.species {
      let generator = try XCTUnwrap(LivingWorldPresentation.generators.first {
        $0.id == species.rawValue
      })
      let compile = try XCTUnwrap(generator.compile)
      let parameters = Dictionary(uniqueKeysWithValues: generator.controls.map { ($0.key, $0.initial) })
      let source = AssetSource(id: species.rawValue, name: species.rawValue,
        generator: species.rawValue, parameters: parameters)
      let designs = LivingWorldPresentation.design(for: species, parameters: parameters)
      let joints = generator.parts(parameters).compactMap(\.joint)
      try PartRig.validate(joints)
      result[species] = Specimen(batches: try compile(source),
        designs: Dictionary(uniqueKeysWithValues: designs.map { ($0.name, $0) }), joints: joints)
    }
    return result
  }

  /// Save decoding sets fixture age only. Brain state and motion are produced by the
  /// ordinary request/step functions; no pose angles or alternate AI live in this test.
  private func actor(_ species: WildlifeSpecies, growth: Int = 100, sample: Int = 0) throws -> WildlifeActor {
    let original = try XCTUnwrap(WildlifePopulation.initial(seed: 17).actors.first {
      $0.species == species
    })
    var brain = CreatureSimulation(home: original.nativeHome, seed: 17)
    var stimulus = CreatureStimulus()
    stimulus.player = original.nativeHome + SIMD2(3, -5)
    stimulus.food = nil
    let requests: [AnimalRequest] = [.follow, .greeting, .play, .wait, .follow, .greeting]
    brain.address(requests[sample % requests.count], accepted: sample != 3, player: stimulus.player)
    let ticks = [0, 15, 45, 90, 240, 480][sample % 6]
    for _ in 0..<ticks { brain.step(stimulus) }
    let encoder = JSONEncoder()
    var fields = try XCTUnwrap(JSONSerialization.jsonObject(with: encoder.encode(original)) as? [String: Any])
    fields["maturityTicks"] = 100
    fields["growthTicks"] = growth
    fields["simulation"] = try JSONSerialization.jsonObject(with: encoder.encode(brain))
    return try JSONDecoder().decode(WildlifeActor.self, from: JSONSerialization.data(withJSONObject: fields))
  }

  private func journey(_ actor: WildlifeActor, mode: SanctuaryJourney.TravelMode) throws -> SanctuaryJourney {
    var result = SanctuaryJourney()
    try result.travel(mode, with: actor.id)
    return result
  }
  private func camera(_ actor: WildlifeActor, mode: SanctuaryJourney.TravelMode, yaw: Float) -> PlayerCamera {
    PlayerCamera(position: V3(actor.position.x, mode == .riding ? 39.5 : 125.5, actor.position.y), yaw: yaw)
  }
  private func xyz(_ p: SIMD4<Float>) -> V3 { V3(p.x, p.y, p.z) }

  private func surfaces(_ sample: Specimen, actor: WildlifeActor, player: PlayerCamera,
    journey: SanctuaryJourney, legacyOffset: Float? = nil) throws -> [Surface]
  {
    let placement = LivingWorldPresentation.placement(for: actor, player: player, journey: journey) { _ in
      SanctuaryCreatureElevation(supportY: 37, rootY: 37, focusY: 38)
    }
    XCTAssertTrue(placement.attached)
    let position = legacyOffset.map { player.position - V3(0, $0, 0) } ?? placement.position
    let root = transform(position, V3(repeating: actor.morphologyScale), placement.yaw)
    let authored = LivingWorldPresentation.hasAuthoredMotion(actor.species)
    let body = authored
      ? LivingWorldPresentation.authoredRootPose(for: actor.species, time: actor.time)
      : LivingWorldPresentation.semanticBodyPose(actor, time: actor.time)
    let jointPoses: [String: JointPose]
    if actor.species == .moonhart, journey.mode == .riding {
      jointPoses = SanctuaryMountMotion.mountedPoses(signal: actor.mountedTravel,
        actorYaw: placement.yaw, morphologyScale: actor.morphologyScale, time: actor.time,
        gaze: actor.gaze, mood: actor.posture)
    } else {
      jointPoses = LivingWorldPresentation.sharedPoses(for: actor.species,
        simulation: actor.simulation, identifier: actor.id,
        player: SIMD2(player.position.x, player.position.z), working: false,
        time: actor.time, phase: nil)
    }
    let matrices = PartRig.matrices(sample.joints, poses: jointPoses)
    let head = LivingWorldPresentation.semanticHeadPose(actor, player: player)
    let blink = CreatureMotion().blink(at: actor.time + LivingWorldPresentation.identifierPhase(actor.id))
    let expressive = LivingWorldPresentation.semanticExpressivePose(actor, time: actor.time)
    return try sample.batches.map { batch in
      let design = try XCTUnwrap(sample.designs[batch.name])
      let bindPoints = batch.mesh.vertices.map { xyz($0.position) }
      let lower = bindPoints.reduce(V3(repeating: .greatestFiniteMagnitude), simd_min)
      let upper = bindPoints.reduce(V3(repeating: -.greatestFiniteMagnitude), simd_max)
      let rigid = authored ? matrices[batch.name] ?? matrix_identity_float4x4
        : LivingWorldPresentation.legacyPartPose(role: design.role, pivot: design.pivot,
          center: (lower + upper) / 2, head: head, blink: blink, expressive: expressive)
      let palette = batch.skinJoints.map { matrices[$0] ?? matrix_identity_float4x4 }
      XCTAssertTrue(batch.skinWeights.isEmpty || batch.skinWeights.count == batch.mesh.vertices.count)
      let instance = try XCTUnwrap(batch.instances.first)
      let points = batch.mesh.vertices.enumerated().map { index, vertex -> V3 in
        if batch.skinWeights.isEmpty {
          return xyz(root * body * rigid * instance.model * vertex.position)
        }
        return xyz(root * body * instance.model * batch.skinWeights[index].matrix(palette) * vertex.position)
      }
      return Surface(name: batch.name, points: points, indices: batch.mesh.indices,
        lower: points.reduce(V3(repeating: .greatestFiniteMagnitude), simd_min),
        upper: points.reduce(V3(repeating: -.greatestFiniteMagnitude), simd_max))
    }
  }

  private func intersectsBox(_ origin: V3, _ direction: V3, _ surface: Surface) -> Bool {
    var near: Float = 0, far: Float = .greatestFiniteMagnitude
    for axis in 0..<3 {
      if abs(direction[axis]) < 0.000001 {
        if origin[axis] < surface.lower[axis] || origin[axis] > surface.upper[axis] { return false }
      } else {
        let a = (surface.lower[axis] - origin[axis]) / direction[axis]
        let b = (surface.upper[axis] - origin[axis]) / direction[axis]
        near = max(near, min(a, b)); far = min(far, max(a, b))
        if near > far { return false }
      }
    }
    return true
  }
  private func hit(_ origin: V3, _ direction: V3, _ surface: Surface) -> Float? {
    guard intersectsBox(origin, direction, surface) else { return nil }
    var nearest: Float?
    for i in stride(from: 0, to: surface.indices.count, by: 3) {
      let a = surface.points[Int(surface.indices[i])]
      let b = surface.points[Int(surface.indices[i + 1])]
      let c = surface.points[Int(surface.indices[i + 2])]
      let e1 = b - a, e2 = c - a, p = cross(direction, e2), determinant = dot(e1, p)
      guard abs(determinant) > 0.0000001 else { continue }
      let t = origin - a, u = dot(t, p) / determinant
      guard u >= 0, u <= 1 else { continue }
      let q = cross(t, e1), v = dot(direction, q) / determinant
      guard v >= 0, u + v <= 1 else { continue }
      let distance = dot(e2, q) / determinant
      if distance > 0 { nearest = min(nearest ?? distance, distance) }
    }
    return nearest
  }

  func testCoverageIncludesEveryProductionRideAndFlySpecies() {
    let eligible = Set(WildlifeSpecies.allCases.filter {
      $0.profile.capabilities.contains(.ride) || $0.profile.capabilities.contains(.fly)
    })
    XCTAssertEqual(Set(Self.species), eligible)
  }

  func testActualAttachmentKeepsRidingSupportAndWalkingPlacementForEveryMorphology() throws {
    for species in Self.species {
      for growth in [0, 50, 100] {
        let actor = try actor(species, growth: growth)
        let player = camera(actor, mode: .riding, yaw: 1.1)
        if actor.capabilities.contains(.ride) {
          let attached = LivingWorldPresentation.placement(for: actor, player: player,
            journey: try journey(actor, mode: .riding)) { _ in
              SanctuaryCreatureElevation(supportY: 37, rootY: 37, focusY: 38)
            }
          XCTAssertEqual(attached.position, V3(player.position.x, 37, player.position.z))
          XCTAssertEqual(attached.yaw, -player.yaw)
        }
        let walking = LivingWorldPresentation.placement(for: actor, player: player,
          journey: SanctuaryJourney()) { _ in
            SanctuaryCreatureElevation(supportY: 41, rootY: 41.3, focusY: 42)
          }
        XCTAssertFalse(walking.attached)
        XCTAssertEqual(walking.position, V3(actor.position.x, 41.3, actor.position.y))
        XCTAssertEqual(walking.yaw, actor.yaw)
      }
    }
  }

  func testPosedMeshesClearActualMountedCentralViewConeAcrossMorphologiesAndHeadings() throws {
    let specimens = try Self.specimens.get()
    // Three degrees around each look direction protects a small central region,
    // rather than certifying only an infinitesimal center ray. Looking straight
    // down at the animal remains possible and is intentionally outside this gate.
    let cone: [(Float, Float)] = [(0, 0), (-0.05236, -0.05236), (-0.05236, 0.05236),
      (0.05236, -0.05236), (0.05236, 0.05236)]
    for species in Self.species {
      let specimen = try XCTUnwrap(specimens[species])
      for mode: SanctuaryJourney.TravelMode in [.riding, .flying] {
        guard species.profile.capabilities.contains(mode == .riding ? .ride : .fly) else { continue }
        for growth in [0, 50, 100] {
          for sample in 0..<6 {
            let actor = try actor(species, growth: growth, sample: sample)
            for yaw: Float in [0, 1.1] {
              let player = camera(actor, mode: mode, yaw: yaw)
              let surfaces = try surfaces(specimen, actor: actor, player: player,
                journey: journey(actor, mode: mode))
              for pitch: Float in [0, -0.08, -0.30] {
                for (dyaw, dpitch) in cone {
                  let direction = PlayerCamera(position: player.position, yaw: yaw + dyaw,
                    pitch: pitch + dpitch).forward
                  for surface in surfaces {
                    XCTAssertNil(hit(player.position, direction, surface),
                      "\(surface.name) blocks \(mode), growth \(growth), sample \(sample), yaw \(yaw), pitch \(pitch + dpitch)")
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  func testOldGliderEyeOffsetFailsActualAnatomyNegativeControl() throws {
    let actor = try actor(.canopyGlider), sample = try XCTUnwrap(Self.specimens.get()[.canopyGlider])
    let player = camera(actor, mode: .flying, yaw: 0)
    let old = try surfaces(sample, actor: actor, player: player,
      journey: journey(actor, mode: .flying), legacyOffset: 0.46)
    XCTAssertTrue(old.contains { hit(player.position, player.forward, $0) != nil },
      "The fixture must expose the reported glider head/body obstruction at the old offset")
  }

  func testFlyingBodyAndWingCuesRemainInLowerForwardView() throws {
    let specimens = try Self.specimens.get()
    for species: WildlifeSpecies in [.canopyGlider, .cloudRay] {
      for growth in [0, 100] {
        let actor = try actor(species, growth: growth)
        let player = camera(actor, mode: .flying, yaw: 0.7)
        let surfaces = try surfaces(try XCTUnwrap(specimens[species]), actor: actor, player: player,
          journey: journey(actor, mode: .flying))
        // A normal downward glance must retain anatomy in its lower field. This
        // is a projection check only; lighting and recognizability need native review.
        let forward = V3(sin(player.yaw), 0, -cos(player.yaw))
        let right = V3(cos(player.yaw), 0, sin(player.yaw))
        let cuePoints = surfaces.filter {
          species == .canopyGlider ? $0.name.contains("mantle") : $0.name == "Wildlife cloudRay head"
        }.flatMap(\.points).filter { point in
          let delta = point - player.position, depth = dot(delta, forward)
          guard depth > 0.08 else { return false }
          let elevation = atan2(delta.y, depth), azimuth = atan2(dot(delta, right), depth)
          return elevation < -0.35 && elevation > -0.82 && abs(azimuth) < 0.70
        }
        XCTAssertGreaterThan(cuePoints.count, 8, "Lost lower-view wing/body cues for \(species), growth \(growth)")
      }
    }
  }
}
