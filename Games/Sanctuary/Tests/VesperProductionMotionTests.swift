import FieldCore
import Foundation
import XCTest
import simd
@testable import SanctuaryContent

final class VesperProductionMotionTests: XCTestCase {
  private var root: URL { URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent() }

  /// Execute the checked-in public authoring recipe, then interpret its source
  /// operations with the same production FieldCore functions as Soundstage.
  private func authoredPhrase() throws -> (MotionPhrase, MotionPhrase, [PartJoint], [String: V3]) {
    let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
    defer { try? FileManager.default.removeItem(at: temporary) }
    let process = Process(); process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["python3", root.appendingPathComponent("Games/Sanctuary/Authoring/author_vesper_performance.py").path, "--write", temporary.path]
    process.standardOutput = FileHandle.nullDevice; try process.run(); process.waitUntilExit()
    XCTAssertEqual(process.terminationStatus, 0)
    let decoder = JSONDecoder()
    let value = try JSONSerialization.jsonObject(with: Data(contentsOf: temporary)) as! [String: Any]
    var phrase: MotionPhrase?, pawPhrase: MotionPhrase?
    for operation in value["operations"] as! [[String: Any]] {
      func decode<T: Decodable>(_ field: String, _: T.Type) throws -> T {
        try decoder.decode(T.self, from: JSONSerialization.data(withJSONObject: operation[field]!))
      }
      switch operation["op"] as! String {
      case "upsert":
        let value = try decode("value", MotionPhrase.self)
        if value.id == "vesper-planted-paw-heading" { pawPhrase = value } else { phrase = value }
      case "footsteps":
        phrase!.contacts = try FootstepPlan.compile(decode("steps", [AuthoredFootstep].self),
          initial: Dictionary(uniqueKeysWithValues: phrase!.contacts.map { ($0.chain, $0.keys[0].position) }),
          duration: phrase!.duration, minimumSupport: 3)
      case "editInterval": phrase = try decode("edit", MotionIntervalEdit.self).applying(to: phrase!)
      default: break
      }
    }
    let document = try JSONSerialization.jsonObject(with: Data(contentsOf: root.appendingPathComponent("Games/Sanctuary/Authoring/Assets/vesper.json"))) as! [String: Any]
    let craft = try decoder.decode(CreatureCraft.self, from: JSONSerialization.data(withJSONObject: document["craft"]!))
    var joints = ProcessionalMotion.joints
    for joint in craft.joints { joints.removeAll { $0.id == joint.id }; joints.append(joint) }
    let offsets = try decoder.decode([String: V3].self, from: JSONSerialization.data(withJSONObject: document["contactOffsets"]!))
    return (phrase!, pawPhrase!, joints, offsets)
  }

  func testFinalContactSolvesAcrossDeepCrouchDiagonalCatchAndSettle() throws {
    let (phrase, pawPhrase, joints, offsets) = try authoredPhrase()
    let chains = [true, false].flatMap { front in [Float(-1), Float(1)].map { side in
      let id = ProcessionalMotion.limbID(front, side)
      return ContactChain(id: id, parent: front ? "neck" : "haunch", upper: id + "-upper",
        lower: id + "-lower", foot: id + "-paw", pole: V3(side * 0.4, 0, 2), sole: 0.18)
    }}
    try phrase.validate(joints: Set(joints.map(\.id)), chains: Set(chains.map(\.id)))
    var worstReach: Float = 0, worstPlantedSpeed: Float = 0, minimumKnee: Float = 100
    var previous: [String: ContactObservation] = [:]
    var previousHeading: [String: V3] = [:]
    var worstPlantedTurn: Float = 0
    for tick in 0...720 {
      let time = Float(tick) / 60
      var pose = phrase.pose(at: time)
      pose.merge(pawPhrase.pose(at: time)) { _, heading in heading }
      let targets = Dictionary(uniqueKeysWithValues: phrase.contacts.map { ($0.chain, $0.sample(time)) })
      let result = ContactRig.solve(joints: joints, matrices: PartRig.matrices(joints, poses: pose),
        chains: chains, targets: targets, offsets: offsets)
      XCTAssertGreaterThanOrEqual(result.contacts.filter(\.planted).count, 3)
      for contact in result.contacts {
        worstReach = max(worstReach, contact.residual)
        minimumKnee = min(minimumKnee, contact.knee.y)
        if contact.planted, let before = previous[contact.id], before.planted {
          worstPlantedSpeed = max(worstPlantedSpeed, length(contact.actual - before.actual) * 60)
        }
        let m=result.matrices[contact.id+"-paw"]!
        let heading=V3(m.columns.2.x,m.columns.2.y,m.columns.2.z)
        if contact.planted, let before=previous[contact.id], before.planted,
          let oldHeading=previousHeading[contact.id] {
          worstPlantedTurn=max(worstPlantedTurn,length(heading-oldHeading))
        }
        previousHeading[contact.id]=heading
        previous[contact.id] = contact
      }
    }
    print("Vesper production phrase: maximum reach residual \(worstReach) m, planted speed \(worstPlantedSpeed) m/s, minimum knee height \(minimumKnee) m")
    XCTAssertLessThan(worstPlantedTurn, 0.00001, "Paw heading moved while planted")
    XCTAssertLessThan(worstReach, 0.002)
    XCTAssertLessThan(worstPlantedSpeed, 0.002)
    XCTAssertGreaterThan(minimumKnee, 0.22, "Knee joints need floor clearance before surface inspection")
    XCTAssertLessThan(phrase.pose(at: 3.4)["body"]!.offset.y, -0.60)
    XCTAssertLessThan(phrase.contacts.first { $0.chain == "fore-left" }!.sample(6.45).position.x, -1.4)
    XCTAssertGreaterThan(phrase.pose(at: 12)["body"]!.rotation.y, 20)
  }
}
