import AppKit
import FieldCompiler
import FieldCore
import MetalKit
import SanctuaryContent

/// Application composition root. The game and workshop prepare frames for one renderer.
final class SanctuarySession: NSObject, MTKViewDelegate {
  let graphics: MetalRenderer
  let isSoundstage: Bool
  var expedition: ExpeditionController?
  var expeditionPresentation: ExpeditionPresentation?
  var world: GardenWorld
  let view: MTKView
  var batches: [GPUBatch] = []
  var pod: GPUBatch!
  var floor: GPUBatch!
  var wetness: Float = 0
  var atmosphereWind = WindSimulation()
  var onUpdate: (() -> Void)?
  private var accumulator: Double = 0
  var lastTime = CACurrentMediaTime()
  var camera = V3(0, 0, 24)
  var yaw: Float = 0
  var pitch: Float = -0.04
  var studioObject = "seed"
  var studioViewName = "Custom"
  var studioSource = AssetSource.defaults("seed")
  var importedSourceURL: URL?
  var sceneLook = (try? SceneLook.load()) ?? SceneLook()
  lazy var workshopSession = WorkshopSession(self)
  var studioBatches: [GPUBatch] = []
  var studioCache: [String: [GPUBatch]] = [:]
  var studioBounds = Bounds(V3(-0.01, 0, -0.01), V3(0.01, 0.035, 0.01))
  var studioRig = StudioRig()
  var studioLayout = StudioLayout()
  var studioBuildMilliseconds: Double = 0
  var studioUnit: Float { studioObject == "sky" ? 1 : max(0.001, studioExtent / 4) }
  var scene = "garden"
  func setStudyView(_ name: String) {
    studioViewName = name
    scene = "studio"
    orbitDistance = 6.8
    studioLayout.focus = 0.5
    if !["sunward", "away", "zenith"].contains(name) { studioObject = studioSource.id }
    switch name {
    case "sunward", "away", "zenith":
      studioObject = "sky"
      orbit = skySettings.azimuth * .pi / 180 + (name == "away" ? .pi : 0)
      orbitElevation = name == "zenith" ? 1.5 : 0.48
    case "back":
      orbit = .pi
      orbitElevation = 0.2
    case "left":
      orbit = -.pi / 2
      orbitElevation = 0.2
    case "right":
      orbit = .pi / 2
      orbitElevation = 0.2
    case "quarter":
      orbit = 0.8
      orbitElevation = 0.2
    case "above":
      orbit = 0.5
      orbitElevation = 1.0
    case "detail", "base", "crown":
      orbit = 0.6
      orbitElevation = 0.2
      orbitDistance = 2
      studioLayout.focus = name == "base" ? 0.12 : (name == "crown" ? 0.8 : 0.5)
    default:
      orbit = 0
      orbitElevation = 0.2
    }
    if !["detail", "base", "crown", "sunward", "away", "zenith"].contains(name) {
      fitStudioCamera()
    }
  }
  var skySettings = SkySettings.preset("morning")
  var lighting = "morning" {
    didSet {
      skySettings = SkySettings.preset(lighting)
      studioRig = StudioRig.preset(lighting == "indoor" ? "room" : "outdoor")
    }
  }
  var exposure: Float = 1
  var wind: Float = 1
  var podScale: Float = 1
  var orbit: Float = 0.48
  var orbitElevation: Float = 0.18
  var orbitDistance: Float = 6.8
  var paused = false
  var simulationTime: Float = 0
  var keys = Set<UInt16>()
  init(view: MTKView, world: GardenWorld, soundstage: Bool) throws {
    self.view = view
    self.world = world
    self.isSoundstage = soundstage
    graphics = try MetalRenderer(view: view)
    super.init()
    batches = world.batches.map(upload)
    pod = upload(SceneBatch(name: "Seed pod", mesh: world.studioMesh, instances: [Instance()]))
    let floorShape = Shape.box(V3(2, 0.01, 2))
    floor = upload(
      SceneBatch(
        name: "Studio floor", mesh: try Mesher.compile(floorShape, resolution: 8),
        instances: [Instance(position: V3(0, -0.01, 0), tint: V3(0.36, 0.38, 0.34), kind: 7)]))
    resetCamera()
    try applySource(AssetSource.load(isSoundstage ? "tree" : "seed"))
    scene = isSoundstage ? "studio" : "garden"
    resetCamera()
    if isSoundstage { paused = true }
    if !soundstage {
      let root = AssetSource.workspace.appendingPathComponent(".sanctuary")
      expedition = try ExpeditionController(root: root)
      expeditionPresentation = try ExpeditionPresentation(
        graphics: graphics, terrain: world.terrain)
      let saved = expedition!.state
      camera = V3(
        saved.player.x, world.terrain.height(saved.player.x, saved.player.y) + 1.72, saved.player.y)
      yaw = saved.yaw
      pitch = saved.pitch
    }
    view.delegate = self
    graphics.metadataProvider = { [weak self] in self?.snapshot() ?? [:] }
  }
  func resetCamera() {
    studioViewName = "Custom"
    camera = V3(0, world.terrain.height(0, 24) + 1.72, 24)
    yaw = 0
    pitch = -0.04
    orbit = 0.48
    orbitDistance = studioLayout.arrangement == "grove" ? 13 : 6.8
    orbitElevation = 0.18
    keys.removeAll()
    if scene == "studio" { fitStudioCamera() }
  }
  var forward: V3 { V3(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch)) }
  var podPosition: V3 {
    V3(
      5,
      world.terrain.height(5, 12) - world.studioShape.bounds.min.y * podScale
        * GardenWorld.podUnitScale, 12)
  }

  func look(dx: Float, dy: Float) {
    if scene == "studio" {
      studioViewName = "Custom"
      orbit += dx * 0.006
      orbitElevation = clamp(orbitElevation + dy * 0.004, -0.1, 1.5)
    } else {
      yaw += dx * 0.003
      pitch = clamp(pitch - dy * 0.003, -1.35, 1.35)
    }
  }

  func move(local: V3) {
    guard scene == "garden" else { return }
    let right = V3(cos(yaw), 0, sin(yaw))
    let forward = V3(sin(yaw), 0, -cos(yaw))
    let delta = right * local.x + forward * local.z
    let count = max(1, Int(ceil(length(delta) / 0.15)))
    for _ in 0..<count {
      var p = camera + delta / Float(count)
      p.x = clamp(p.x, -130, 130)
      p.z = clamp(p.z, -135, 120)
      var ground = world.terrain.height(p.x, p.z)
      for off in [V3(0.25, 0, 0), V3(-0.25, 0, 0), V3(0, 0, 0.25), V3(0, 0, -0.25)] {
        ground = max(ground, world.terrain.height(p.x + off.x, p.z + off.z))
      }
      p.y = ground + 1.72
      var nearby = world.collisionGrid.candidates(at: p).map { world.solids[$0] }
      nearby.append(
        Solid(
          shape: world.studioShape, position: podPosition,
          scale: podScale * GardenWorld.podUnitScale, name: "Seed pod"))
      for solid in nearby {
        for height: Float in [0.35, 1.0, 1.55] {
          let sample = V3(p.x, ground + height, p.z)
          let d = solid.value(sample)
          if d < 0.28 {
            let gradient = solid.shape.sample(at: (sample - solid.position) / solid.scale).gradient
            var n = V3(gradient.x, 0, gradient.z)
            if length_squared(n) > 0.000001 {
              n = normalize(n)
              p += n * (0.28 - d)
            }
          }
        }
      }
      p.y = max(world.terrain.height(p.x, p.z), ground) + 1.72
      camera = p
    }
  }

  func step(_ dt: Float) {
    simulationTime += dt
    if scene == "studio" && studioLayout.turntable { orbit += dt * 0.25 }
    atmosphereWind.step(dt)
    if scene == "garden" {
      var m = V3.zero
      if keys.contains(13) { m.z += 1 }
      if keys.contains(1) { m.z -= 1 }
      if keys.contains(0) { m.x -= 1 }
      if keys.contains(2) { m.x += 1 }
      if length_squared(m) > 0 { move(local: normalize(m) * dt * (keys.contains(56) ? 10 : 5.5)) }
      syncExpeditionPlayer()
      expedition?.advance(
        dt, running: keys.contains(56) && length_squared(m) > 0, visible: creatureVisible)
    }
  }

  func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
  func draw(in view: MTKView) {
    let start = CACurrentMediaTime()
    let elapsed = start - lastTime
    lastTime = start
    let simulationStart = CACurrentMediaTime()
    if !paused {
      accumulator += min(elapsed, 0.05)
      while accumulator >= 1.0 / 60 {
        step(1 / 60)
        accumulator -= 1.0 / 60
      }
    } else {
      accumulator = 0
    }
    let sun = indoorStudio ? normalize(rigPosition - studioAssetCenter) : skySettings.sunDirection
    var eye: V3
    var target: V3
    if scene == "studio" {
      target = studioCenter
      eye =
        target + V3(
          sin(orbit) * cos(orbitElevation), sin(orbitElevation), cos(orbit) * cos(orbitElevation))
        * orbitDistance * studioUnit
    } else {
      eye = camera
      target = camera + forward
    }
    if scene == "studio" && studioObject == "sky" {
      eye = V3(0, 1.72, 0)
      target =
        eye
        + V3(
          sin(orbit) * cos(orbitElevation), sin(orbitElevation), -cos(orbit) * cos(orbitElevation))
    }
    let vp =
      perspective(1.05, 16 / 9, scene == "studio" ? max(0.00001, studioExtent * 0.001) : 0.08, 450)
      * lookAt(eye, target)
    // A millimetre-scale object camera must not determine the precision of
    // rays aimed at the sun. Reconstruct sky directions without translation.
    let skyInverse = (perspective(1.05, 16 / 9, 0.1, 450) * lookAt(.zero, target - eye)).inverse
    let lightCenter = scene == "studio" ? studioAssetCenter : V3(camera.x, 0, camera.z - 20)
    let lightSize: Float =
      scene == "studio"
      ? max(0.02, studioExtent * (studioLayout.arrangement == "grove" ? 3 : 1.2)) : 65
    let lamp =
      indoorStudio ? rigPosition : lightCenter + sun * (scene == "studio" ? studioExtent * 5 : 110)
    let lightView = lookAt(lamp, lightCenter, up: abs(sun.y) > 0.98 ? V3(0, 0, 1) : V3(0, 1, 0))
    var lvp =
      (indoorStudio
        ? perspective(1.9, 1, max(0.00001, studioExtent * 0.01), studioExtent * 12)
        : orthographic(
          lightSize, scene == "studio" ? max(0.00001, studioExtent * 0.01) : 1,
          scene == "studio" ? studioExtent * 12 : 250)) * lightView
    // Anchor the shadow projection to whole texels instead of following every camera subpixel.
    let origin = lvp * SIMD4<Float>(0, 0, 0, 1)
    lvp[3].x += (round(origin.x * 1024) - origin.x * 1024) / 1024
    lvp[3].y += (round(origin.y * 1024) - origin.y * 1024) / 1024
    var uniforms = Uniforms(
      viewProjection: vp, lightVP: lvp, inverseVP: skyInverse,
      cameraTime: SIMD4(eye, simulationTime), sunExposure: SIMD4(sun, exposure),
      options: SIMD4(lighting == "sunset" ? 1 : 0, wind, scene == "studio" ? 1 : 0, 0),
      sky: SIMD4(
        skySettings.coverage, skySettings.density, skySettings.haze, skySettings.cloudSeed),
      environment: SIMD4(
        [
          "morning": 0, "noon": 0, "sunset": 1, "overcast": 2, "rain": 3, "indoor": 4, "golden": 5,
          "afterglow": 6,
        ][lighting] ?? 0, wetness, 0, 0))
    if indoorStudio { uniforms.environment.x = 4 }
    uniforms.rigPosition = SIMD4(rigPosition, indoorStudio ? 1 : 0)
    let warmth = studioRig.warmth
    uniforms.rigColor = SIMD4(
      V3(1, 1 - warmth * 0.25, 1 - warmth * 0.55), studioRig.intensity * studioExtent * studioExtent
    )
    uniforms.rigParams = SIMD4(studioRig.size, studioRig.fill, studioExtent, 0)
    uniforms.lookSurface = sceneLook.surface
    uniforms.lookLight = sceneLook.light
    uniforms.lookGrade = sceneLook.grade
    let input = RenderFrame(
      id: scene, uniforms: uniforms, camera: camera,
      lightSize: lightSize, paused: paused, wind: wind, windState: atmosphereWind,
      look: sceneLook, items: renderItems(),
      infiniteGround: scene == "studio" && studioObject != "sky" && studioRig.name != "room"
        && studioLayout.ground)
    graphics.draw(
      input, elapsed: elapsed,
      simulationMilliseconds: (CACurrentMediaTime() - simulationStart) * 1000)
    if graphics.frame % 20 == 0 { onUpdate?() }
  }
  func renderItems() -> [RenderItem] {
    guard !(scene == "studio" && studioObject == "sky") else { return [] }
    var items: [RenderItem] = []
    func draw(
      _ batch: GPUBatch, instance: Instance? = nil, subject: Bool = false, emitter: Bool = false,
      castsShadow: Bool = true
    ) {
      var material = batch.material
      if subject {
        if studioLayout.roughness >= 0 { material.x = studioLayout.roughness }
        if studioLayout.metallic >= 0 { material.y = studioLayout.metallic }
      }
      if emitter { material.z = 1 }
      if batch.name == "Studio floor" && !emitter { material.w = 1 }
      items.append(
        RenderItem(batch: batch, instance: instance, castsShadow: castsShadow, material: material))
    }
    if scene == "garden" {
      for b in batches {
        draw(b, castsShadow: !["Meadow", "Wildflowers", "Leaves"].contains(b.name))
      }
      draw(
        pod,
        instance: Instance(
          position: podPosition, scale: V3(repeating: podScale * GardenWorld.podUnitScale),
          tint: V3(0.48, 0.27, 0.13), kind: 6))
    } else {
      let e = studioExtent
      if studioRig.name == "room" {
        draw(
          floor,
          instance: Instance(
            position: V3(0, -e * 0.01, 0), scale: V3(e * 4, e, e * 4), tint: V3(repeating: 0.43),
            kind: 7))
      }
      if studioRig.name == "room" {
        for z: Float in [-4, 4] {
          draw(
            floor,
            instance: Instance(
              position: V3(0, e * 1.5, z * e), scale: V3(e * 2, e * 150, e * 0.02),
              tint: V3(0.55, 0.53, 0.5), kind: 7))
        }
        for x: Float in [-4, 4] {
          draw(
            floor,
            instance: Instance(
              position: V3(x * e, e * 1.5, 0), scale: V3(e * 0.02, e * 150, e * 2),
              tint: V3(0.5, 0.53, 0.55), kind: 7))
        }
        draw(
          floor,
          instance: Instance(
            position: V3(0, e * 3, 0), scale: V3(e * 2, e, e * 2), tint: V3(repeating: 0.6), kind: 7
          ))
      }
      if indoorStudio {
        draw(
          floor,
          instance: Instance(
            position: rigPosition,
            scale: V3(e * studioRig.size, e * studioRig.size, e * studioRig.size),
            tint: V3(1, 1 - studioRig.warmth * 0.25, 1 - studioRig.warmth * 0.55), kind: 7),
          emitter: true, castsShadow: false)
      }
      let center = (studioBounds.min + studioBounds.max) / 2
      let offsets: [V3] =
        studioLayout.arrangement == "grove"
        ? [.zero, V3(-1.25 * e, 0, -e), V3(1.25 * e, 0, -e)] : [.zero]
      for (i, offset) in offsets.enumerated() {
        for batch in studioBatches {
          let tint = batch.sourceInstances[0].tint
          var instance = Instance(
            position: offset + V3(-center.x, -studioBounds.min.y, -center.z) * studioLayout.scale,
            scale: V3(repeating: studioLayout.scale), yaw: Float(i) * 0.7,
            tint: V3(tint.x, tint.y, tint.z) * studioLayout.tint, kind: tint.w)
          instance.model = instance.model * batch.sourceInstances[0].model
          draw(batch, instance: instance, subject: true)
        }
      }
      if studioLayout.reference {
        // A 1.72 m measuring post, with a short horizontal eye-height bar.
        draw(
          floor,
          instance: Instance(
            position: V3(e * 0.7, 0.86, 0), scale: V3(0.015, 86, 0.015), tint: V3(0.8, 0.38, 0.14),
            kind: 7))
        draw(
          floor,
          instance: Instance(
            position: V3(e * 0.7, 1.72, 0), scale: V3(0.12, 1, 0.015), tint: V3(0.8, 0.38, 0.14),
            kind: 7))
      }
    }
    if scene == "garden", let expedition, let expeditionPresentation {
      items += expeditionPresentation.items(expedition.state)
    }
    return items
  }
  func snapshot() -> [String: Any] {
    let triangles =
      scene == "garden"
      ? batches.reduce(0) { $0 + $1.indexCount / 3 * $1.instanceCount } + pod.indexCount / 3
      : visibleTriangles
    return [
      "expedition": expedition?.snapshot ?? [:], "sceneLook": sceneLook.values,
      "gameTreeParameters": world.treeSource.parameters, "workshop": workshopSnapshot(),
      "mode": isSoundstage ? "soundstage" : "game", "sky": skySettings.snapshot, "scene": scene,
      "studioObject": studioObject, "antialiasing": "4x MSAA", "seed": world.seed, "frame": frame,
      "time": simulationTime, "paused": paused,
      "camera": [
        "position": [camera.x, camera.y, camera.z], "yaw": yaw, "pitch": pitch, "orbit": orbit,
        "elevation": orbitElevation, "distance": orbitDistance,
      ],
      "lighting": lighting, "parameters": ["exposure": exposure, "wind": wind, "scale": podScale],
      "renderSize": [1920, 1080], "device": device.name, "fullDetailTriangles": triangles,
      "thermalState": ProcessInfo.processInfo.thermalState.rawValue,
      "lowPowerMode": ProcessInfo.processInfo.isLowPowerModeEnabled,
      "skySnapshotTimes": [atmosphere.previousPublishedTime, atmosphere.publishedTime],
      "gpuPassSampleCounts": gpuPassTimes.mapValues(\.count), "profiling": profiling,
      "gpuCountersSupported": GPUProfile.supported(device),
      "gpuPassMilliseconds": gpuPassTimes.mapValues { stats($0) },
      "gpuMilliseconds": stats(gpuTimes),
      "frameGPUBySkyPhase": gpuPhaseTimes.mapValues { stats($0) },
      "skyUpdatePhase": atmosphere.updatePhase,
      "weather": [
        "model": "moist columns", "seconds": atmosphere.weatherTime,
        "totalWater": atmosphere.weather.totalWater, "evaporated": atmosphere.weather.evaporated,
        "precipitated": atmosphere.weather.precipitated,
      ], "cpuEncodeMilliseconds": stats(cpuTimes), "frameIntervalMilliseconds": stats(frameTimes),
      "cpuSimulationMilliseconds": stats(simulationTimes),
      "cpuAtmosphereMilliseconds": stats(atmosphereCPUTimes),
      "submissionToCompletionMilliseconds": stats(completionTimes),
      "presentedIntervalMilliseconds": stats(presentedIntervals),
      "presentedFrames": presentedFrames,
      "drawableWaitMilliseconds": stats(drawableWaitTimes), "visibleTriangles": visibleTriangles,
      "shadowTriangles": shadowTriangles,
      "timingSampleCount": gpuTimes.count, "allocatedMetalBytes": device.currentAllocatedSize,
      "groundHeight": world.terrain.height(camera.x, camera.z),
      "sourceRevision": ProcessInfo.processInfo.environment["SANCTUARY_REVISION"] ?? Bundle.main
        .object(forInfoDictionaryKey: "SanctuaryRevision") as? String ?? "working-tree",
      "sourceDigest": Bundle.main.object(forInfoDictionaryKey: "SanctuarySourceDigest") as? String
        ?? "unbundled", "shaderDigest": shaderDigest, "gpuErrors": gpuErrors,
      "renderer": useMeshlets
        ? "Field-derived meshes / Metal mesh shaders" : "Field-derived meshes / indexed raster",
      "worldUnit": "metre", "seedPodHeightMetres": GardenWorld.podMetres * podScale,
      "wetness": wetness, "indirectLighting": "sky irradiance with approximate ground bounce",
      "exposureMeter": "clear sky hemisphere, gain 1...24",
      "skyCacheSize": [atmosphere.sky.width, atmosphere.sky.height],
      "cloudModel":
        "conservative moist-air transport, saturation adjustment, buoyancy and procedural 3D density",
      "cloudCacheExtentKm": 64, "windTicks": atmosphereWind.ticks,
      "sessionPID": ProcessInfo.processInfo.processIdentifier,
    ]
  }
}
