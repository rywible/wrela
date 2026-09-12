import FieldCore
import Foundation
import simd

/// A game supplies content and editor metadata. The engine never imports a game.
package struct ScalarControl {
  package var key: String
  package var title: String
  package var initial: Float
  package var range: ClosedRange<Float>
  package init(_ key: String, _ title: String, _ initial: Float, _ range: ClosedRange<Float>) {
    self.key = key
    self.title = title
    self.initial = initial
    self.range = range
  }
}
package struct InspectionCamera: Codable {
  package var name: String
  package var eye: V3
  package var target: V3
  package init(_ name: String, _ eye: V3, _ target: V3) {
    self.name = name
    self.eye = eye
    self.target = target
  }
}
package struct AssetGenerator {
  package var id: String
  package var name: String
  package var controls: [ScalarControl]
  package var views: [InspectionCamera]
  package var parts: ([String: Float]) -> [AssetPart]
  package var compile: ((AssetSource) throws -> [SceneBatch])?
  package var animation: AnimationDefinition?
  package init(
    id: String, name: String, controls: [ScalarControl] = [], animation: AnimationDefinition? = nil,
    parts: @escaping ([String: Float]) -> [AssetPart] = { _ in [] }, views: [InspectionCamera] = [],
    compile: ((AssetSource) throws -> [SceneBatch])? = nil
  ) {
    self.views = views
    self.id = id
    self.name = name
    self.controls = controls
    self.parts = parts
    self.compile = compile
    self.animation = animation
  }
}
package struct SimulationInspection {
  package var subject: String
  package var actor: BehaviorSnapshot
  package init(subject: String, actor: BehaviorSnapshot) {
    self.subject = subject
    self.actor = actor
  }
}
package final class GameProject {
  package let id: String
  package let name: String
  package let defaultSubject: String
  package let directory: URL
  package let usesAtmosphere: Bool
  package let generators: [AssetGenerator]
  package var inspectSimulation: ((Data) throws -> SimulationInspection)?
  package let create: (MetalRenderer) throws -> any GameExperience
  package init(
    id: String, name: String, defaultSubject: String, directory: URL,
    generators: [AssetGenerator], usesAtmosphere: Bool = true,
    inspectSimulation: ((Data) throws -> SimulationInspection)? = nil,
    create: @escaping (MetalRenderer) throws -> any GameExperience
  ) {
    self.id = id
    self.name = name
    self.defaultSubject = defaultSubject
    self.directory = directory
    self.usesAtmosphere = usesAtmosphere
    self.generators = generators
    self.inspectSimulation = inspectSimulation
    self.create = create
  }
  package var authoring: URL { directory.appendingPathComponent("Authoring") }
}
package enum ProjectContext {
  package static var dataRoot: URL {
    ProcessInfo.processInfo.environment["WRELA_DATA_ROOT"].map { URL(fileURLWithPath: $0) }
      ?? workspace.appendingPathComponent("." + current.id)
  }
  package static var testing: Bool { ProcessInfo.processInfo.environment["WRELA_TESTING"] == "1" }

  package static var projects: [GameProject] = []
  package static var current: GameProject!
  package static var workspace: URL {
    URL(
      fileURLWithPath: ProcessInfo.processInfo.environment["WRELA_WORKSPACE"] ?? ProcessInfo
        .processInfo.environment["SANCTUARY_WORKSPACE"] ?? Bundle.main.object(
          forInfoDictionaryKey: "SanctuaryWorkspace") as? String
        ?? FileManager.default.currentDirectoryPath)
  }
  package static func configure(_ projects: [GameProject], selected: String? = nil) {
    self.projects = projects
    current = projects.first { $0.id == selected } ?? projects.first!
  }
  package static func select(_ id: String) throws {
    guard let project = projects.first(where: { $0.id == id }) else {
      throw RuntimeError.message("Unknown project: \(id)")
    }
    current = project
  }
  package static func generator(_ id: String) -> AssetGenerator? {
    current?.generators.first { $0.id == id }
  }
}

