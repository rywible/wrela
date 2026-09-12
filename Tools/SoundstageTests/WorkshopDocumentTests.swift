import XCTest
import FieldCore
import FieldEngine
@testable import SoundstageKit

/// Persistence and authored-pose checks only: never construct a Metal device,
/// renderer, application or native window in this target.
final class WorkshopDocumentTests: XCTestCase {
  private var directory: URL!
  private var previousProjects: [GameProject] = []
  private var previousProject: GameProject?

  override func setUpWithError() throws {
    directory = FileManager.default.temporaryDirectory.appendingPathComponent("wrela-study-" + UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    previousProjects = ProjectContext.projects
    previousProject = ProjectContext.current
    let project = GameProject(id: "study-tests", name: "Study tests", defaultSubject: "fixture",
      directory: directory, generators: [], create: { _ in
        throw RuntimeError.message("CPU study tests must not launch a game")
      })
    ProjectContext.configure([project])
  }
  override func tearDownWithError() throws {
    ProjectContext.projects = previousProjects
    ProjectContext.current = previousProject
    try FileManager.default.removeItem(at: directory)
  }
  private func document(rich: Bool = false) -> WorkshopDocument {
    let names = ["study-body"] + (rich ? (0..<63).map { String(format: "study-articulation-node-%02d", $0) } : [])
    var source = AssetSource(id: "study-fixture", name: "Study fixture", generator: "fields")
    source.parts = [AssetPart(name: "study-body", field: FieldExpression(op: "sphere", values: [0.45]), color: [0.4, 0.5, 0.6])]
    source.motion = MotionParameters()
    source.craft.joints = names.dropFirst().map { PartJoint(id: $0, parent: "study-body") }
    let count = rich ? 193 : 2
    let samples = (0..<count).map { i -> PoseSample in
      let time = Float(i) * 12 / Float(count - 1)
      let offset = V3(0, (time / 60 * 1_000_000).rounded() / 1_000_000, 0)
      return PoseSample(time: time, poses: Dictionary(uniqueKeysWithValues: names.map { ($0, JointPose(offset: offset)) }))
    }
    source.craft.phrases = [MotionPhrase(id: "rich-replay", duration: 12, samples: samples)]
    var creature = CreatureWorkshop()
    creature.generator = "fields"
    creature.mode = "rich-replay"
    creature.seconds = 3
    var review = SurfaceReview()
    review.mode = "clay"
    return WorkshopDocument(project: "study-tests", creature: creature, source: source,
      skyOnly: false, rig: StudioRig(), layout: StudioLayout(), lighting: "noon",
      sky: ["altitude": 35, "azimuth": 0, "coverage": 0.3, "density": 1, "haze": 1, "cloudSeed": 17],
      orbit: 0, elevation: 0.2, distance: 6.8, exposure: 1, wind: 1, wetness: 0, time: 3,
      paused: true, windState: WindSimulation(), surfaceReview: review)
  }
  func testRichSourceAndAuthoredPosesRoundTripThroughCompactAndLegacyFiles() throws {
    let original = document(rich: true)
    try original.source.validate()
    let sourceBytes = try JSONEncoder().encode(original.source)
    XCTAssertGreaterThan(sourceBytes.count, 1_048_576)
    XCTAssertLessThan(sourceBytes.count, AssetSource.maximumSourceBytes)
    XCTAssertEqual(original.source.craft.phrases[0].samples.reduce(0) { $0 + $1.poses.count }, 12_352)
    let compact = try original.encoded()
    let url = directory.appendingPathComponent("rich.json")
    try original.write(to: url)
    let restored = try WorkshopDocument.read(url)
    try restored.source.validate()
    XCTAssertEqual(try restored.encoded(), compact)
    let before = original.source.evaluatedPose(clip: "rich-replay", time: 3, state: BehaviorSnapshot(), contacts: false).matrices
    let future = original.source.evaluatedPose(clip: "rich-replay", time: 4.25, state: BehaviorSnapshot(), contacts: false).matrices
    XCTAssertNotEqual(before["study-body"], future["study-body"])
    let replay = restored.source.evaluatedPose(clip: "rich-replay", time: 4.25, state: BehaviorSnapshot(), contacts: false).matrices
    XCTAssertEqual(replay, future)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    let legacy = try encoder.encode(original)
    XCTAssertGreaterThan(legacy.count, compact.count)
    XCTAssertLessThan(legacy.count, WorkshopDocument.maximumBytes)
    try legacy.write(to: url)
    XCTAssertEqual(try WorkshopDocument.read(url).encoded(), compact)
  }
  func testOversizedReadAndSaveRejectWithoutReplacingThePreviousStudy() throws {
    let original = document()
    let url = directory.appendingPathComponent("saved.json")
    try original.write(to: url)
    let previous = try Data(contentsOf: url)
    var oversized = original
    oversized.creature!.actor.data = Data(repeating: 0, count: WorkshopDocument.maximumBytes)
    XCTAssertThrowsError(try oversized.write(to: url)) { error in
      XCTAssertTrue(error.localizedDescription.contains("8 MiB"))
    }
    XCTAssertEqual(try Data(contentsOf: url), previous)
    XCTAssertEqual(try WorkshopDocument.read(url).encoded(), previous)
    let input = directory.appendingPathComponent("oversized.json")
    try Data("{}".utf8).write(to: input)
    let file = try FileHandle(forWritingTo: input)
    try file.truncate(atOffset: UInt64(WorkshopDocument.maximumBytes))
    try file.close()
    XCTAssertThrowsError(try WorkshopDocument.read(input)) { error in
      XCTAssertTrue(error.localizedDescription.contains("8 MiB"))
    }
    XCTAssertEqual(try Data(contentsOf: url), previous)
  }
  func testOlderStudiesOmitReviewAndNamedReviewValidationRemainsStrict() throws {
    let original = document()
    var object = try XCTUnwrap(JSONSerialization.jsonObject(with: original.encoded()) as? [String: Any])
    object.removeValue(forKey: "surfaceReview")
    object.removeValue(forKey: "project")
    let url = directory.appendingPathComponent("legacy.json")
    try JSONSerialization.data(withJSONObject: object).write(to: url)
    let restored = try WorkshopDocument.read(url)
    XCTAssertNil(restored.surfaceReview)
    XCTAssertNil(restored.project)
    XCTAssertEqual(restored.source.id, original.source.id)
    var review = SurfaceReview()
    review.mode = "clay"
    review.hiddenParts = ["study-body"]
    XCTAssertNoThrow(try review.validate(parts: ["study-body"]))
    review.hiddenParts.append("missing")
    XCTAssertThrowsError(try review.validate(parts: ["study-body"]))
    review.hiddenParts = ["study-body", "study-body"]
    XCTAssertThrowsError(try review.validate(parts: ["study-body"]))
  }
}
