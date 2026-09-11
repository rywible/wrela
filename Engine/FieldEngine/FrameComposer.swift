import FieldCore
import simd

/// Converts scene-level camera and light choices into the renderer's GPU ABI.
package enum FrameComposer {
  package static func game(
    _ game: any GameExperience, time: Float, look: SceneLook, wind: WindSimulation, paused: Bool
  ) -> RenderFrame {
    let camera = game.camera
    let settings = game.lighting
    let local = settings.flashlight
    let eye = camera.position
    let target = eye + camera.forward
    let sun = local ? -camera.forward : settings.sky.sunDirection
    let vp = perspective(1.05, 16 / 9, 0.08, 450) * lookAt(eye, target)
    let inverse = (perspective(1.05, 16 / 9, 0.1, 450) * lookAt(.zero, camera.forward)).inverse
    let center = local ? eye + camera.forward * 12 : V3(eye.x, 0, eye.z - 20)
    let lamp = local ? eye + V3(0, -0.08, 0) : center + sun * 110
    var lvp =
      (local ? perspective(1.32, 1, 0.08, 65) : orthographic(65, 1, 250))
      * lookAt(
        lamp, local ? lamp + camera.forward : center,
        up: abs(sun.y) > 0.98 ? V3(0, 0, 1) : V3(0, 1, 0))
    if !local {
      let o = lvp * SIMD4<Float>(0, 0, 0, 1)
      lvp[3].x += (round(o.x * 1024) - o.x * 1024) / 1024
      lvp[3].y += (round(o.y * 1024) - o.y * 1024) / 1024
    }
    let sky = settings.sky
    var u = Uniforms(
      viewProjection: vp, lightVP: lvp, inverseVP: inverse, cameraTime: SIMD4(eye, time),
      sunExposure: SIMD4(sun, settings.exposure),
      options: SIMD4(settings.preset == "sunset" ? 1 : 0, local ? 0 : 1, 0, 0),
      sky: SIMD4(sky.coverage, sky.density, sky.haze, sky.cloudSeed),
      environment: SIMD4(
        local
          ? 4
          : ([
            "morning": Float(0), "noon": 0, "sunset": 1, "overcast": 2, "rain": 3, "golden": 5,
            "afterglow": 6,
          ][settings.preset] ?? 0), settings.wetness, 0, 0))
    if local {
      u.rigPosition = SIMD4(lamp, 2)
      u.rigColor = SIMD4(1, 0.94, 0.82, settings.lightEnabled ? settings.intensity : 0)
      u.rigParams = SIMD4(0.015, 0, 1, 0.80)
    }
    u.lookSurface = look.surface
    u.lookLight = look.light
    u.lookGrade = look.grade
    return RenderFrame(
      id: ProjectContext.current.id, uniforms: u, camera: eye, lightSize: local ? 35 : 65,
      paused: paused, wind: local ? 0 : 1, windState: wind, look: look, items: game.items(),
      infiniteGround: false)
  }
}
