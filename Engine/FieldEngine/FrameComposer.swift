import FieldCore
import simd

/// Converts scene-level camera and light choices into the renderer's GPU ABI.
package enum FrameComposer {
  /// Keeps malformed project settings from creating a singular projection while
  /// leaving each game responsible for its authored visibility distance.
  package static func validatedVisibilityDistance(_ requested: Float) -> Float {
    guard requested.isFinite else { return 450 }
    return clamp(requested, 1, 50_000)
  }
  package static func cameraProjection(_ requested: Float) -> simd_float4x4 {
    perspective(1.05, 16 / 9, 0.08, validatedVisibilityDistance(requested))
  }
  package static func outdoorShadowMatrix(_ shadow: OutdoorShadow, sun: V3) -> simd_float4x4 {
    var matrix = orthographic(shadow.size, shadow.near, shadow.far)
      * lookAt(shadow.center + sun * shadow.distance, shadow.center,
        up: abs(sun.y) > 0.98 ? V3(0, 0, 1) : V3(0, 1, 0))
    // Match the existing 2048-pixel directional-shadow texel anchoring.
    let origin = matrix * SIMD4<Float>(0, 0, 0, 1)
    matrix[3].x += (round(origin.x * 1024) - origin.x * 1024) / 1024
    matrix[3].y += (round(origin.y * 1024) - origin.y * 1024) / 1024
    return matrix
  }

  package static func game(
    _ game: any GameExperience, time: Float, look: SceneLook, wind: WindSimulation, paused: Bool
  ) -> RenderFrame {
    let camera = game.camera
    let settings = game.lighting
    let local = settings.flashlight
    let eye = camera.position
    let target = eye + camera.forward
    let sun = local ? -camera.forward : settings.sky.sunDirection
    let projection = cameraProjection(game.visibilityDistance)
    let vp = projection * lookAt(eye, target)
    // Sky and infinite-ground shaders need a translation-free ray transform.
    // Use this frame's exact projection so its far and near mapping matches vp.
    let inverse = (projection * lookAt(.zero, camera.forward)).inverse
    let shadowElevation = game.shadowReferenceElevation.isFinite
      ? game.shadowReferenceElevation : eye.y - 1.72
    let center = local ? eye + camera.forward * 12 : V3(eye.x, shadowElevation, eye.z - 20)
    let lamp = local ? eye + V3(0, -0.08, 0) : center + sun * 110
    let outdoorShadow = local ? nil : OutdoorShadow(center: center, size: 65,
      near: 1, far: 250, distance: 110)
    let lvp = outdoorShadow.map { outdoorShadowMatrix($0, sun: sun) }
      ?? (perspective(1.32, 1, 0.08, 65)
        * lookAt(lamp, lamp + camera.forward,
          up: abs(sun.y) > 0.98 ? V3(0, 0, 1) : V3(0, 1, 0)))
    let sky = settings.sky
    func boundedOutdoorFloor(_ value: Float) -> Float {
      guard value.isFinite else { return 0 }
      return clamp(value, 0, 0.25)
    }
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
    } else {
      // `rigColor` is otherwise unused by the outdoor path. Reusing its RGB
      // lanes keeps the GPU ABI stable while giving games a bounded authored
      // readability floor for moonless or stylized night presentation.
      u.rigColor = SIMD4(
        boundedOutdoorFloor(settings.outdoorAmbientFloor.x),
        boundedOutdoorFloor(settings.outdoorAmbientFloor.y),
        boundedOutdoorFloor(settings.outdoorAmbientFloor.z), 0)
    }
    u.lookSurface = look.surface
    u.lookLight = look.light
    u.lookGrade = look.grade
    return RenderFrame(
      id: ProjectContext.current.id, uniforms: u, camera: eye, lightSize: local ? 35 : 65,
      paused: paused, wind: local ? 0 : 1, windState: wind, look: look, items: game.items(),
      infiniteGround: false, outdoorShadow: outdoorShadow, surfaceInfluences: game.surfaceInfluences)
  }
}
