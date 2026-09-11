import AppKit
import CoreFoundation
import FieldCore
import FieldEngine
import Foundation

final class GameBridge {
  let session: GameSession, root: URL
  var timer: Timer?
  var onMessage: ((String) -> Void)?
  var pending = Set<String>()
  init(_ session: GameSession, root: URL) throws {
    self.session = session
    self.root = root
    for d in ["inbox", "outbox", "captures"] {
      try FileManager.default.createDirectory(
        at: root.appendingPathComponent(d), withIntermediateDirectories: true)
    }
    for p
      in (try? FileManager.default.contentsOfDirectory(
        at: root.appendingPathComponent("inbox"), includingPropertiesForKeys: nil)) ?? []
    where p.pathExtension == "json" {
      if let data = try? Data(contentsOf: p),
        let c = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
        let id = c["id"] as? String, UUID(uuidString: id) != nil
      {
        respond(id, ["ok": false, "error": "Session restarted before command completed"])
      }
      try? FileManager.default.removeItem(at: p)
    }
    publish()
    timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
      self?.poll()
    }
  }
  func write(_ data: [String: Any], _ url: URL) {
    do {
      try JSONSerialization.data(withJSONObject: data, options: [.prettyPrinted, .sortedKeys])
        .write(to: url, options: .atomic)
    } catch { onMessage?(error.localizedDescription) }
  }
  func publish() {
    var s = session.snapshot()
    s["heartbeat"] = Date().timeIntervalSince1970
    s["protocolVersion"] = 1
    write(s, root.appendingPathComponent("status.json"))
  }
  func respond(_ id: String, _ data: [String: Any]) {
    var d = data
    d["id"] = id
    d["frame"] = session.graphics.frame
    write(d, root.appendingPathComponent("outbox/\(id).json"))
    pending.remove(id)
  }
  func poll() {
    for url
      in ((try? FileManager.default.contentsOfDirectory(
        at: root.appendingPathComponent("inbox"), includingPropertiesForKeys: nil)) ?? []).sorted(
        by: { $0.path < $1.path }) where url.pathExtension == "json"
    {
      do {
        guard
          let c = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any],
          let id = c["id"] as? String, UUID(uuidString: id) != nil
        else { throw RuntimeError.message("Command needs a UUID") }
        try FileManager.default.removeItem(at: url)
        guard pending.insert(id).inserted else { continue }
        do { try execute(c, id: id) } catch {
          respond(id, ["ok": false, "error": error.localizedDescription])
        }
      } catch {
        try? FileManager.default.removeItem(at: url)
        onMessage?(error.localizedDescription)
      }
    }
    publish()
  }
  func execute(_ c: [String: Any], id: String) throws {
    guard let action = c["action"] as? String else { throw RuntimeError.message("Missing action") }
    func number(_ key: String, _ fallback: Float) -> Float {
      (c[key] as? NSNumber)?.floatValue ?? fallback
    }
    for (_, v) in c {
      if let n = v as? NSNumber, !n.doubleValue.isFinite {
        throw RuntimeError.message("Numbers must be finite")
      }
    }
    for key in ["frames", "x", "y", "z", "yaw", "pitch", "exposure", "wetness", "scale"]
    where c[key] != nil {
      guard let n = c[key] as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() else {
        throw RuntimeError.message("\(key) must be a number")
      }
    }
    let g = session.game
    let r = session.graphics
    switch action {
    case "simulationRestore", "simulationRun", "simulationState", "testingQuit":
      guard ProjectContext.testing else {
        throw RuntimeError.message("Simulation harness actions require an isolated testing process")
      }
      if action == "testingQuit" {
        respond(id, ["ok": true])
        DispatchQueue.main.async { NSApp.terminate(nil) }
        return
      }
      session.paused = true
      session.keys = []
      if action == "simulationRestore" {
        guard let encoded = c["snapshot"] as? String, let data = Data(base64Encoded: encoded) else {
          throw RuntimeError.message("Missing simulation snapshot")
        }
        try g.simulation.restore(data)
        session.time = 0
        session.wind = WindSimulation()
      }
      if action == "simulationRun" {
        guard let values = c["actions"] as? [Any], values.count <= 10000 else {
          throw RuntimeError.message("At most 10000 simulation actions")
        }
        let actions = try JSONDecoder().decode(
          [SimulationAction].self, from: JSONSerialization.data(withJSONObject: values))
        for event in actions {
          try g.simulation.apply(event)
          if case .tick = event {
            session.time += 1 / 60
            if !g.lighting.flashlight { session.wind.step(1 / 60) }
          }
        }
      }
      respond(
        id,
        [
          "ok": true, "snapshot": try g.simulation.checkpoint().base64EncodedString(),
          "observations": g.simulation.observations,
        ])
      session.onUpdate?()
      return
    case "status": break
    case "pause", "profiling", "meshlets":
      guard let value = c["value"] as? Bool else { throw RuntimeError.message("Boolean required") }
      if action == "pause" {
        session.paused = value
        session.keys = []
      } else if action == "profiling" {
        r.profiling = value
      } else {
        r.useMeshlets = value
      }
    case "step":
      let n = number("frames", 1)
      guard (1...3600).contains(n), n.rounded() == n else {
        throw RuntimeError.message("Step requires 1...3600 ticks")
      }
      session.paused = true
      for _ in 0..<Int(n) { session.step(1 / 60) }
    case "camera":
      var camera = g.camera
      camera.position = V3(
        number("x", camera.position.x), number("y", camera.position.y),
        number("z", camera.position.z))
      camera.yaw = number("yaw", camera.yaw)
      camera.pitch = number("pitch", camera.pitch)
      guard abs(camera.position.x) <= 1000, abs(camera.position.y) <= 1000,
        abs(camera.position.z) <= 1000, abs(camera.pitch) <= 1.35
      else { throw RuntimeError.message("Camera outside bounds") }
      let old = g.camera
      g.camera = camera
      do { try g.command("camera", c) } catch {
        g.camera = old
        throw error
      }
    case "query":
      let p = V3(number("x", 0), number("y", 0), number("z", 0))
      guard abs(p.x) <= 10000, abs(p.y) <= 10000, abs(p.z) <= 10000 else {
        throw RuntimeError.message("Query outside bounds")
      }
      var result = try g.query(p)
      result["ok"] = true
      respond(id, result)
      return
    case "move":
      let x = number("x", 0)
      let z = number("z", 0)
      guard abs(x) <= 20, abs(z) <= 20 else { throw RuntimeError.message("Move limited to 20 m") }
      g.move(V3(x, 0, z))
    case "interact": onMessage?(try g.interact())
    case "look":
      var edits: [String: Float] = [:]
      for (k, v) in c where !["id", "action"].contains(k) {
        guard let n = v as? NSNumber else { throw RuntimeError.message("Look needs numbers") }
        edits[k] = n.floatValue
      }
      try session.look.set(edits)
    case "lighting":
      guard let name = c["value"] as? String,
        ["morning", "noon", "sunset", "golden", "afterglow", "rain", "overcast"].contains(name),
        !g.lighting.flashlight
      else { throw RuntimeError.message("Outdoor lighting unavailable") }
      g.lighting.preset = name
      g.lighting.sky = SkySettings.preset(name)
      g.lighting.wetness = name == "rain" ? 1 : 0
    case "parameters":
      let e = number("exposure", g.lighting.exposure)
      let wet = number("wetness", g.lighting.wetness)
      let scale = number("scale", 1)
      guard (0.5...1.5).contains(e), (0...1).contains(wet), (0.25...4).contains(scale) else {
        throw RuntimeError.message("Invalid parameter")
      }
      try g.command(action, c)
      g.lighting.exposure = e
      g.lighting.wetness = wet
    case "reloadAssets":
      let look = try SceneLook.load()
      try g.command(action, c)
      session.look = look
      onMessage?("Published assets and project look reloaded")
    case "clearMetrics": r.clearMetrics()
    case "reloadShaders":
      try r.buildPipelines(
        sourceURL: ProjectContext.workspace.appendingPathComponent(
          "Engine/FieldEngine/Resources/Surface.metal"))
    case "capture":
      guard r.captureRequest == nil else { throw RuntimeError.message("Capture pending") }
      let label = String(
        (c["label"] as? String ?? "capture").filter { $0.isLetter || $0.isNumber || $0 == "-" }
          .prefix(64))
      let url = root.appendingPathComponent("captures/\(label)-\(id.prefix(8)).png")
      r.captureRequest = (
        url, session.snapshot(),
        { [weak self] result in
          switch result {
          case .success(let p):
            self?.respond(
              id,
              [
                "ok": true, "path": p.path,
                "metadata": p.deletingPathExtension().appendingPathExtension("json").path,
              ])
          case .failure(let e): self?.respond(id, ["ok": false, "error": e.localizedDescription])
          }
        }
      )
      return
    default:
      try g.command(action, c)
      if action == "expeditionSlot" { onMessage?("Loaded game slot") }
    }
    session.onUpdate?()
    respond(id, ["ok": true, "state": session.snapshot()])
  }
  func stopped() {
    timer?.invalidate()
    write(
      ["stopped": true, "sessionPID": ProcessInfo.processInfo.processIdentifier],
      root.appendingPathComponent("status.json"))
  }
}