/// A document preserves parameters, not an expanded program. Recipes remain Swift.
package struct MotionParameters: Codable, Equatable {
  package var values: [String: Float]
  package init(_ values: [String: Float] = [:]) { self.values = values }
  package init(from decoder: Decoder) throws {
    values = try decoder.singleValueContainer().decode([String: Float].self)
  }
  package func encode(to encoder: Encoder) throws {
    var c = encoder.singleValueContainer()
    try c.encode(values)
  }
  package func decoded<T: Decodable>(_ type: T.Type) throws -> T {
    try JSONDecoder().decode(type, from: JSONEncoder().encode(values))
  }
}
package struct PreviewInput: Codable {
  package var player = SIMD2<Float>(0, 3)
  package var visible = true
  package var running = false
  package var food: SIMD2<Float>?
  package var obstacle: SIMD2<Float>?
  package var obstacleRadius: Float = 0.35
  package var values: [String: Float] = [:]
  package init() {}
}
package struct PreviewEvent: Codable {
  package var tick: Int?
  package var state: String
  package var reason: String
  package init(tick: Int?, state: String, reason: String) {
    self.tick = tick
    self.state = state
    self.reason = reason
  }
}
/// Presentation metadata plus opaque, versioned game state. No shared AI policy.
package struct BehaviorSnapshot: Codable {
  package var data: Data = Data()
  package var position = SIMD2<Float>.zero
  package var target = SIMD2<Float>.zero
  package var yaw: Float = 0
  package var state = "resting"
  package var reason = "Ready"
  package var fear: Float = 0
  package var tick: Int = 0
  package var phase: Float?
  package var events: [PreviewEvent] = []
  package var hopping: Bool { phase != nil }
  package init() {}
}
package struct AnimationDefinition {
  package var secondaryRig: (([String:Float])->SecondaryRig)?

  package var collisionBodies: [RigCapsule] = []
  package var contactChains: [ContactChain] = []
  package var contactTargets: ((String,Float,BehaviorSnapshot,MotionParameters)->[String:ContactTarget])?
  package var beats: [PerformanceBeat] = []
  package var motionEnvelope: Bounds? = nil
  package var beatDuration: Float = 1
  package var rigLinks: [(String,String)] = []
  package var inspect: ((String, Float, BehaviorSnapshot, MotionParameters) -> [String: Float])? = nil
  package var signalName: String
  package var controls: [ScalarControl]
  package var clips: [String]
  package var scenarios: [String]
  package var bounds: (String) -> Bounds
  package var duration: (MotionParameters) -> Float
  package var poses: (String, Float, BehaviorSnapshot, MotionParameters) -> [String: JointPose]
  package var root: (String, Float, BehaviorSnapshot, MotionParameters) -> simd_float4x4
  package var blink: (Float, MotionParameters) -> Float
  package var initialize: (UInt32) -> BehaviorSnapshot
  package var step: (BehaviorSnapshot, PreviewInput, MotionParameters) -> BehaviorSnapshot
  package var input: (String, Float) -> PreviewInput
  package var validate: (MotionParameters) throws -> Void
  package init(
    controls: [ScalarControl], clips: [String], scenarios: [String], signalName: String = "Signal",
    duration: @escaping (MotionParameters) -> Float,
    poses: @escaping (String, Float, BehaviorSnapshot, MotionParameters) -> [String: JointPose],
    root: @escaping (String, Float, BehaviorSnapshot, MotionParameters) -> simd_float4x4,
    blink: @escaping (Float, MotionParameters) -> Float = { _, _ in 0 },
    initialize: @escaping (UInt32) -> BehaviorSnapshot,
    step: @escaping (BehaviorSnapshot, PreviewInput, MotionParameters) -> BehaviorSnapshot,
    input: @escaping (String, Float) -> PreviewInput,
    bounds: @escaping (String) -> Bounds = { _ in Bounds(V3(-3, 0, -3), V3(3, 2, 3)) },
    validate: @escaping (MotionParameters) throws -> Void
  ) {
    self.signalName = signalName
    self.bounds = bounds
    self.controls = controls
    self.clips = clips
    self.scenarios = scenarios
    self.duration = duration
    self.poses = poses
    self.root = root
    self.blink = blink
    self.initialize = initialize
    self.step = step
    self.input = input
    self.validate = validate
  }
}

private enum JSONValue: Codable {
  case object([String: JSONValue])
  case array([JSONValue])
  case string(String)
  case number(Double)
  case bool(Bool)
  case null
  init(from decoder: Decoder) throws {
    let c = try decoder.singleValueContainer()
    if c.decodeNil() {
      self = .null
    } else if let v = try? c.decode(Bool.self) {
      self = .bool(v)
    } else if let v = try? c.decode(Double.self) {
      self = .number(v)
    } else if let v = try? c.decode(String.self) {
      self = .string(v)
    } else if let v = try? c.decode([String: Self].self) {
      self = .object(v)
    } else {
      self = .array(try c.decode([Self].self))
    }
  }
  func encode(to encoder: Encoder) throws {
    var c = encoder.singleValueContainer()
    switch self {
    case .object(let v): try c.encode(v)
    case .array(let v): try c.encode(v)
    case .string(let v): try c.encode(v)
    case .number(let v): try c.encode(v)
    case .bool(let v): try c.encode(v)
    case .null: try c.encodeNil()
    }
  }
}
extension BehaviorSnapshot {
  enum CodingKeys: String, CodingKey {
    case data, position, target, yaw, state, reason, fear, tick, phase, events
  }
  package init(from decoder: Decoder) throws {
    self.init()
    let c = try decoder.container(keyedBy: CodingKeys.self)
    data =
      try c.decodeIfPresent(Data.self, forKey: .data)
      ?? JSONEncoder().encode(JSONValue(from: decoder))
    position = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .position) ?? .zero
    target = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .target) ?? .zero
    yaw = try c.decodeIfPresent(Float.self, forKey: .yaw) ?? 0
    state = try c.decodeIfPresent(String.self, forKey: .state) ?? "resting"
    reason = try c.decodeIfPresent(String.self, forKey: .reason) ?? "Ready"
    fear = try c.decodeIfPresent(Float.self, forKey: .fear) ?? 0
    tick = try c.decodeIfPresent(Int.self, forKey: .tick) ?? 0
    phase = try c.decodeIfPresent(Float.self, forKey: .phase)
    events = try c.decodeIfPresent([PreviewEvent].self, forKey: .events) ?? []
  }
}
extension PreviewInput {
  enum CodingKeys: String, CodingKey {
    case player, visible, running, food, obstacle, obstacleRadius, values
  }
  package init(from decoder: Decoder) throws {
    self.init()
    let c = try decoder.container(keyedBy: CodingKeys.self)
    player = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .player) ?? SIMD2(0, 3)
    visible = try c.decodeIfPresent(Bool.self, forKey: .visible) ?? true
    running = try c.decodeIfPresent(Bool.self, forKey: .running) ?? false
    food = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .food)
    obstacle = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .obstacle)
    obstacleRadius = try c.decodeIfPresent(Float.self, forKey: .obstacleRadius) ?? 0.35
    values = try c.decodeIfPresent([String: Float].self, forKey: .values) ?? [:]
  }
}
