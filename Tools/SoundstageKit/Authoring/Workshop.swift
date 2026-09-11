import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import MetalKit
import simd

struct StudioRig: Codable {
  var name = "outdoor"
  // Positions and emitter radius in subject extents. They remain useful from
  // millimetre seeds to full trees; metadata also publishes metres.
  var x: Float = -1.4
  var y: Float = 1.8
  var z: Float = 1.5
  var intensity: Float = 16
  var size: Float = 0.18
  var warmth: Float = 0.25
  var fill: Float = 0.18
  static let names = ["outdoor", "softbox", "overhead", "lamp", "room", "flashlight"]
  static func preset(_ name: String) -> Self {
    var r = Self()
    r.name = name
    switch name {
    case "overhead":
      r.x = 0
      r.y = 2
      r.z = 0.35
      r.size = 0.35
      r.fill = 0.015
      r.warmth = 0
    case "flashlight":
      r.x = -0.6
      r.y = 0.7
      r.z = -1.5
      r.intensity = 3
      r.size = 0.015
      r.fill = 0
      r.warmth = 0.1
    case "lamp":
      r.x = -0.85
      r.y = 0.35
      r.z = 1.1
      r.size = 0.035
      r.intensity = 3
      r.fill = 0.005
      r.warmth = 0.9
    case "room":
      r.x = 0.4
      r.y = 2
      r.z = 0.6
      r.size = 0.3
      r.fill = 0.12
      r.warmth = 0.45
    default: break
    }
    return r
  }
  func validate() throws {
    guard Self.names.contains(name),
      [x, y, z, intensity, size, warmth, fill].allSatisfy(\.isFinite), (-4...4).contains(x),
      (0.1...4).contains(y), (-4...4).contains(z), (0...40).contains(intensity),
      (0.005...1).contains(size), (0...1).contains(warmth), (0...0.5).contains(fill)
    else { throw RuntimeError.message("Invalid lighting rig; see catalog for ranges") }
  }
}
struct StudioLayout: Codable {
  var arrangement = "single"
  var scale: Float = 1
  var reference = false
  var ground = true
  var turntable = false
  var roughness: Float = -1
  var metallic: Float = -1
  var tint: Float = 1
  var focus: Float = 0.5
  func validate() throws {
    guard ["single", "grove"].contains(arrangement),
      [scale, roughness, metallic, tint, focus].allSatisfy(\.isFinite), (0.25...4).contains(scale),
      roughness == -1 || (0.05...1).contains(roughness),
      metallic == -1 || (0...1).contains(metallic), (0.3...1.5).contains(tint),
      (0...1).contains(focus)
    else { throw RuntimeError.message("Invalid subject layout or material values") }
  }
}
struct WorkshopDocument: Codable {
  var inspectionCamera: InspectionCamera?
  var project: String?
  var creature: CreatureWorkshop?
  var version = 1
  var source: AssetSource
  var skyOnly: Bool
  var rig: StudioRig
  var layout: StudioLayout
  var lighting: String
  var sky: [String: Float]
  var orbit: Float
  var elevation: Float
  var distance: Float
  var exposure: Float
  var wind: Float
  var wetness: Float
  var time: Float
  var paused: Bool
  var windState: WindSimulation?
  var sourcePath: String?
  var sceneLook: SceneLook?
  var viewName: String?
}

