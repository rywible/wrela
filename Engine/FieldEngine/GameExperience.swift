import FieldCore
import Foundation
@_exported import SimulationCore
import simd

package struct GameLighting {
  package var sky = SkySettings.preset("morning")
  package var preset = "morning"
  package var wetness: Float = 0
  package var exposure: Float = 1
  /// An authored outdoor readability floor in diffuse irradiance / pi units.
  /// It is zero for ordinary daylight and is not an atmosphere simulation.
  package var outdoorAmbientFloor: V3 = .zero
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
package struct GameControl: Equatable {
  package enum Presentation { case inline, document }
  package var id: String
  package var title: String
  package var presentation: Presentation
  package init(id: String, title: String, presentation: Presentation = .inline) {
    self.id = id; self.title = title; self.presentation = presentation
  }
}
package protocol GameExperience: AnyObject {
  var simulation: any GameSimulation { get }
  var camera: PlayerCamera { get set }
  var lighting: GameLighting { get set }
  var hud: GameHUD { get }
  var snapshot: [String: Any] { get }
  /// Far clip distance in metres. Projects may opt into a larger authored world.
  var visibilityDistance: Float { get }
  var surfaceInfluences: SurfaceInfluenceSnapshot { get }
  /// World elevation used to center the outdoor shadow region beneath the player.
  var shadowReferenceElevation: Float { get }
  /// A game may expose player text without putting its vocabulary in the host.
  var textInputPrompt: String? { get }
  func submitText(_ text: String) throws -> String
  var controls: [GameControl] { get }
  func activateControl(_ id: String) throws -> String
  func move(_ delta: V3)
  func step(_ seconds: Float, running: Bool)
  func items() -> [RenderItem]
  func interact() throws -> String
  /// Handles a project-owned transient interaction. `false` preserves the host's
  /// normal Escape focus behavior.
  func cancelInteraction() -> Bool
  func command(_ action: String, _ values: [String: Any]) throws
  func query(_ p: V3) throws -> [String: Any]
  func audioCues() -> [AudioCue]
  func save() throws
}

extension GameExperience {
  package var visibilityDistance: Float { 450 }
  package var surfaceInfluences: SurfaceInfluenceSnapshot { .empty }
  package var shadowReferenceElevation: Float { camera.position.y - 1.72 }
  package var controls: [GameControl] { [] }
  package func activateControl(_ id: String) throws -> String {
    try SimulationSemanticInput.validateControl(id)
    return try simulation.control(id)
  }
  package var textInputPrompt: String? { nil }
  package func submitText(_ text: String) throws -> String {
    try SimulationSemanticInput.validateRequest(text)
    return try simulation.request(text)
  }
  package func cancelInteraction() -> Bool { false }
  package func audioCues() -> [AudioCue] { [] }
  package func query(_ p: V3) throws -> [String: Any] {
    throw RuntimeError.message("This game does not expose field queries")
  }
}
