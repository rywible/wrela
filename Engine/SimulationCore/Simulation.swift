import FieldCore
import Foundation
import simd

public struct PlayerCamera: Codable, Equatable {
  public var position: V3
  public var yaw: Float
  public var pitch: Float
  public var forward: V3 { V3(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch)) }
  public init(position: V3, yaw: Float = 0, pitch: Float = 0) {
    self.position = position
    self.yaw = yaw
    self.pitch = pitch
  }
}
public enum SimulationAction: Codable, Equatable {
  case move(V3)
  case look(yaw: Float, pitch: Float)
  case interact
  case flashlight(Bool)
  /// Free-form player input is replayed through the game's production request path.
  case request(String)
  /// A game-owned, bounded semantic control identifier.
  case control(String)
  /// A recorded game-owned external decision. Replay never re-queries its producer.
  case externalDecision(Data)
  case tick(running: Bool)
}
public enum SimulationFailure: LocalizedError {
  case invalid(String)
  public var errorDescription: String? {
    switch self {
    case .invalid(let message): return message
    }
  }
}
/// Runtime and test drivers share this interface. No renderer, window, wall clock or device.
public protocol GameSimulation: AnyObject {
  var camera: PlayerCamera { get set }
  var observations: [String: String] { get }
  func move(_ delta: V3)
  func advance(_ seconds: Float, running: Bool)
  @discardableResult func interact() throws -> String
  func flashlight(_ enabled: Bool) throws
  @discardableResult func request(_ text: String) throws -> String
  @discardableResult func control(_ id: String) throws -> String
  func applyExternalDecision(_ data: Data) throws
  func checkpoint() throws -> Data
  func restore(_ data: Data) throws
  func validate() throws
}
extension GameSimulation {
  public func apply(_ action: SimulationAction) throws {
    switch action {
    case .move(let d):
      guard [d.x, d.y, d.z].allSatisfy({ $0.isFinite && abs($0) <= 20 }) else {
        throw SimulationFailure.invalid("Move outside ±20 metres")
      }
      move(d)
    case .look(let yaw, let pitch):
      guard yaw.isFinite, abs(yaw) < 100000, pitch.isFinite, abs(pitch) <= 1.35 else {
        throw SimulationFailure.invalid("Invalid look angle")
      }
      camera.yaw = yaw
      camera.pitch = pitch
    case .interact: _ = try interact()
    case .flashlight(let enabled): try flashlight(enabled)
    case .request(let text):
      try SimulationSemanticInput.validateRequest(text)
      _ = try request(text)
    case .control(let id):
      try SimulationSemanticInput.validateControl(id)
      _ = try control(id)
    case .externalDecision(let data):
      guard !data.isEmpty, data.count <= 16_384 else {
        throw SimulationFailure.invalid("External decision exceeds transport bounds")
      }
      try applyExternalDecision(data)
    case .tick(let running): advance(1 / 60, running: running)
    }
    try validate()
  }
  public func applyExternalDecision(_ data: Data) throws {
    throw SimulationFailure.invalid("This simulation does not accept external decisions")
  }
  public func flashlight(_ enabled: Bool) throws {
    throw SimulationFailure.invalid("This simulation has no flashlight")
  }
  public func request(_ text: String) throws -> String {
    throw SimulationFailure.invalid("This simulation does not accept text requests")
  }
  public func control(_ id: String) throws -> String {
    throw SimulationFailure.invalid("This simulation does not expose semantic controls")
  }
}
/// Shared transport bounds. Games still own their vocabularies and the meaning of accepted input.
public enum SimulationSemanticInput {
  public static func validateRequest(_ text: String) throws {
    guard text.count <= 1_024 else {
      throw SimulationFailure.invalid("Text requests are limited to 1024 characters")
    }
  }
  public static func validateControl(_ id: String) throws {
    guard !id.isEmpty, id.count <= 128,
      id.unicodeScalars.allSatisfy({ scalar in
        switch scalar.value {
        case 45, 46, 48...57, 65...90, 95, 97...122: true
        default: false
        }
      })
    else {
      throw SimulationFailure.invalid("Invalid semantic control identifier")
    }
  }
}
public enum SimulationCoding {
  public static func encode<T: Encodable>(_ value: T) throws -> Data {
    let e = JSONEncoder()
    e.outputFormatting = [.sortedKeys]
    return try e.encode(value)
  }
  public static func validateCamera(_ c: PlayerCamera) throws {
    guard [c.position.x, c.position.y, c.position.z, c.yaw, c.pitch].allSatisfy(\.isFinite),
      abs(c.position.x) <= 100_000, abs(c.position.y) <= 1000, abs(c.position.z) <= 100_000,
      abs(c.yaw) < 100000, abs(c.pitch) <= 1.35
    else { throw SimulationFailure.invalid("Invalid player camera") }
  }
}
