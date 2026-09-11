import AppKit
import FieldCore
import FieldEngine
import MetalKit
import simd

final class GameSession: NSObject, MTKViewDelegate {
  let graphics: MetalRenderer
  let game: any GameExperience
  let view: MTKView
  var audio: AudioScene?
  var paused = false
  var time: Float = 0
  var look = (try? SceneLook.load()) ?? SceneLook()
  var wind = WindSimulation()
  var keys = Set<UInt16>()
  var last = CACurrentMediaTime(), accumulator: Double = 0
  var onUpdate: (() -> Void)?
  init(view: MTKView) throws {
    self.view = view
    graphics = try MetalRenderer(
      view: view,
      materialSourceURL: ProjectContext.current.authoring.appendingPathComponent("Materials.metal"),
      atmosphericRendering: ProjectContext.current.usesAtmosphere)
    game = try ProjectContext.current.create(graphics)
    super.init()
    paused = ProjectContext.testing
    audio = try? AudioScene()
    view.delegate = self
    graphics.metadataProvider = { [weak self] in self?.snapshot() ?? [:] }
  }
  func step(_ dt: Float) {
    time += dt
    if !game.lighting.flashlight { wind.step(dt) }
    var m = V3.zero
    if keys.contains(13) { m.z += 1 }
    if keys.contains(1) { m.z -= 1 }
    if keys.contains(0) { m.x -= 1 }
    if keys.contains(2) { m.x += 1 }
    let moving = length_squared(m) > 0
    if moving { game.move(normalize(m) * dt * (keys.contains(56) ? 10 : 5.5)) }
    game.step(dt, running: moving && keys.contains(56))
    audio?.listener(game.camera)
    for cue in game.audioCues() { audio?.play(cue) }
  }
  func look(dx: Float, dy: Float) {
    game.camera.yaw += dx * 0.003
    game.camera.pitch = clamp(game.camera.pitch - dy * 0.003, -1.35, 1.35)
  }
  func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
  func draw(in view: MTKView) {
    let now = CACurrentMediaTime()
    let elapsed = now - last
    last = now
    if !paused {
      accumulator += min(elapsed, 0.05)
      while accumulator >= 1 / 60 {
        step(1 / 60)
        accumulator -= 1 / 60
      }
    } else {
      accumulator = 0
    }
    let frame = FrameComposer.game(game, time: time, look: look, wind: wind, paused: paused)
    graphics.draw(
      frame, elapsed: elapsed, simulationMilliseconds: (CACurrentMediaTime() - now) * 1000)
    if graphics.frame % 15 == 0 { onUpdate?() }
  }
  func snapshot() -> [String: Any] {
    var s = graphics.diagnostics()
    s.merge(game.snapshot) { _, new in new }
    let c = game.camera
    s.merge([
      "project": ProjectContext.current.id, "mode": "game", "time": time, "paused": paused,
      "camera": [
        "position": [c.position.x, c.position.y, c.position.z], "yaw": c.yaw, "pitch": c.pitch,
      ],
      "sceneLook": look.values, "lighting": game.lighting.preset, "sky": game.lighting.sky.snapshot,
      "parameters": [
        "exposure": game.lighting.exposure, "wind": game.lighting.flashlight ? 0 : 1,
        "scale": game.snapshot["scale"] ?? 1,
      ],
      "wetness": game.lighting.wetness, "worldUnit": "metre",
      "atmosphereEnabled": !game.lighting.flashlight,
      "sessionPID": ProcessInfo.processInfo.processIdentifier,
    ]) { _, new in new }
    return s
  }
}
