// CPU-only verification of the authored study through production FieldCore.
// It deliberately does not contain substitute motion, IK or support algorithms.
import FieldCore
import Foundation
import simd

let input = URL(fileURLWithPath: CommandLine.arguments[1])
let envelope = try JSONSerialization.jsonObject(with: Data(contentsOf: input)) as! [String: Any]
let operations = envelope["operations"] as! [[String: Any]]
var document: [String: Any] = ["version": 1]
let decoder = JSONDecoder()
func craft() throws -> CreatureCraft {
  try decoder.decode(CreatureCraft.self, from: JSONSerialization.data(withJSONObject: document))
}
func json<T: Encodable>(_ value: T) throws -> Any {
  try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
}
for operation in operations {
  let op = operation["op"] as! String
  switch op {
  case "upsert":
    let collection = operation["collection"] as! String
    let value = operation["value"] as! [String: Any]
    let key = collection == "masses" ? "joint" : "id"
    var values = document[collection] as? [[String: Any]] ?? []
    if let index = values.firstIndex(where: { $0[key] as? String == value[key] as? String }) {
      values[index] = value
    } else { values.append(value) }
    document[collection] = values
  case "set":
    document[operation["collection"] as! String] = operation["value"]!
  case "footsteps":
    var source = try craft()
    let index = source.phrases.firstIndex { $0.id == operation["key"] as! String }!
    let steps = try decoder.decode([AuthoredFootstep].self,
      from: JSONSerialization.data(withJSONObject: operation["steps"]!))
    let initial = Dictionary(uniqueKeysWithValues: source.phrases[index].contacts.map {
      ($0.chain, $0.keys.first!.position)
    })
    source.phrases[index].contacts = try FootstepPlan.compile(steps, initial: initial,
      duration: source.phrases[index].duration, minimumSupport: operation["minimumSupport"] as! Int)
    document["phrases"] = try json(source.phrases)
  case "anatomyAnchor":
    var source = try craft()
    let anatomy = source.anatomy.first { $0.id == operation["source"] as! String }!
    let point = try decoder.decode(V3.self,
      from: JSONSerialization.data(withJSONObject: operation["point"]!))
    source.anatomyAnchors.append(try anatomy.anchor(id: operation["key"] as! String, at: point))
    document["anatomyAnchors"] = try json(source.anatomyAnchors)
  default:
    fatalError("This source verifier does not handle operation \(op); use the native transaction")
  }
}
let source = try craft()
let rig = source.joints
let arrangement = source.arrangement!
try source.validate(parts: Set(source.anatomy.map(\.part)), rig: rig,
  guides: [], chains: Set(source.contactChains.map(\.id)), clips: [arrangement.clip])
var maximumResidual: Float = 0
var minimumSupport = Int.max
var maximumOutside: Float = 0
var minimumClearance: Float = .greatestFiniteMagnitude
var worstTime: Float = 0
var plantedMovement: Float = 0
var previous: [String: ContactObservation] = [:]
let ticks = Int((arrangement.duration * 60).rounded())
for tick in 0...ticks {
  let time = Float(tick) / 60
  let authored = arrangement.evaluate(time: time, phrases: source.phrases, poses: [:], contacts: [:])
  let matrices = PartRig.matrices(rig, poses: authored.poses)
  let result = ContactRig.solve(joints: rig, matrices: matrices,
    chains: source.contactChains, targets: authored.contacts)
  precondition(result.contacts.count == 6, "A source-defined contact chain was not evaluated")
  for contact in result.contacts {
    if contact.residual > maximumResidual { maximumResidual = contact.residual; worstTime = time }
    minimumClearance = min(minimumClearance, contact.clearance)
    if contact.planted, let old = previous[contact.id], old.planted {
      plantedMovement = max(plantedMovement, length(contact.actual-old.actual))
    }
    previous[contact.id] = contact
  }
  let planted = result.contacts.filter(\.planted)
  minimumSupport = min(minimumSupport, planted.count)
  let support = BodySupport.inspect(masses: source.masses, matrices: result.matrices,
                                    anchors: planted.map(\.actual))
  maximumOutside = max(maximumOutside, support.outsideDistance)
}
let passed = maximumResidual < 0.004 && minimumSupport >= 3 && plantedMovement < 0.0001
  && minimumClearance > -0.004
let report: [String: Any] = ["passed": passed, "scope": "Production FieldCore source validation, FootstepPlan, PartRig, ContactRig and BodySupport at every 60 Hz tick. No mesh compilation, renderer, garment/groom collision, physical forces or visual review.",
  "samples": ticks+1, "durationSeconds": arrangement.duration, "anatomySources": source.anatomy.count,
  "joints": rig.count, "contactChains": source.contactChains.count,
  "maximumContactResidualMetres": maximumResidual, "worstResidualTime": worstTime,
  "minimumPlantedContacts": minimumSupport, "maximumPlantedMovementMetres": plantedMovement,
  "minimumContactClearanceMetres": minimumClearance,
  "maximumOutsideStaticSupportMetres": maximumOutside,
  "nativeMeshCompilationRun": false, "visualReviewRun": false]
let output = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: output, as: UTF8.self))
if !passed { exit(1) }
