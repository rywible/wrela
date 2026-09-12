import AppKit
import CryptoKit
import FieldEngine

extension WorkshopDocument {
  // A full supported 4 MiB source plus the fixed-step simulation, camera and
  // review state must round-trip through every study entry point.
  static let maximumBytes = AssetSource.maximumSourceBytes * 2

  static func readData(_ url: URL) throws -> Data {
    let file = try FileHandle(forReadingFrom: url)
    defer { try? file.close() }
    var data = Data()
    while data.count < maximumBytes {
      let block = try file.read(upToCount: min(1_048_576, maximumBytes - data.count)) ?? Data()
      if block.isEmpty { return data }
      data.append(block)
    }
    throw RuntimeError.message("Study must be smaller than 8 MiB, including its source and replay state")
  }
  static func read(_ url: URL) throws -> WorkshopDocument {
    try JSONDecoder().decode(WorkshopDocument.self, from: readData(url))
  }
  func encoded() throws -> Data {
    let encoder = JSONEncoder()
    // Avoid multiplying rich motion/anatomy sources with indentation. Old
    // pretty-printed studies are still accepted by the same bounded reader.
    encoder.outputFormatting = [.sortedKeys]
    let data = try encoder.encode(self)
    guard data.count < Self.maximumBytes else {
      throw RuntimeError.message("Study must be smaller than 8 MiB, including its source and replay state")
    }
    return data
  }
  func write(to url: URL) throws {
    let data = try encoded()
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try data.write(to: url, options: .atomic)
  }
}

/// A study-owned rendering lens. It never changes the authored source or bindings.
struct SurfaceReview: Codable {
  var mode = "material"
  var hiddenParts: [String] = []

  func validate(parts: [String]? = nil) throws {
    guard ["material", "clay"].contains(mode) else {
      throw RuntimeError.message("Surface review mode must be material or clay")
    }
    guard Set(hiddenParts).count == hiddenParts.count else {
      throw RuntimeError.message("Each hidden part must be named once")
    }
    guard let parts else { return }
    let unknown = hiddenParts.filter { !parts.contains($0) }
    guard unknown.isEmpty else {
      throw RuntimeError.message("Unknown hidden parts: \(unknown.joined(separator: ", ")). Use status.workshop.parts or the Parts inspector for exact names.")
    }
  }
}

extension WorkshopRenderer {
  func editSurfaceReview(mode: String? = nil, hiddenParts: [String]? = nil) throws {
    var candidate = surfaceReview
    if let mode { candidate.mode = mode }
    if let hiddenParts { candidate.hiddenParts = hiddenParts }
    try candidate.validate(parts: studioBatches.map(\.name))
    candidate.hiddenParts.sort()
    surfaceReview = candidate
  }
}

