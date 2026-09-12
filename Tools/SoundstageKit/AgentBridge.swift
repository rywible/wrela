import FieldCore
import FieldEngine
import Foundation

/// A local, inspectable file protocol. Commands are atomic JSON files; every ID
/// gets a durable acknowledgement. Captures acknowledge only after GPU readback.
final class AgentBridge {
  let renderer: WorkshopRenderer
  let root: URL
  let inbox: URL
  let outbox: URL
  let captures: URL
  var timer: Timer?
  var onMessage: ((String) -> Void)?
  var activeIDs = Set<String>()
  private let statusWriter=DispatchQueue(label:"dev.wrela.soundstage.status",qos:.utility)
  private var publishing=false
  private var lastPublishedTime = -Double.infinity

  init(renderer: WorkshopRenderer, root: URL) throws {
    self.renderer = renderer
    self.root = root
    inbox = root.appendingPathComponent("inbox")
    outbox = root.appendingPathComponent("outbox")
    captures = root.appendingPathComponent("captures")
    for url in [root, inbox, outbox, captures] {
      try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }
    // Commands belong to a session; an old pending command is rejected explicitly.
    let old =
      (try? FileManager.default.contentsOfDirectory(at: inbox, includingPropertiesForKeys: nil))
      ?? []
    for url in old where url.pathExtension == "json" {
      if let data = try? Data(contentsOf: url),
        let c = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
        let id = c["id"] as? String
      {
        respond(id, ["ok": false, "error": "Session restarted before command completed"])
      }
      try? FileManager.default.removeItem(at: url)
    }
    publish()
    timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
      self?.poll()
    }
  }

  func publish() {
    // Keep the full existing status contract, but never queue an unbounded trail
    // of obsolete heartbeats or serialize a large motion library on the UI thread.
    let now=ProcessInfo.processInfo.systemUptime
    // Commands still poll at 10 Hz and return a fresh snapshot. The complete
    // heartbeat document only needs 2 Hz; it is not a frame-stream transport.
    guard !publishing,now-lastPublishedTime>=0.5 else {return}
    lastPublishedTime=now
    publishing=true
    var s = renderer.snapshot()
    s["heartbeat"] = Date().timeIntervalSince1970
    s["protocolVersion"] = 1
    s["captureDirectory"] = captures.path
    enqueue(s,to:root.appendingPathComponent("status.json")) { [weak self] in self?.publishing=false }
  }
  private func enqueue(_ snapshot:[String:Any],to url:URL,completed:@escaping()->Void) {
    let reportError=onMessage
    statusWriter.async { [weak self] in
      guard self != nil else {return}
      var failure:String?
      do {try JSONSerialization.data(withJSONObject:snapshot,options:[.prettyPrinted,.sortedKeys]).write(to:url,options:.atomic)}
      catch {failure=error.localizedDescription}
      DispatchQueue.main.async {
        if let failure {reportError?("Agent report failed: \(failure)")}
        completed()
      }
    }
  }
  func stopPublishing() {
    timer?.invalidate();timer=nil
    // Drain before the final stopped marker so a late heartbeat cannot revive it.
    statusWriter.sync {}
  }
  func write(_ value: [String: Any], to url: URL) {
    do {
      try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])
        .write(to: url, options: .atomic)
    } catch { onMessage?("Agent report failed: \(error.localizedDescription)") }
  }
  func respond(_ id: String, _ value: [String: Any]) {
    if value["ok"] as? Bool == true {
      renderer.workshopSession.commit()
    } else {
      renderer.workshopSession.pending = nil
    }
    var result = value
    if result["state"] != nil { result["state"] = renderer.snapshot() }
    result["id"] = id
    result["frame"] = renderer.frame
    // Snapshot/commit is synchronous; the acknowledgement becomes visible only
    // after its atomic write. Rendering need not wait for JSON serialization.
    enqueue(result,to:outbox.appendingPathComponent(id + ".json")) { [weak self] in
      self?.activeIDs.remove(id)
    }
  }
  func poll() {
    let files =
      ((try? FileManager.default.contentsOfDirectory(at: inbox, includingPropertiesForKeys: nil))
      ?? []).filter { $0.pathExtension == "json" }.sorted {
        $0.lastPathComponent < $1.lastPathComponent
      }
    for url in files {
      do {
        let data = try Data(contentsOf: url)
        guard let c = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          let id = c["id"] as? String,
          UUID(uuidString: id) != nil
        else { throw RuntimeError.message("Command requires a UUID id") }
        guard !activeIDs.contains(id) else { continue }
        activeIDs.insert(id)
        try FileManager.default.removeItem(at: url)
        do { try execute(c, id: id) } catch {
          respond(id, ["ok": false, "error": error.localizedDescription])
        }
      } catch {
        onMessage?("Invalid agent command: \(error.localizedDescription)")
        try? FileManager.default.removeItem(at: url)
      }
    }
    publish()
  }
  func number(_ c: [String: Any], _ key: String, _ fallback: Float) throws -> Float {
    guard let value = c[key] else { return fallback }
    guard let n = value as? NSNumber, n.floatValue.isFinite else {
      throw RuntimeError.message("\(key) must be finite")
    }
    return n.floatValue
  }
  func execute(_ c: [String: Any], id: String) throws {
    guard let action = c["action"] as? String else { throw RuntimeError.message("Missing action") }
    let authoringKeys: [String: Set<String>] = [
      "creature": ["mode", "scenario", "seconds", "rate", "seed", "follow"],
      "performanceKey": ["joint","channel","seconds","value","remove"],
      "performanceBeat": ["name"],
      "surfaceReview": ["mode", "hiddenParts"],
      "part": ["value", "isolated"], "stimulus": ["x", "z", "running", "visible"],
      "motion": Set((renderer.studioSource.animation?.controls ?? []).map(\.key)),
      "partEdit": [
        "parent", "x", "y", "z", "pivotX", "pivotY", "pivotZ", "pitch", "yaw", "roll", "scale",
        "roughness", "metallic",
      ],
    ]
    if let allowed = authoringKeys[action] {
      guard c.keys.allSatisfy({ allowed.contains($0) || ["id", "action"].contains($0) }) else {
        throw RuntimeError.message("Unknown authoring parameter")
      }
      for key in ["mode", "scenario", "parent", "value"] where c[key] != nil && action != "performanceKey" {
        guard c[key] is String else { throw RuntimeError.message("\(key) requires text") }
      }
      for key in ["follow", "isolated", "running", "visible", "remove"] where c[key] != nil {
        guard c[key] is Bool else { throw RuntimeError.message("\(key) requires a boolean") }
      }
    }
    let edits: Set<String> = [
      "studioObject", "subject", "rig", "shape", "layout", "assetLoad", "assetSave", "assetReload",
      "loadStudy", "lighting", "view", "sky", "reset", "camera", "pause", "step", "parameters",
      "author", "craft", "secondary", "rehearsal", "look", "motion", "creature", "part", "partEdit", "stimulus", "framePart", "performanceKey", "performanceBeat", "rigView", "surfaceReview", "sculptFrameLock", "sculptOverlay", "frameVisible",
    ]
    if edits.contains(action) {
      renderer.workshopSession.commit()
      renderer.workshopSession.begin(action)
    }
    switch action {
    case "inspectSimulation":
      guard ProjectContext.testing, let inspect = ProjectContext.current.inspectSimulation,
        let encoded = c["snapshot"] as? String, let data = Data(base64Encoded: encoded)
      else {
        throw RuntimeError.message(
          "Simulation inspection needs an isolated harness session and supported project")
      }
      let inspection = try inspect(data)
      let original = renderer.workshopDocument()
      do {
        try renderer.selectSubject(inspection.subject)
        var document = renderer.workshopDocument()
        var lab = renderer.creatureWorkshop
        lab.mode = "behavior"
        lab.actor = inspection.actor
        lab.follow = true
        lab.recording = String((c["label"] as? String ?? "Game snapshot").prefix(120))
        lab.seconds = Float(inspection.actor.tick % 3600) / 60
        document.creature = lab
        document.paused = true
        try renderer.restoreWorkshop(document)
      } catch {
        try? renderer.restoreWorkshop(original)
        throw error
      }
    case "craftInspect": respond(id,["ok":true,"craft":try renderer.craftSnapshot()]);return
    case "craft":
      let result=try renderer.editCraft(c);respond(id,["ok":true,"craft":try renderer.craftSnapshot(),"correspondence":result["correspondence"] ?? [],"posedFits":result["posedFits"] ?? [],"sculptFits":result["sculptFits"] ?? []]);return
    case "craftReport":
      let n=try number(c,"samples",25)
      guard n.rounded()==n && (2...121).contains(n) else {throw RigError.invalid}
      if c["part"] != nil && !(c["part"] is String) {throw RigError.invalid}
      let start=try number(c,"start",0)
      let end:Float? = c["end"] == nil ? nil:try number(c,"end",0)
      respond(id,["ok":true,"report":try renderer.craftReport(samples:Int(n),part:c["part"] as? String,start:start,end:end)]);return
    case "authoring": respond(id,["ok":true,"authoring":try renderer.authoringSnapshot()]);return
    case "author":
      try renderer.author(c)
      respond(id,["ok":true,"authoring":try renderer.authoringSnapshot()]);return
    case "secondary": try renderer.editSecondary(c)
    case "secondaryReport":
      let n=try number(c,"samples",25)
      guard n.rounded()==n else {throw RigError.invalid}
      respond(id,["ok":true,"report":try renderer.secondaryReport(samples:Int(n))]);return
    case "surfaceContacts":
      let n=try number(c,"samples",1)
      guard n.rounded()==n else {throw RigError.invalid}
      if c["part"] != nil && !(c["part"] is String) {throw RigError.invalid}
      respond(id,["ok":true,"report":try renderer.surfaceContactReport(samples:Int(n),part:c["part"] as? String)]);return
    case "rehearsal": try renderer.editRehearsal(c)
    case "sculptProbe":
      respond(id,["ok":true,"selection":try renderer.sculptProbe(c)]);return
    case "sculptFrameLock":
      try renderer.lockSculptFrame(c)
    case "sculptOverlay":
      try renderer.editSculptOverlay(c)
    case "frameVisible":
      try CraftError.require(Set(c.keys).isSubset(of:["id","action","expectedRevision","part"])
        && c["expectedRevision"] as? String==renderer.sourceRevision && (c["part"]==nil || c["part"] is String),"frameVisible.revision","Supply current revision and optional part")
      let bounds=try renderer.frameVisibleSurface(part:c["part"] as? String)
      respond(id,["ok":true,"bounds":bounds,"state":renderer.snapshot()]);return
    case "surfaceProbe":
      let p=SIMD3<Float>(try number(c,"x",0),try number(c,"y",0),try number(c,"z",0))
      respond(id,["ok":true,"probe":try renderer.surfaceProbe(part:c["part"] as? String ?? "",point:p,radius:try number(c,"radius",0.25))]);return
    case "poseReport":
      let n=try number(c,"samples",61)
      guard n.rounded()==n,(2...241).contains(n) else {throw RigError.invalid}
      respond(id,["ok":true,"report":try renderer.poseReport(start:try number(c,"start",0),end:try number(c,"end",renderer.performanceDuration),samples:Int(n))]);return
    case "framePart": try renderer.framePart()
    case "surfaceReview":
      if c["hiddenParts"] != nil && !(c["hiddenParts"] is [String]) {
        throw RuntimeError.message("hiddenParts requires an array of exact part names")
      }
      try renderer.editSurfaceReview(mode: c["mode"] as? String, hiddenParts: c["hiddenParts"] as? [String])
    case "rigView":
      guard let value=c["value"] as? Bool else {throw RuntimeError.message("Rig view requires a boolean")}
      renderer.creatureWorkshop.rigView=value
    case "performance": respond(id,["ok":true,"performance":renderer.performanceSnapshot]); return
    case "performanceBeat": try renderer.jumpToBeat(c["name"] as? String ?? "")
    case "performanceKey":
      try renderer.editPerformanceKey(joint:c["joint"] as? String ?? "",channel:c["channel"] as? String ?? "",
        time:try number(c,"seconds",renderer.performanceTime),value:try number(c,"value",0),remove:c["remove"] as? Bool ?? false)
    case "rehearsalReview": try renderer.workshopSession.captureReview(rehearsal:true)
    case "motionReview": try renderer.workshopSession.captureReview(motion: true)
    case "motion", "partEdit":
      var values: [String: Float] = [:]
      for key in c.keys where !["id", "action", "parent"].contains(key) {
        values[key] = try number(c, key, 0)
      }
      if action == "motion" {
        try renderer.editMotion(values)
      } else {
        try renderer.editPart(values, parent: c["parent"] as? String)
      }
    case "part":
      let selected = c["value"] as? String ?? renderer.creatureWorkshop.selected
      try renderer.selectPart(selected == "all" ? nil : selected, isolated: c["isolated"] as? Bool)
    case "creature":
      let seed = try number(c, "seed", Float(renderer.creatureWorkshop.seed))
      guard (0...10000).contains(seed), seed.rounded() == seed else {
        throw RuntimeError.message("Seed must be an integer 0...10000")
      }
      try renderer.editCreatureWorkshop(
        mode: c["mode"] as? String, scenario: c["scenario"] as? String,
        seconds: c["seconds"] == nil ? nil : try number(c, "seconds", 0),
        rate: c["rate"] == nil ? nil : try number(c, "rate", 1),
        seed: c["seed"] == nil ? nil : UInt32(seed), follow: c["follow"] as? Bool)
    case "stimulus":
      var lab = renderer.creatureWorkshop
      var input = lab.input
      input.player.x = try number(c, "x", input.player.x)
      input.player.y = try number(c, "z", input.player.y)
      if let running = c["running"] as? Bool { input.running = running }
      if let visible = c["visible"] as? Bool { input.visible = visible }
      lab.stimulusOverride = input
      try lab.validate(source: renderer.studioSource)
      renderer.creatureWorkshop = lab
    case "undo": try renderer.workshopSession.undo()
    case "redo": try renderer.workshopSession.redo()
    case "watchSource":
      guard let value = c["value"] as? Bool else {
        throw RuntimeError.message("watchSource requires a boolean")
      }
      renderer.workshopSession.watching = value
      renderer.workshopSession.resetWatch()
    case "automation":
      guard let value = c["value"] as? Bool else {
        throw RuntimeError.message("automation requires a boolean")
      }
      renderer.workshopSession.commit()
      renderer.workshopSession.automating = value
      if !value { renderer.workshopSession.resetWatch() }
    case "checkpoint":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Provide a checkpoint name")
      }
      respond(id, ["ok": true, "path": try renderer.workshopSession.checkpoint(name).path])
      return
    case "restoreCheckpoint":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Provide a checkpoint name")
      }
      try renderer.workshopSession.restoreCheckpoint(name)
    case "pinBaseline":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Provide a baseline name")
      }
      respond(id, ["ok": true, "path": try renderer.workshopSession.pinBaseline(name).path])
      return
    case "selectBaseline":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Provide a baseline name")
      }
      try renderer.workshopSession.selectBaseline(name)
    case "captureReview": try renderer.workshopSession.captureReview()
    case "styleBoard": try renderer.workshopSession.captureReview(styleBoard: true)
    case "publishLook": try renderer.sceneLook.publish()
    case "look":
      var values: [String: Float] = [:]
      for key in c.keys where key != "action" && key != "id" { values[key] = try number(c, key, 0) }
      try renderer.sceneLook.set(values)
    case "status": break
    case "scene":
      guard let s = c["value"] as? String, ["garden", "studio"].contains(s) else {
        throw RuntimeError.message("Scene must be garden or studio")
      }
      guard !renderer.isSoundstage || s == "studio" else {
        throw RuntimeError.message(
          "Use the selected game application for playtesting")
      }
      renderer.scene = s
      renderer.keys.removeAll()
    case "studioObject", "subject":
      guard let name = c["value"] as? String else {
        throw RuntimeError.message("Subject requires a name")
      }
      try renderer.selectSubject(name)
    case "catalog":
      respond(id, ["ok": true, "catalog": WorkshopRenderer.workshopCatalog])
      return
    case "rig":
      var rig = renderer.studioRig
      if let name = c["value"] as? String {
        guard StudioRig.names.contains(name) else { throw RuntimeError.message("Unknown rig") }
        rig = StudioRig.preset(name)
      }
      rig.x = try number(c, "x", rig.x)
      rig.y = try number(c, "y", rig.y)
      rig.z = try number(c, "z", rig.z)
      rig.intensity = try number(c, "intensity", rig.intensity)
      rig.size = try number(c, "size", rig.size)
      rig.warmth = try number(c, "warmth", rig.warmth)
      rig.fill = try number(c, "fill", rig.fill)
      try rig.validate()
      renderer.studioRig = rig
    case "shape":
      var values: [String: Float] = [:]
      for key in c.keys where key != "action" && key != "id" {
        guard renderer.studioSource.shapeRanges[key] != nil else {
          throw RuntimeError.message("Unknown shape parameter \(key)")
        }
        values[key] = try number(c, key, 0)
      }
      try renderer.editShape(values)
    case "layout":
      var layout = renderer.studioLayout
      if let name = c["arrangement"] as? String { layout.arrangement = name }
      layout.scale = try number(c, "scale", layout.scale)
      layout.focus = try number(c, "focus", layout.focus)
      layout.roughness = try number(c, "roughness", layout.roughness)
      layout.metallic = try number(c, "metallic", layout.metallic)
      layout.tint = try number(c, "tint", layout.tint)
      for key in ["reference", "ground", "turntable"] where c[key] != nil {
        guard let value = c[key] as? Bool else {
          throw RuntimeError.message("\(key) requires a boolean")
        }
        switch key {
        case "reference": layout.reference = value
        case "ground": layout.ground = value
        default: layout.turntable = value
        }
      }
      try layout.validate()
      let distance = renderer.orbitDistance * renderer.studioUnit
      renderer.studioLayout = layout
      renderer.orbitDistance = renderer.boundedStudioDistance(distance / renderer.studioUnit)
    case "assetLoad":
      guard let path = c["path"] as? String else {
        throw RuntimeError.message("Provide an asset source path")
      }
      let url =
        path.hasPrefix("/")
        ? URL(fileURLWithPath: path) : AssetSource.workspace.appendingPathComponent(path)
      try renderer.applySource(AssetSource.read(url))
      renderer.importedSourceURL = url
      renderer.workshopSession.resetWatch()
      renderer.resetCamera()
    case "assetSave":
      respond(
        id, ["ok": true, "path": try renderer.saveAsset().path, "state": renderer.snapshot()])
      return
    case "assetReload": try renderer.applySource(AssetSource.read(renderer.studioSourceURL))
    case "saveStudy", "loadStudy":
      guard let path = c["path"] as? String, !path.isEmpty else {
        throw RuntimeError.message("Provide a study path")
      }
      let url =
        path.hasPrefix("/")
        ? URL(fileURLWithPath: path)
        : root.appendingPathComponent("studies").appendingPathComponent(path)
      if action == "saveStudy" {
        try renderer.workshopSession.write(renderer.workshopDocument(), to: url)
      } else {
        try renderer.restoreWorkshop(WorkshopDocument.read(url))
      }
      respond(id, ["ok": true, "path": url.path, "state": renderer.snapshot()])
      return
    case "project":
      guard let name = c["value"] as? String else {
        throw RuntimeError.message("Project id required")
      }
      try renderer.switchProject(name)
    case "reloadAssets": try renderer.applySource(AssetSource.load(renderer.studioSource.id))
    case "profiling":
      guard let value = c["value"] as? Bool else {
        throw RuntimeError.message("Profiling requires a boolean")
      }
      renderer.profiling = value
      renderer.clearMetrics()
    case "meshlets":
      guard let value = c["value"] as? Bool else {
        throw RuntimeError.message("Meshlets requires a boolean")
      }
      renderer.useMeshlets = value
      renderer.clearMetrics()
    case "lighting":
      guard let s = c["value"] as? String,
        ["morning", "noon", "golden", "sunset", "afterglow", "overcast", "rain", "indoor"].contains(
          s)
      else { throw RuntimeError.message("Unknown lighting preset") }
      renderer.lighting = s
    case "view":
      guard let name = c["value"] as? String,
        [
          "front", "quarter", "back", "left", "right", "above", "detail", "base", "crown",
          "sunward", "away", "zenith",
        ].contains(name)
          || (renderer.studioSource.definition?.views.contains { $0.name == name } ?? false)
      else { throw RuntimeError.message("Unknown view") }
      renderer.setStudyView(name)
    case "sky":
      var s = renderer.skySettings
      s.altitude = try number(c, "altitude", s.altitude)
      s.azimuth = try number(c, "azimuth", s.azimuth)
      s.coverage = try number(c, "coverage", s.coverage)
      s.density = try number(c, "density", s.density)
      s.haze = try number(c, "haze", s.haze)
      s.cloudSeed = try number(c, "cloudSeed", s.cloudSeed)
      guard (-12...85).contains(s.altitude), (-360...360).contains(s.azimuth),
        (0...1).contains(s.coverage), (0...3).contains(s.density), (0.1...3).contains(s.haze),
        (0...10000).contains(s.cloudSeed)
      else {
        throw RuntimeError.message(
          "Sky range: altitude -12...85°, azimuth ±360°, coverage 0...1, density 0...3, haze .1...3, cloudSeed 0...10000"
        )
      }
      renderer.skySettings = s
    case "reset": renderer.resetCamera()
    case "camera":
      let yaw = try number(c, "yaw", renderer.yaw)
      let pitch = try number(c, "pitch", renderer.pitch)
      let x = try number(c, "x", renderer.camera.x)
      let z = try number(c, "z", renderer.camera.z)
      guard abs(x) <= 130, z >= -135, z <= 120 else {
        throw RuntimeError.message("Camera is outside the garden")
      }
      let orbit = try number(c, "orbit", renderer.orbit)
      let elevation = try number(c, "elevation", renderer.orbitElevation)
      let distance =
        try number(
          c, "metres", try number(c, "distance", renderer.orbitDistance) * renderer.studioUnit)
        / renderer.studioUnit
      renderer.yaw = yaw
      renderer.pitch = clamp(pitch, -1.35, 1.35)
      renderer.camera = V3(x, 1.72, z)
      if c["orbit"] != nil || c["elevation"] != nil { renderer.studioViewName = "Custom" }
      renderer.orbit = orbit
      renderer.orbitElevation = clamp(elevation, -0.1, 1.55)
      renderer.orbitDistance = renderer.boundedStudioDistance(distance)
    case "move":
      let x = try number(c, "x", 0)
      let z = try number(c, "z", 0)
      guard abs(x) <= 25, abs(z) <= 25 else {
        throw RuntimeError.message("Movement per command is limited to 25 metres per axis")
      }
      renderer.move(local: V3(x, 0, z))
    case "pause":
      guard let paused = c["value"] as? Bool else {
        throw RuntimeError.message("Pause requires a boolean value")
      }
      renderer.paused = paused
      renderer.keys.removeAll()
    case "step":
      guard renderer.paused else { throw RuntimeError.message("Pause before stepping") }
      let frames = try number(c, "frames", 1)
      guard (1...600).contains(frames), frames.rounded(.towardZero) == frames else {
        throw RuntimeError.message("Step requires an integer from 1...600 frames")
      }
      let count = Int(frames)
      for _ in 0..<count { renderer.step(1 / 60) }
    case "parameters":
      let exposure = try number(c, "exposure", renderer.exposure)
      let wind = try number(c, "wind", renderer.wind)
      let scale = try number(c, "scale", renderer.studioLayout.scale)
      guard (0.5...1.5).contains(exposure), (0...2).contains(wind), (0.6...1.7).contains(scale)
      else {
        throw RuntimeError.message(
          "Parameter outside supported range: exposure .5...1.5, wind 0...2, scale .6...1.7")
      }
      let wetness = try number(c, "wetness", renderer.wetness)
      guard c["gi"] == nil else {
        throw RuntimeError.message(
          "The old GI switch was removed; lighting uses atmosphere irradiance")
      }
      guard (0...1).contains(wetness) else {
        throw RuntimeError.message("Wetness must be between 0 and 1")
      }
      renderer.wetness = wetness
      renderer.exposure = exposure
      renderer.wind = wind
      if c["scale"] != nil {
        renderer.setStudioScale(scale)
      }
    case "clearMetrics": renderer.clearMetrics()
    case "reloadShaders":
      try renderer.buildPipelines(
        sourceURL: root.deletingLastPathComponent().appendingPathComponent(
          "Engine/FieldEngine/Resources/Surface.metal"))
      renderer.clearMetrics()
      onMessage?("Shaders reloaded. Camera and world preserved.")
    case "verifySky":
      guard renderer.paused else {
        throw RuntimeError.message("Pause before verifying sky traversal")
      }
      respond(
        id, ["ok": true, "verification": try renderer.atmosphere.verifyEmptySpace(renderer.queue)])
      return
    case "verifyFields":
      let report = try renderer.verifyFields()
      respond(id, ["ok": true, "verification": report])
      return
    case "capture":
      guard renderer.captureRequest == nil else {
        throw RuntimeError.message("A capture is already pending; await its acknowledgement")
      }
      let label = (c["label"] as? String ?? "capture").filter {
        $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_"
      }.prefix(64)
      let url = captures.appendingPathComponent("\(label)-\(id.prefix(8)).png")
      // Freeze metadata on the actual rendered capture frame in the renderer.
      renderer.captureRequest = (
        url, renderer.snapshot(),
        { [weak self] result in
          switch result {
          case .success(let path):
            self?.respond(
              id,
              [
                "ok": true, "path": path.path,
                "metadata": path.deletingPathExtension().appendingPathExtension("json").path,
              ])
            self?.onMessage?("Captured \(path.lastPathComponent)")
          case .failure(let error):
            self?.respond(id, ["ok": false, "error": error.localizedDescription])
          }
        }
      )
      return
    default: throw RuntimeError.message("Unknown action: \(action)")
    }
    renderer.onUpdate?()
    respond(id, ["ok": true, "state": renderer.snapshot()])
  }
  func capture() {
    let id = UUID().uuidString
    do {
      try execute(["action": "capture", "label": renderer.scene + "-" + renderer.lighting], id: id)
    } catch { onMessage?(error.localizedDescription) }
  }
}
