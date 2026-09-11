import FieldCore
import Foundation
@_exported import SimulationCore
import simd

package struct GameLighting {
  package var sky = SkySettings.preset("morning")
  package var preset = "morning"
  package var wetness: Float = 0
  package var exposure: Float = 1
  package var flashlight = false
  package var lightEnabled = true
  package var intensity: Float = 24
  package init() {}
}
package struct GameHUD {
  package var title: String
  package var objective: String
  package var detail: String
  package var prompt: String?
  package init(title: String, objective: String, detail: String, prompt: String?) {
    self.title = title
    self.objective = objective
    self.detail = detail
    self.prompt = prompt
  }
}
package protocol GameExperience: AnyObject {
  var simulation: any GameSimulation { get }
  var camera: PlayerCamera { get set }
  var lighting: GameLighting { get set }
  var hud: GameHUD { get }
  var snapshot: [String: Any] { get }
  func move(_ delta: V3)
  func step(_ seconds: Float, running: Bool)
  func items() -> [RenderItem]
  func interact() throws -> String
  func command(_ action: String, _ values: [String: Any]) throws
  func query(_ p: V3) throws -> [String: Any]
  func audioCues() -> [AudioCue]
  func save() throws
}

extension GameExperience {
  package func audioCues() -> [AudioCue] { [] }
  package func query(_ p: V3) throws -> [String: Any] {
    throw RuntimeError.message("This game does not expose field queries")
  }
}
