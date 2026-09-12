import AppKit
import FieldCompiler
import FieldCore
import FieldEngine
import MetalKit

/// Application composition root. The game and workshop prepare frames for one renderer.
final class WorkshopRenderer: NSObject, MTKViewDelegate {
  let secondaryPlayer = SecondaryPosePlayer()

  let graphics: MetalRenderer
  let isSoundstage: Bool
  let view: MTKView
  var batches: [GPUBatch] = []
  var floor: GPUBatch!
  var rehearsalMesh: GPUBatch?
  var rehearsalMeshSurface = RehearsalSurface()
  var wetness: Float = 0
  var atmosphereWind = WindSimulation()
  var onUpdate: (() -> Void)?
  private var accumulator: Double = 0
  var lastTime = CACurrentMediaTime()
  var camera = V3(0, 0, 24)
  var yaw: Float = 0
  var pitch: Float = -0.04
  var creatureWorkshop = CreatureWorkshop()
  var studioObject = ProjectContext.current.defaultSubject
  var inspectionCamera: InspectionCamera?
  var studioViewName = "Custom"
  var studioRevisionCache:String?
  var studioSourceObjectCache:Any?
  var studioSource = AssetSource.defaults(ProjectContext.current.defaultSubject) {
    didSet {studioRevisionCache=nil;studioSourceObjectCache=nil}
  }
  var importedSourceURL: URL?
  var sceneLook = (try? SceneLook.load()) ?? SceneLook()
  lazy var workshopSession = WorkshopSession(self)
  var studioBatches: [GPUBatch] = []
  var sculptFrame:SculptFrame?
  var studioPartCache: [String:GPUBatch] = [:]
  var studioReusedBatches = 0
  var studioRebuiltBatches = 0
  var studioBaseCache: [String:[SceneBatch]] = [:]
  var sculptAnatomyCache:[String:AnatomyCompilation] = [:]
  var sculptAnatomyCacheOrder:[String] = []
  var studioBaseCacheHit = false
  var studioCache: [String: [GPUBatch]] = [:]
  var measuredStudioBounds = Bounds(V3(-0.01, 0, -0.01), V3(0.01, 0.035, 0.01))
  var sculptReviewFrame:SculptReviewFrame?
  var sculptOverlay:SculptOverlay? {didSet {sculptOverlayCache=nil}}
  var sculptOverlayCache:(String,GPUBatch)?
  var studioBounds:Bounds {
    get {sculptReviewFrame.map{Bounds($0.minimum,$0.maximum)} ?? measuredStudioBounds}
    set {measuredStudioBounds=newValue}
  }
  var studioRig = StudioRig()
  var studioLayout = StudioLayout()
  var surfaceReview = SurfaceReview()
  var studioBuildMilliseconds: Double = 0
  var studioUnit: Float { studioObject == "sky" ? 1 : max(0.001, studioExtent / 4) }
  var scene = "studio"
  func setStudyView(_ name: String) {
    studioViewName = name
    inspectionCamera = studioSource.definition?.views.first { $0.name == name }
    if inspectionCamera != nil {
      scene = "studio"
      studioObject = studioSource.id
      return
    }
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
    if studioSource.animation != nil && studioObject != "sky" { orbit += .pi }
    creatureWorkshop.focusPart = false
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
  var orbit: Float = 0.48
  var orbitElevation: Float = 0.18
  var orbitDistance: Float = 6.8
  var paused = false
  var simulationTime: Float = 0
  var keys = Set<UInt16>()
  init(view: MTKView) throws {
    self.view = view
    self.isSoundstage = true
    graphics = try MetalRenderer(
      view: view,
      materialSourceURL: ProjectContext.current.authoring.appendingPathComponent("Materials.metal"))
    super.init()
    let floorShape = Shape.box(V3(2, 0.01, 2))
    floor = upload(
      SceneBatch(
        name: "Studio floor", mesh: try Mesher.compile(floorShape, resolution: 8),
        instances: [Instance(position: V3(0, -0.01, 0), tint: V3(0.36, 0.38, 0.34), kind: 7)]))
    resetCamera()
    try applySource(AssetSource.load(ProjectContext.current.defaultSubject))
    scene = "studio"
    resetCamera()
    paused = true
    view.delegate = self
    graphics.metadataProvider = { [weak self] in self?.snapshot() ?? [:] }
  }
  func resetCamera() {
    studioViewName = "Custom"
    camera = V3(0, 1.72, 24)
    yaw = 0
    pitch = -0.04
    orbit = 0.48
    orbitDistance = studioLayout.arrangement == "grove" ? 13 : 6.8
    orbitElevation = 0.18
    keys.removeAll()
    if scene == "studio" { fitStudioCamera() }
  }
  var forward: V3 { V3(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch)) }
  func look(dx: Float, dy: Float) {
    if var camera = inspectionCamera {
      let d = normalize(camera.target - camera.eye)
      let yaw = atan2(d.x, -d.z) + dx * 0.003
      let pitch = clamp(asin(d.y) - dy * 0.003, -1.35, 1.35)
      camera.target = camera.eye + V3(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch))
      inspectionCamera = camera
      return
    }
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
    guard var camera = inspectionCamera else { return }
    let d = normalize(camera.target - camera.eye)
    let right = normalize(cross(d, V3(0, 1, 0)))
    let delta = right * local.x + d * local.z
    camera.eye += delta
    camera.target += delta
    inspectionCamera = camera
  }

  func step(_ dt: Float) {
    simulationTime += dt
    if scene == "studio" && studioLayout.turntable { orbit += dt * 0.25 }
    if inspectionCamera != nil {
      var d = V3.zero
      if keys.contains(13) { d.z += 1 }
      if keys.contains(1) { d.z -= 1 }
      if keys.contains(0) { d.x -= 1 }
      if keys.contains(2) { d.x += 1 }
      if length_squared(d) > 0 { move(local: normalize(d) * dt * 3) }
    }
    atmosphereWind.step(dt)
    if scene == "studio", let motion = studioSource.motion {
      creatureWorkshop.step(dt, motion: motion, definition: studioSource.animation)
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
    var sun = indoorStudio ? normalize(rigPosition - studioAssetCenter) : skySettings.sunDirection
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
    if let inspection = inspectionCamera {
      let center = (studioBounds.min + studioBounds.max) / 2
      let shift = V3(-center.x, -studioBounds.min.y, -center.z)
      eye = (inspection.eye + shift) * studioLayout.scale
      target = (inspection.target + shift) * studioLayout.scale
      if studioRig.name == "flashlight" { sun = normalize(eye - target) }
    }
    let interiorLamp = inspectionCamera != nil && studioRig.name == "flashlight"
    let vp =
      perspective(1.05, 16 / 9, scene == "studio" ? max(0.00001, studioExtent * 0.001) : 0.08, 450)
      * lookAt(eye, target)
    // A millimetre-scale object camera must not determine the precision of
    // rays aimed at the sun. Reconstruct sky directions without translation.
    let skyInverse = (perspective(1.05, 16 / 9, 0.1, 450) * lookAt(.zero, target - eye)).inverse
    let lightCenter =
      interiorLamp ? target : scene == "studio" ? studioAssetCenter : V3(camera.x, 0, camera.z - 20)
    let lightSize: Float =
      scene == "studio"
      ? max(0.02, studioExtent * (studioLayout.arrangement == "grove" ? 3 : 1.2)) : 65
    let lamp =
      interiorLamp
      ? eye
      : indoorStudio
        ? rigPosition : lightCenter + sun * (scene == "studio" ? studioExtent * 5 : 110)
    let lightView = lookAt(lamp, lightCenter, up: abs(sun.y) > 0.98 ? V3(0, 0, 1) : V3(0, 1, 0))
    var lvp =
      (indoorStudio
        ? perspective(1.9, 1, max(0.00001, studioExtent * 0.01), studioExtent * 12)
        : orthographic(
          lightSize, scene == "studio" ? max(0.00001, studioExtent * 0.01) : 1,
          scene == "studio" ? studioExtent * 12 : 250)) * lightView
    if interiorLamp { lvp = perspective(1.32, 1, 0.08, 65) * lookAt(eye, target) }
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
    uniforms.rigPosition = SIMD4(
      rigPosition, studioRig.name == "flashlight" ? 2 : indoorStudio ? 1 : 0)
    let warmth = studioRig.warmth
    uniforms.rigColor = SIMD4(
      V3(1, 1 - warmth * 0.25, 1 - warmth * 0.55), studioRig.intensity * studioExtent * studioExtent
    )
    uniforms.rigParams = SIMD4(
      studioRig.size, studioRig.fill, studioExtent, studioRig.name == "flashlight" ? 0.8 : 0)
    if interiorLamp {
      uniforms.rigPosition = SIMD4(eye, 2)
      uniforms.rigColor.w = studioRig.intensity * 25
      uniforms.rigParams.z = 1
    }
    uniforms.lookSurface = sceneLook.surface
    uniforms.lookLight = sceneLook.light
    uniforms.lookGrade = sceneLook.grade
    let input = RenderFrame(
      id: scene, uniforms: uniforms, camera: camera,
      lightSize: lightSize, paused: paused, wind: wind, windState: atmosphereWind,
      look: sceneLook, items: renderItems(),
      infiniteGround: inspectionCamera == nil && scene == "studio" && studioObject != "sky"
        && studioRig.name != "room"
        && studioLayout.ground && !rehearsalActive)
    rememberSculptFrame(input)
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
      castsShadow: Bool = true, skinPalette: [simd_float4x4] = []
    ) {
      var material = batch.material
      if subject {
        if studioLayout.roughness >= 0 { material.x = studioLayout.roughness }
        if studioLayout.metallic >= 0 { material.y = studioLayout.metallic }
        // The fragment-only review flag preserves the exact deformation and shadow paths.
        if surfaceReview.mode == "clay" { material = SIMD4(0.72, 0, 0, -1) }
      }
      if emitter { material.z = 1 }
      if batch.name == "Studio floor" && !emitter { material.w = 1 }
      let overlay=subject && sculptOverlay?.part==batch.name
      if overlay {material=SIMD4(0.72,0,0,-2)}
      var instance=instance
      if overlay {var value=instance ?? batch.sourceInstances[0];value.tint=SIMD4(1,1,1,value.tint.w);instance=value}
      var item = RenderItem(batch: overlay ? overlayBatch(batch):batch, instance: instance, castsShadow: castsShadow, material: material)
      item.correctives=subject ? studioSource.correctiveUniforms(part:batch.name,clip:creatureWorkshop.mode,time:creatureWorkshop.seconds,state:creatureWorkshop.actor):[]
      item.surfaceLayers=subject && surfaceReview.mode != "clay" && !overlay ? studioSource.resolvedSurfaceLayers.filter{$0.part==batch.name}:[]
      item.skinPalette = skinPalette
      items.append(item)
    }
    do {
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
      if indoorStudio && inspectionCamera == nil {
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
      items += rehearsalItems(center:center)
      let pose = secondaryPose()
      let root = creatureWorkshop.root(studioSource.motion ?? MotionParameters())
      for (i, offset) in offsets.enumerated() {
        for batch in studioBatches where !creatureWorkshop.rigView && !surfaceReview.hiddenParts.contains(batch.name) && creatureWorkshop.visible(batch.name, source: studioSource)
        {
          let tint = batch.sourceInstances[0].tint
          var instance = Instance(
            position: offset + V3(-center.x, -studioBounds.min.y, -center.z) * studioLayout.scale,
            scale: V3(repeating: studioLayout.scale), yaw: Float(i) * 0.7,
            tint: V3(tint.x, tint.y, tint.z) * studioLayout.tint, kind: tint.w)
          instance.model =
            instance.model * root * (batch.skinJoints.isEmpty ? (pose[batch.name] ?? matrix_identity_float4x4) : matrix_identity_float4x4)
            * batch.sourceInstances[0].model
          draw(batch, instance: instance, subject: true, skinPalette: batch.skinJoints.map { pose[$0] ?? matrix_identity_float4x4 })
        }
      }
      if creatureWorkshop.rigView {
        var points: [String:V3]=[:]
        for joint in studioSource.joints {
          let p=root*(pose[joint.id] ?? matrix_identity_float4x4)*SIMD4(joint.pivot,1)
          let point=(V3(p.x,p.y,p.z)-V3(center.x,studioBounds.min.y,center.z))*studioLayout.scale
          points[joint.id]=point
          let size=e*0.018
          draw(floor,instance:Instance(position:point,scale:V3(size/4,size/0.02,size/4),
            tint:joint.id == creatureWorkshop.selected ? V3(0.8,0.2,0.1):V3(0.8,0.63,0.25),kind:7),castsShadow:false)
        }
        let links=studioSource.joints.compactMap { j in j.parent.map { ($0,j.id) } } + (studioSource.animation?.rigLinks ?? [])
        for (a,b) in links {
          guard let start=points[a],let end=points[b],length(end-start)>0.001 else {continue}
          let thickness=e*0.004
          var line=Instance(tint:V3(0.4,0.65,0.65),kind:7)
          line.model=transform((start+end)/2,V3(repeating:1),0)
            * simd_float4x4(simd_quatf(from:V3(0,1,0),to:normalize(end-start)))
            * simd_float4x4(diagonal:SIMD4(thickness/4,length(end-start)/0.02,thickness/4,1))
          draw(floor,instance:line,castsShadow:false)
        }
      }
      if creatureWorkshop.mode == "behavior" {
        let input = creatureWorkshop.input
        let shift = SIMD2<Float>.zero
        func marker(_ p: SIMD2<Float>, _ color: V3, _ radius: Float, _ height: Float) {
          draw(
            floor,
            instance: Instance(
              position: V3(p.x - shift.x, height / 2, p.y - shift.y) * studioLayout.scale,
              scale: V3(radius / 2, height * 50, radius / 2) * studioLayout.scale, tint: color,
              kind: 7))
        }
        marker(input.player, input.running ? V3(0.9, 0.22, 0.1) : V3(0.2, 0.5, 0.9), 0.25, 1.0)
        if let food = input.food { marker(food, V3(0.3, 0.75, 0.2), 0.15, 0.15) }
        if let obstacle = input.obstacle {
          marker(obstacle, V3(0.4, 0.4, 0.4), input.obstacleRadius * 2, 0.65)
        }
        marker(creatureWorkshop.actor.target, V3(0.95, 0.65, 0.1), 0.09, 0.025)
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
    return items
  }
  func snapshot() -> [String: Any] {
    let triangles = visibleTriangles
    return [
      "project": ProjectContext.current.id,
      "projectDirectory": ProjectContext.current.directory.path,
      "artDirectionPath": SceneLook.url.path,
      "sceneLook": sceneLook.values,
      "workshop": workshopSnapshot(),
      "sculptFrame": sculptFrameMetadata,
      "mode": isSoundstage ? "soundstage" : "game", "sky": skySettings.snapshot, "scene": scene,
      "studioObject": studioObject, "antialiasing": "4x MSAA", "seed": 82317, "frame": frame,
      "time": simulationTime, "paused": paused,
      "camera": [
        "position": [camera.x, camera.y, camera.z], "yaw": yaw, "pitch": pitch, "orbit": orbit,
        "elevation": orbitElevation, "distance": orbitDistance,
      ],
      "lighting": lighting, "parameters": ["exposure": exposure, "wind": wind, "scale": studioLayout.scale],
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
      "groundHeight": 0,
      "sourceRevision": ProcessInfo.processInfo.environment["SANCTUARY_REVISION"] ?? Bundle.main
        .object(forInfoDictionaryKey: "SanctuaryRevision") as? String ?? "working-tree",
      "sourceDigest": Bundle.main.object(forInfoDictionaryKey: "SanctuarySourceDigest") as? String
        ?? "unbundled", "shaderDigest": shaderDigest, "gpuErrors": gpuErrors,
      "renderer": useMeshlets
        ? "Field-derived meshes / Metal mesh shaders" : "Field-derived meshes / indexed raster",
      "worldUnit": "metre",
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
