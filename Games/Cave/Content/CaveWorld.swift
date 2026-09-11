import FieldCore
import Foundation
import SimulationCore
import simd

public final class CaveWorld: GameSimulation {
  public var state: CaveSimulation
  public var camera = PlayerCamera(position: CaveLayout.entrance)
  public var lightEnabled = true
  public var rock: Shape
  public init(seed: UInt32 = 19, width: Float = 1, ceiling: Float = 1) {
    state = CaveSimulation(seed: seed)
    rock = CaveLayout.rock(width: width, ceiling: ceiling)
  }
  public func move(_ local: V3) {
    let delta =
      V3(cos(camera.yaw), 0, sin(camera.yaw)) * local.x + V3(sin(camera.yaw), 0, -cos(camera.yaw))
      * local.z
    camera.position = FieldQueries.moveCapsule(
      eye: camera.position, delta: delta * 0.65, solid: rock)
  }
  public var illuminated: Bool {
    let target = state.monster + V3(0, 1.8, 0)
    let delta = target - camera.position
    return lightEnabled && length_squared(delta) < 65 * 65 && length_squared(delta) > 0.001
      && dot(normalize(delta), camera.forward) > 0.82
      && FieldQueries.visible(from: camera.position, to: target, solid: rock)
  }
  public func advance(_ seconds: Float, running: Bool) {
    if state.phase == "returning", camera.position.z > -29, !state.returnedScare {
      let candidate = camera.position + camera.forward * 3.4 - V3(0, 1.4, 0)
      if rock.value(at: candidate + V3(0, 1.8, 0)) > 0.35 { state.monster = candidate }
    }

    state.step(
      seconds: seconds, player: camera.position, forward: camera.forward, illuminated: illuminated)
  }
  public func flashlight(_ enabled: Bool) throws { lightEnabled = enabled }
  public func interact() throws -> String {
    if state.phase == "escaped" {
      state = CaveSimulation(seed: state.seed &+ 1)
      camera = PlayerCamera(position: CaveLayout.entrance)
      lightEnabled = true
      return "A new survey begins."
    }
    _ = state.recoverBeacon(player: camera.position)
    return state.reason
  }
  private struct Snapshot: Codable {
    var version = 1
    var state: CaveSimulation
    var camera: PlayerCamera
    var flashlight: Bool
  }
  public func checkpoint() throws -> Data {
    try SimulationCoding.encode(Snapshot(state: state, camera: camera, flashlight: lightEnabled))
  }
  public func restore(_ data: Data) throws {
    let s = try JSONDecoder().decode(Snapshot.self, from: data)
    guard s.version == 1 else { throw SimulationFailure.invalid("Unsupported cave snapshot") }
    try s.state.validate()
    try SimulationCoding.validateCamera(s.camera)
    guard rock.value(at: s.camera.position) > 0.1 else {
      throw SimulationFailure.invalid("Camera is in solid rock")
    }
    state = s.state
    camera = s.camera
    lightEnabled = s.flashlight
  }
  public func validate() throws {
    try state.validate()
    try SimulationCoding.validateCamera(camera)
    guard rock.value(at: camera.position) > 0.1 else {
      throw SimulationFailure.invalid("Player crossed a cave wall")
    }
  }
  public var observations: [String: String] {
    [
      "phase": state.phase, "encounter": state.encounter, "scares": String(state.scares),
      "illuminated": String(illuminated), "flashlight": String(lightEnabled),
      "time": String(state.time),
      "x": String(camera.position.x), "y": String(camera.position.y),
      "z": String(camera.position.z),
    ]
  }
}