/// Authoring state is separate from renderer caches and from published asset files.
final class WorkshopSession {
  unowned let renderer: WorkshopRenderer
  struct Edit {
    var name: String
    var before: WorkshopDocument
    var after: WorkshopDocument
  }
  var undoStack: [Edit] = [], redoStack: [Edit] = []
  var pending: (String, WorkshopDocument)?
  var automating = false
  var watching = true
  var watchMessage = "Watching source"
  var watchURL: URL?
  var observed: Data?
  var applied: Data?
  var changedAt = Date.distantPast
  var timer: Timer?
  var onMessage: ((String) -> Void)?
  var lastReview: URL?
  var selectedBaseline: URL?
  var reviewProcess: Process?
  var reviewMessage = ""
  var reviewOriginal: WorkshopDocument?
  var root: URL {
    AssetSource.workspace.appendingPathComponent(
      ".soundstage/projects/" + ProjectContext.current.id)
  }
  init(_ renderer: WorkshopRenderer) { self.renderer = renderer }
  func start() {
    timer?.invalidate()
    try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    // Preserve pre-project Sanctuary checkpoints without overwriting newer work.
    if ProjectContext.current.id == "sanctuary" {
      let legacy = AssetSource.workspace.appendingPathComponent(".soundstage")
      for folder in ["checkpoints", "baselines"] {
        let destination = root.appendingPathComponent(folder)
        try? FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        for file
          in (try? FileManager.default.contentsOfDirectory(
            at: legacy.appendingPathComponent(folder), includingPropertiesForKeys: nil)) ?? []
        where file.pathExtension == "json" {
          let target = destination.appendingPathComponent(file.lastPathComponent)
          if !FileManager.default.fileExists(atPath: target.path) {
            try? FileManager.default.copyItem(at: file, to: target)
          }
        }
      }
    }
    if let data = try? Data(contentsOf: root.appendingPathComponent("review-state.json")),
      let paths = try? JSONDecoder().decode([String: String].self, from: data)
    {
      lastReview = paths["last"].map { URL(fileURLWithPath: $0) }
      selectedBaseline = paths["baseline"].map { URL(fileURLWithPath: $0) }
    }
    resetWatch()
    timer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in
      self?.pollSource()
    }
  }
  func bytes(_ doc: WorkshopDocument) -> Data {
    let e = JSONEncoder()
    e.outputFormatting = [.sortedKeys]
    return (try? e.encode(doc)) ?? Data()
  }
  func begin(_ name: String) {
    guard renderer.isSoundstage, !automating, pending == nil else { return }
    pending = (name, renderer.workshopDocument())
  }
  func commit() {
    guard let (name, before) = pending else { return }
    pending = nil
    let after = renderer.workshopDocument()
    guard bytes(before) != bytes(after) else { return }
    undoStack.append(Edit(name: name, before: before, after: after))
    if undoStack.count > 50 { undoStack.removeFirst() }
    redoStack.removeAll()
  }
  func edit(_ name: String, _ action: () throws -> Void) throws {
    guard !automating, reviewProcess == nil else {
      throw RuntimeError.message("A review is capturing. Wait for it to finish before editing.")
    }
    commit()
    begin(name)
    do {
      try action()
      commit()
    } catch {
      pending = nil
      throw error
    }
  }
  func undo() throws {
    guard !automating else { throw RuntimeError.message("Wait for the review to finish") }
    commit()
    guard let edit = undoStack.last else { return }
    try renderer.restoreWorkshop(edit.before)
    undoStack.removeLast()
    redoStack.append(edit)
    resetWatch()
    onMessage?("Undid \(edit.name)")
  }
  func redo() throws {
    guard !automating else { throw RuntimeError.message("Wait for the review to finish") }
    guard let edit = redoStack.last else { return }
    try renderer.restoreWorkshop(edit.after)
    redoStack.removeLast()
    undoStack.append(edit)
    resetWatch()
    onMessage?("Redid \(edit.name)")
  }
  func resetWatch() {
    watchURL = renderer.studioSourceURL
    observed = try? Data(contentsOf: renderer.studioSourceURL)
    applied = observed
    watchMessage =
      watching ? "Watching \(renderer.studioSourceURL.lastPathComponent)" : "Auto reload off"
  }
  func pollSource() {
    guard renderer.isSoundstage, watching, !automating, reviewProcess == nil, pending == nil else {
      return
    }
    let url = renderer.studioSourceURL
    if url != watchURL {
      resetWatch()
      return
    }
    let data = try? Data(contentsOf: url)
    if data != observed {
      observed = data
      changedAt = Date()
      return
    }
    guard data != applied, Date().timeIntervalSince(changedAt) > 0.45 else { return }
    applied = data  // Attempt once per stable file revision, including malformed revisions.
    do {
      guard let data, data.count < AssetSource.maximumSourceBytes else {
        throw RuntimeError.message("Source is missing or exceeds 4 MiB")
      }
      let source = try AssetSource.decode(data)
      let skyOnly = renderer.studioObject == "sky"
      try edit("Source reload") {
        try renderer.applySource(source)
        if renderer.studioSourceURL != url { renderer.importedSourceURL = url }
        if skyOnly { renderer.studioObject = "sky" }
      }
      watchMessage = "Reloaded \(url.lastPathComponent)"
      onMessage?(watchMessage)
    } catch {
      watchMessage = "Source error · keeping last valid object"
      onMessage?("\(watchMessage): \(error.localizedDescription)")
    }
    renderer.onUpdate?()
  }
  func write(_ doc: WorkshopDocument, to url: URL) throws {
    try doc.write(to: url)
  }
  func namedFiles(_ folder: String) -> [(String, URL)] {
    let dir = root.appendingPathComponent(folder)
    return
      ((try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil))
      ?? []).filter { $0.pathExtension == "json" }.sorted {
        $0.lastPathComponent < $1.lastPathComponent
      }.map { ($0.deletingPathExtension().lastPathComponent, $0) }
  }
  func namedURL(_ name: String, _ folder: String) throws -> URL {
    let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !clean.isEmpty, clean.count <= 64, !clean.contains("/"), !clean.contains("\\"),
      clean != ".", clean != ".."
    else { throw RuntimeError.message("Use a name of 1–64 characters without slashes") }
    let url = root.appendingPathComponent(folder).appendingPathComponent(clean + ".json")
    guard !FileManager.default.fileExists(atPath: url.path) else {
      throw RuntimeError.message("That name already exists. Choose a new checkpoint name.")
    }
    return url
  }
  func checkpoint(_ name: String) throws -> URL {
    let url = try namedURL(name, "checkpoints")
    try write(renderer.workshopDocument(), to: url)
    onMessage?("Saved checkpoint · \(name)")
    return url
  }
  func restoreCheckpoint(_ name: String) throws {
    guard let url = namedFiles("checkpoints").first(where: { $0.0 == name })?.1 else {
      throw RuntimeError.message("Unknown checkpoint")
    }
    let doc = try WorkshopDocument.read(url)
    try edit("Restore \(name)") { try renderer.restoreWorkshop(doc) }
    resetWatch()
  }
  func saveReviewState() {
    var state: [String: String] = [:]
    state["last"] = lastReview?.path
    state["baseline"] = selectedBaseline?.path
    try? JSONEncoder().encode(state).write(
      to: root.appendingPathComponent("review-state.json"), options: .atomic)
  }
  func pinBaseline(_ name: String) throws -> URL {
    guard let lastReview, FileManager.default.fileExists(atPath: lastReview.path) else {
      throw RuntimeError.message("Capture a review first")
    }
    let url = try namedURL(name, "baselines")
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try JSONEncoder().encode([
      "report": lastReview.deletingLastPathComponent().appendingPathComponent("report.json").path
    ]).write(to: url, options: .atomic)
    selectedBaseline = lastReview.deletingLastPathComponent().appendingPathComponent("report.json")
    saveReviewState()
    onMessage?("Pinned baseline · \(name)")
    return url
  }
  func selectBaseline(_ name: String) throws {
    if name == "None" {
      selectedBaseline = nil
      saveReviewState()
      return
    }
    guard let url = namedFiles("baselines").first(where: { $0.0 == name })?.1 else {
      throw RuntimeError.message("Unknown baseline")
    }
    let value = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: url))
    guard let path = value["report"], FileManager.default.fileExists(atPath: path) else {
      throw RuntimeError.message("Baseline report is missing")
    }
    selectedBaseline = URL(fileURLWithPath: path)
    saveReviewState()
  }
  func captureReview(styleBoard: Bool = false, motion: Bool = false, rehearsal: Bool = false) throws {
    guard reviewProcess == nil, !automating else {
      throw RuntimeError.message("A review is already running")
    }
    commit()
    reviewOriginal = renderer.workshopDocument()
    let process = Process()
    let log = root.appendingPathComponent("review-job.log")
    FileManager.default.createFile(atPath: log.path, contents: nil)
    let handle = try FileHandle(forWritingTo: log)
    process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
    var environment = ProcessInfo.processInfo.environment
    environment["WRELA_APP_BUNDLE"] = Bundle.main.bundleURL.path
    process.environment = environment
    var arguments = [
      AssetSource.workspace.appendingPathComponent("scripts/workshop-study").path,
      styleBoard ? "--style-board" : "--current", "--label",
      styleBoard ? "style-board" : "authoring",
    ]
    if motion {
      arguments.removeAll { $0 == "--current" }
      guard renderer.studioSource.animation != nil, renderer.creatureWorkshop.mode != "bind" else {
        throw RuntimeError.message("Choose a motion clip or behavior scenario before recording")
      }
      arguments += [
        "--creature-sequence", "--motion", "12", "--step",
        renderer.creatureWorkshop.mode == "behavior" ? "60" : "5",
        "--views", "front,left", "--lights", "", "--rigs", renderer.studioRig.name,
      ]
    }
    if !styleBoard, !motion, let selectedBaseline {
      if let data = try? Data(contentsOf: selectedBaseline),
        let report = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
        let records = report["records"] as? [[String: Any]],
        let last = records.last?["variant"] as? String,
        records.filter({ $0["variant"] as? String == last }).count > 1
      {
        arguments.removeAll { $0 == "--current" }
      }
      arguments += ["--reference", selectedBaseline.path]
    }
    if rehearsal {
      guard renderer.studioSource.animation != nil,renderer.creatureWorkshop.mode != "bind" else {throw RuntimeError.message("Choose a motion clip before review")}
      let duration=renderer.performanceDuration
      let times=(0..<9).map {String(Float($0)*duration/8)}.joined(separator:",")
      arguments=[AssetSource.workspace.appendingPathComponent("scripts/creature-review").path,"--label","rehearsal","--times",times]
    }
    process.arguments = arguments
    process.standardOutput = handle
    process.standardError = handle
    process.terminationHandler = { [weak self] p in
      try? handle.close()
      DispatchQueue.main.async {
        guard let self else { return }
        self.reviewProcess = nil
        if p.terminationStatus != 0, let original = self.reviewOriginal {
          try? self.renderer.restoreWorkshop(original)
        }
        self.reviewOriginal = nil
        self.automating = false
        let output = (try? String(contentsOf: log, encoding: .utf8)) ?? ""
        if p.terminationStatus == 0, let line = output.split(separator: "\n").last,
          let data = String(line).data(using: .utf8),
          let result = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
          let path = result["viewer"] as? String
        {
          self.lastReview = URL(fileURLWithPath: path)
          self.saveReviewState()
          self.reviewMessage = "Review ready"
        } else {
          self.reviewMessage = "Review failed · \(output.suffix(400))"
        }
        self.onMessage?(self.reviewMessage)
        self.renderer.onUpdate?()
      }
    }
    try process.run()
    reviewProcess = process
    reviewMessage = "Capturing review…"
    onMessage?(reviewMessage)
  }
  var snapshot: [String: Any] {
    [
      "undoDepth": undoStack.count, "redoDepth": redoStack.count, "canUndo": !undoStack.isEmpty,
      "canRedo": !redoStack.isEmpty, "undo": undoStack.last?.name ?? "",
      "redo": redoStack.last?.name ?? "", "watching": watching, "watchStatus": watchMessage,
      "automating": automating, "checkpoints": namedFiles("checkpoints").map(\.0),
      "baselines": namedFiles("baselines").map(\.0), "lastReview": lastReview?.path ?? "",
      "baseline": selectedBaseline?.path ?? "", "reviewRunning": reviewProcess != nil,
      "reviewStatus": reviewMessage,
    ]
  }
}