extension WorkshopRenderer {
  var studioExtent: Float {
    max(
      studioBounds.max.x - studioBounds.min.x,
      max(studioBounds.max.y - studioBounds.min.y, studioBounds.max.z - studioBounds.min.z))
      * studioLayout.scale
  }
  var studioHeight: Float { (studioBounds.max.y - studioBounds.min.y) * studioLayout.scale }
  var creatureCameraOffset: V3 {
    guard creatureWorkshop.mode == "behavior" else { return .zero }
    if !creatureWorkshop.follow,
      let bounds = studioSource.animation?.bounds(creatureWorkshop.scenario)
    {
      let center = (bounds.min + bounds.max) / 2
      return V3(center.x, 0, center.z) * studioLayout.scale
    }
    let p = creatureWorkshop.actor.position
    return V3(p.x, 0, p.y) * studioLayout.scale
  }
  var studioAssetCenter: V3 { V3(0, studioHeight * 0.5, 0) + creatureCameraOffset }
  var studioCenter: V3 {
    selectedPartCenter ?? (V3(0, studioHeight * studioLayout.focus, 0) + creatureCameraOffset)
  }
  var rigPosition: V3 {
    studioAssetCenter + V3(studioRig.x, studioRig.y, studioRig.z) * studioExtent
  }
  var indoorStudio: Bool { scene == "studio" && studioRig.name != "outdoor" }
  var studioSourceURL: URL { importedSourceURL ?? AssetSource.url(studioSource.id) }
  func boundedStudioDistance(_ value: Float) -> Float {
    clamp(value, 0.2, min(10000, 400 / studioUnit))
  }
  func fitStudioCamera() {
    guard studioObject != "sky" else { return }
    creatureWorkshop.focusPart = false
    let direction = V3(
      sin(orbit) * cos(orbitElevation), sin(orbitElevation), cos(orbit) * cos(orbitElevation))
    let right = normalize(cross(V3(0, 1, 0), direction))
    let up = cross(direction, right)
    let extents = (studioBounds.max - studioBounds.min) * studioLayout.scale
    var lo = V3(-extents.x / 2, 0, -extents.z / 2)
    var hi = V3(extents.x / 2, extents.y, extents.z / 2)
    if studioLayout.arrangement == "grove" {
      lo.x -= studioExtent * 1.3
      hi.x += studioExtent * 1.3
      lo.z -= studioExtent
    }
    if creatureWorkshop.mode == "behavior", !creatureWorkshop.follow {
      let bounds =
        studioSource.animation?.bounds(creatureWorkshop.scenario)
        ?? Bounds(V3(-3, 0, -3), V3(3, 2, 3))
      let offset = creatureCameraOffset
      lo = bounds.min * studioLayout.scale - offset
      hi = bounds.max * studioLayout.scale - offset
    }

    if studioLayout.reference {
      hi.y = max(hi.y, 1.74)
      hi.x = max(hi.x, studioExtent * 0.7 + 0.25)
    }
    var distance: Float = 0
    for x in [lo.x, hi.x] {
      for y in [lo.y, hi.y] {
        for z in [lo.z, hi.z] {
          let p = V3(x, y, z) - (studioCenter - creatureCameraOffset)
          distance = max(
            distance,
            dot(p, direction)
              + max(
                abs(dot(p, right)) / (tan(1.05 / 2) * Float(16) / 9 * 0.78),
                abs(dot(p, up)) / (tan(1.05 / 2) * 0.78)))
        }
      }
    }
    orbitDistance = boundedStudioDistance((distance + studioExtent * 0.04) / studioUnit)
  }
  func selectSubject(_ name: String) throws {
    scene = "studio"
    if name == "sky" {
      studioObject = "sky"
      setStudyView("sunward")
      return
    }
    let source = try AssetSource.load(name)
    try applySource(source)
    importedSourceURL = nil
    studioLayout = StudioLayout()
    resetCamera()
  }
  func applySource(_ source: AssetSource) throws {
    let start = CACurrentMediaTime()
    let oldDistance = orbitDistance * studioUnit
    try source.validate()
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    var geometry = source
    geometry.motion = nil
    geometry.parts = source.resolvedParts
    geometry.partOverrides = [:]
    if source.definition?.compile == nil {
      geometry.generator = "fields"
      geometry.parameters = [:]
    }
    for i in geometry.parts.indices {
      geometry.parts[i].joint = PartJoint(id: geometry.parts[i].key)
    }
    let key = String(data: try encoder.encode(geometry), encoding: .utf8)!
    let prepared: [GPUBatch]
    if let cached = studioCache[key] {
      prepared = cached
    } else {
      let compiled = try source.compile()
      guard compiled.allSatisfy({ !$0.mesh.indices.isEmpty }) else {
        throw RuntimeError.message("Field produced an empty surface")
      }
      prepared = compiled.map(upload)
      if studioCache.count >= 6 { studioCache.removeAll() }
      studioCache[key] = prepared
    }
    // True compiled bounds, excluding the renderer's conservative wind margin.
    var lo = V3(repeating: Float.greatestFiniteMagnitude)
    var hi = -lo
    let rest = PartRig.matrices(source.joints)
    for batch in prepared {
      let vertices = batch.vertices.contents().bindMemory(
        to: Vertex.self, capacity: batch.vertices.length / MemoryLayout<Vertex>.stride)
      for i in 0..<batch.vertices.length / MemoryLayout<Vertex>.stride {
        let point =
          (rest[batch.name] ?? matrix_identity_float4x4) * batch.sourceInstances[0].model
          * vertices[i].position
        let p = V3(point.x, point.y, point.z)
        lo = simd_min(lo, p)
        hi = simd_max(hi, p)
      }
    }
    guard [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z].allSatisfy(\.isFinite),
      (0.001...100).contains(length(hi - lo))
    else {
      throw RuntimeError.message("Transformed subject bounds must span 1 mm...100 m")
    }
    if source.id != studioSource.id {
      creatureWorkshop = CreatureWorkshop()
      inspectionCamera = nil
    }
    creatureWorkshop.generator = source.generator
    if creatureWorkshop.scenario.isEmpty {
      creatureWorkshop.scenario = source.animation?.scenarios.first ?? ""
      creatureWorkshop.reset()
    }
    if source.motion == nil { creatureWorkshop.mode = "bind" }
    if !source.resolvedParts.contains(where: { $0.key == creatureWorkshop.selected }) {
      creatureWorkshop.selected = nil
    }
    studioSource = source
    studioBatches = prepared
    studioObject = source.id
    studioBounds = Bounds(lo, hi)
    orbitDistance = boundedStudioDistance(oldDistance / studioUnit)
    studioBuildMilliseconds = (CACurrentMediaTime() - start) * 1000
    scene = "studio"
  }
  func setStudioScale(_ scale: Float) {
    let distance = orbitDistance * studioUnit
    studioLayout.scale = scale
    orbitDistance = boundedStudioDistance(distance / studioUnit)
  }
  func editShape(_ values: [String: Float]) throws {
    guard !studioSource.controls.isEmpty else {
      throw RuntimeError.message(
        "This subject uses field parts; edit its source JSON and reload it")
    }
    var candidate = studioSource
    for (key, value) in values { candidate.parameters[key] = value }
    try applySource(candidate)
  }
  func saveAsset() throws -> URL {
    var candidate = studioSource
    if studioLayout.roughness >= 0 { candidate.appearance["roughness"] = studioLayout.roughness }
    if studioLayout.metallic >= 0 { candidate.appearance["metallic"] = studioLayout.metallic }
    candidate.appearance["tint"] = (candidate.appearance["tint"] ?? 1) * studioLayout.tint
    try candidate.validate()
    let url = AssetSource.url(candidate.id)
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(candidate).write(to: url, options: .atomic)
    try applySource(candidate)
    importedSourceURL = nil
    workshopSession.resetWatch()
    studioLayout.roughness = -1
    studioLayout.metallic = -1
    studioLayout.tint = 1
    return url
  }
  func workshopDocument(includeWind: Bool = true) -> WorkshopDocument {
    WorkshopDocument(
      inspectionCamera: inspectionCamera, project: ProjectContext.current.id,
      creature: creatureWorkshop, source: studioSource, skyOnly: studioObject == "sky",
      rig: studioRig, layout: studioLayout,
      lighting: lighting,
      sky: [
        "altitude": skySettings.altitude, "azimuth": skySettings.azimuth,
        "coverage": skySettings.coverage, "density": skySettings.density, "haze": skySettings.haze,
        "cloudSeed": skySettings.cloudSeed,
      ], orbit: orbit, elevation: orbitElevation, distance: orbitDistance, exposure: exposure,
      wind: wind, wetness: wetness, time: simulationTime, paused: paused,
      windState: includeWind ? atmosphereWind : nil, sourcePath: importedSourceURL?.path,
      sceneLook: sceneLook, viewName: studioViewName)
  }
  func restoreWorkshop(_ document: WorkshopDocument) throws {
    var d = document
    guard d.project == nil || d.project == ProjectContext.current.id else {
      throw RuntimeError.message("This study belongs to another project; switch projects first")
    }
    d.creature?.generator = d.source.generator
    if let c = d.inspectionCamera {
      guard
        [c.eye.x, c.eye.y, c.eye.z, c.target.x, c.target.y, c.target.z].allSatisfy({
          $0.isFinite && abs($0) < 1000
        }), length(c.eye - c.target) > 0.001
      else { throw RuntimeError.message("Invalid inspection camera") }
    }
    try d.source.validate()
    try d.creature?.validate(source: d.source)
    try d.rig.validate()
    try d.layout.validate()
    try d.sceneLook?.validate()
    if let state = d.windState, !state.validSnapshot {
      throw RuntimeError.message("Invalid wind snapshot")
    }
    guard d.version == 1,
      ["morning", "noon", "golden", "sunset", "afterglow", "overcast", "rain", "indoor"].contains(
        d.lighting),
      [d.orbit, d.elevation, d.distance, d.exposure, d.wind, d.wetness, d.time].allSatisfy(
        \.isFinite), (0...3600).contains(d.time), (-0.1...1.55).contains(d.elevation),
      (0.2...10000).contains(d.distance), (0.5...1.5).contains(d.exposure),
      (0...2).contains(d.wind), (0...1).contains(d.wetness)
    else { throw RuntimeError.message("Invalid study settings") }
    var sky = SkySettings.preset(d.lighting)
    guard let altitude = d.sky["altitude"], let azimuth = d.sky["azimuth"],
      let coverage = d.sky["coverage"], let density = d.sky["density"], let haze = d.sky["haze"],
      let seed = d.sky["cloudSeed"], d.sky.values.allSatisfy(\.isFinite),
      (-12...85).contains(altitude), (-360...360).contains(azimuth), (0...1).contains(coverage),
      (0...3).contains(density), (0.1...3).contains(haze), (0...10000).contains(seed)
    else { throw RuntimeError.message("Invalid saved sky") }
    sky.altitude = altitude
    sky.azimuth = azimuth
    sky.coverage = coverage
    sky.density = density
    sky.haze = haze
    sky.cloudSeed = seed
    try applySource(d.source)
    creatureWorkshop = d.creature ?? CreatureWorkshop()
    importedSourceURL = d.sourcePath.flatMap {
      FileManager.default.fileExists(atPath: $0) ? URL(fileURLWithPath: $0) : nil
    }
    if let look = d.sceneLook { sceneLook = look }
    lighting = d.lighting
    skySettings = sky
    studioRig = d.rig
    studioLayout = d.layout
    if d.skyOnly { studioObject = "sky" }
    inspectionCamera = d.inspectionCamera
    studioViewName = d.viewName ?? "Custom"
    orbit = d.orbit
    orbitElevation = d.elevation
    orbitDistance = d.distance
    exposure = d.exposure
    wind = d.wind
    wetness = d.wetness
    paused = d.paused
    // Restore deterministic fixed-step vegetation state as well as cloud time.
    if let state = d.windState {
      atmosphereWind = state
    } else {
      atmosphereWind = WindSimulation()
      for _ in 0..<Int((d.time * 60).rounded()) { atmosphereWind.step(1 / 60) }
    }
    simulationTime = d.time
    atmosphere.key = ""
    lastTime = CACurrentMediaTime()
  }
  func workshopSnapshot() -> [String: Any] {
    let encoder = JSONEncoder()
    let document =
      (try? JSONSerialization.jsonObject(with: encoder.encode(workshopDocument(includeWind: false))))
      ?? [:]
    return [
      "creature": creatureWorkshop.telemetry,
      "animationDuration": studioSource.animation?.duration(
        studioSource.motion ?? MotionParameters()) ?? 1,
      "blink": studioSource.animation?.blink(
        creatureWorkshop.seconds, studioSource.motion ?? MotionParameters()) ?? 0,
      "authoring": workshopSession.snapshot,
      "document": document,
      "sourcePath": studioSourceURL.path, "extentMetres": studioExtent,
      "heightMetres": studioHeight, "cameraDistanceMetres": orbitDistance * studioUnit,
      "lightPositionMetres": [rigPosition.x, rigPosition.y, rigPosition.z],
      "buildMilliseconds": studioBuildMilliseconds, "parts": studioBatches.map(\.name),
    ]
  }
  static var allShapeRanges: [String: ClosedRange<Float>] {
    Dictionary(
      ProjectContext.current.generators.flatMap { $0.controls }.map { ($0.key, $0.range) },
      uniquingKeysWith: { a, _ in a })
  }
  static var allMotionRanges: [String: ClosedRange<Float>] {
    Dictionary(
      ProjectContext.current.generators.flatMap { $0.animation?.controls ?? [] }.map {
        ($0.key, $0.range)
      }, uniquingKeysWith: { a, _ in a })
  }
  static var workshopCatalog: [String: Any] {
    [
      "anatomyRanges": allShapeRanges.mapValues { [$0.lowerBound, $0.upperBound] },
      "projects": ProjectContext.projects.map { ["id": $0.id, "name": $0.name] },
      "motionModes": ["bind"]
        + ProjectContext.current.generators.flatMap { $0.animation?.clips ?? [] } + ["behavior"],
      "behaviorScenarios": ProjectContext.current.generators.flatMap {
        $0.animation?.scenarios ?? []
      },
      "motionRanges": allMotionRanges.mapValues { [$0.lowerBound, $0.upperBound] },
      "subjects": AssetSource.availableIDs + ["sky"], "rigs": StudioRig.names,
      "views": [
        "front", "quarter", "back", "left", "right", "above", "detail", "base", "crown", "sunward",
        "away", "zenith",
      ] + ProjectContext.current.generators.flatMap { $0.views.map(\.name) },
      "shapeRanges": allShapeRanges.mapValues { [$0.lowerBound, $0.upperBound] },
      "rigRanges": [
        "x": [-4, 4], "y": [0.1, 4], "z": [-4, 4], "intensity": [0, 40], "size": [0.005, 1],
        "warmth": [0, 1], "fill": [0, 0.5],
      ],
      "rigUnits":
        "positions and radius are subject extents; intensity is relative, not lux calibrated",
      "layoutRanges": [
        "scale": [0.25, 4], "roughness": [-1, 1], "metallic": [-1, 1], "tint": [0.3, 1.5],
        "focus": [0, 1],
      ],
      "commands": [
        "subject", "rig", "shape", "layout", "assetLoad", "assetSave", "assetReload", "saveStudy",
        "loadStudy", "reloadAssets", "catalog", "undo", "redo", "checkpoint", "restoreCheckpoint",
        "watchSource", "captureReview", "pinBaseline", "selectBaseline", "styleBoard", "look",
        "publishLook", "motion", "creature", "part", "partEdit", "stimulus", "motionReview",
      ], "sceneLookRanges": SceneLook.ranges.mapValues { [$0.lowerBound, $0.upperBound] },
      "artDirection": SceneLook.url.path,
      "fieldOperations": [
        "sphere", "box", "capsule", "torus", "move", "stretch", "scale", "union", "subtract",
        "blend",
      ], "schema": "docs/SOUNDSTAGE.md",
    ]
  }
}

