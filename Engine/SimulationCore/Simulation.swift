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
    case .tick(let running): advance(1 / 60, running: running)
    }
    try validate()
  }
  public func flashlight(_ enabled: Bool) throws {
    throw SimulationFailure.invalid("This simulation has no flashlight")
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
      abs(c.position.x) <= 1000, abs(c.position.y) <= 1000, abs(c.position.z) <= 1000,
      abs(c.yaw) < 100000, abs(c.pitch) <= 1.35
    else { throw SimulationFailure.invalid("Invalid player camera") }
  }
}