extension WorkshopRenderer {
  func switchProject(_ id: String) throws {
    guard id != ProjectContext.current.id else { return }
    guard !workshopSession.automating, workshopSession.reviewProcess == nil else {
      throw RuntimeError.message("Finish the current review before switching projects")
    }
    let previous = ProjectContext.current!
    let original = workshopDocument()
    let folder = AssetSource.workspace.appendingPathComponent(".soundstage/projects")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try JSONEncoder().encode(original).write(
      to: folder.appendingPathComponent(previous.id + ".json"), options: .atomic)
    do {
      try ProjectContext.select(id)
      graphics.materialSourceURL = ProjectContext.current.authoring.appendingPathComponent(
        "Materials.metal")
      try graphics.buildPipelines()
      studioCache.removeAll()
      creatureWorkshop = CreatureWorkshop()
      try selectSubject(ProjectContext.current.defaultSubject)
      sceneLook = try SceneLook.load()
      let saved = folder.appendingPathComponent(id + ".json")
      if let data = try? Data(contentsOf: saved) {
        try restoreWorkshop(JSONDecoder().decode(WorkshopDocument.self, from: data))
      }
      workshopSession.pending = nil
      workshopSession.undoStack = []
      workshopSession.redoStack = []
      workshopSession.selectedBaseline = nil
      workshopSession.lastReview = nil
      workshopSession.start()
      onUpdate?()
    } catch {
      ProjectContext.current = previous
      graphics.materialSourceURL = previous.authoring.appendingPathComponent("Materials.metal")
      try? graphics.buildPipelines()
      try? restoreWorkshop(original)
      throw error
    }
  }
}
