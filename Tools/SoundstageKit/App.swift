import AppKit
import FieldCore
import FieldEngine
import MetalKit
import WebKit
import simd

final class GameView: MTKView {
  weak var controller: AppController?
  private var lastDragPoint: NSPoint?
  private var zoomEnd: Timer?
  override var acceptsFirstResponder: Bool { true }
  override func mouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
    guard controller?.renderer?.workshopSession.automating != true else { return }
    controller?.renderer?.workshopSession.begin("Orbit camera")
    lastDragPoint = event.locationInWindow
  }
  override func rightMouseDown(with event: NSEvent) { mouseDown(with: event) }
  override func mouseDragged(with event: NSEvent) {
    let p = event.locationInWindow
    if let previous = lastDragPoint {
      controller?.renderer?.look(dx: Float(p.x - previous.x), dy: Float(previous.y - p.y))
    }
    lastDragPoint = p
  }
  override func mouseUp(with event: NSEvent) {
    lastDragPoint = nil
    controller?.renderer?.workshopSession.commit()
  }
  override func rightMouseUp(with event: NSEvent) { mouseUp(with: event) }
  override func rightMouseDragged(with event: NSEvent) { mouseDragged(with: event) }
  override func scrollWheel(with event: NSEvent) {
    if let r = controller?.renderer, r.scene == "studio", !r.workshopSession.automating {
      r.workshopSession.begin("Zoom camera")
      r.orbitDistance = r.boundedStudioDistance(
        r.orbitDistance * exp(Float(event.scrollingDeltaY) * 0.025))
      zoomEnd?.invalidate()
      zoomEnd = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
        self?.controller?.renderer?.workshopSession.commit()
      }
    }
  }
  override func keyDown(with event: NSEvent) {
    guard let c = controller, let r = c.renderer else { return }
    if r.workshopSession.automating { return }
    if [48, 15, 35, 18, 19, 20, 123, 124, 125, 126].contains(event.keyCode) {
      r.workshopSession.commit()
      r.workshopSession.begin("Keyboard edit")
    }
    defer { r.workshopSession.commit() }
    switch event.keyCode {
    case 48: c.toggleScene()
    case 3:

      do { try r.workshopSession.captureReview() } catch {
        c.messageLabel.stringValue = error.localizedDescription
      }

    case 15: r.resetCamera()
    case 35:
      r.paused.toggle()
      r.keys.removeAll()
    case 18: r.lighting = "morning"
    case 19: r.lighting = "sunset"
    case 20: r.lighting = "overcast"
    case 4: c.toggleNotes()
    case 123: r.look(dx: -35, dy: 0)
    case 124: r.look(dx: 35, dy: 0)
    case 125: r.look(dx: 0, dy: 25)
    case 126: r.look(dx: 0, dy: -25)
    case 53:
      r.keys.removeAll()
      window?.makeFirstResponder(self)
    default:
      // A computer-use key tap can begin and end between two fixed ticks.
      // Give its initial press a small step; held keys still move at dt speed.
      if !event.isARepeat && !r.keys.contains(event.keyCode) && !r.paused {
        let tap: [UInt16: V3] = [
          13: V3(0, 0, 0.12), 1: V3(0, 0, -0.12), 0: V3(-0.12, 0, 0), 2: V3(0.12, 0, 0),
        ]
        if let movement = tap[event.keyCode] { r.move(local: movement) }
      }
      r.keys.insert(event.keyCode)
    }
    c.updateHUD()
  }
  override func keyUp(with event: NSEvent) { controller?.renderer?.keys.remove(event.keyCode) }
  override func flagsChanged(with event: NSEvent) {
    if event.modifierFlags.contains(.shift) {
      controller?.renderer?.keys.insert(56)
    } else {
      controller?.renderer?.keys.remove(56)
    }
  }
}

final class ActionButton: NSButton {
  var actionBlock: (() -> Void)?
  init(_ title: String, action: @escaping () -> Void) {
    super.init(frame: .zero)
    self.title = title
    actionBlock = action
    target = self
    self.action = #selector(invoke)
    bezelStyle = .rounded
    font = NSFont.systemFont(ofSize: 12, weight: .medium)
    controlSize = .regular
    setAccessibilityLabel(title)
  }
  required init?(coder: NSCoder) { fatalError() }
  @objc func invoke() { actionBlock?() }
}

final class AppController: NSObject, NSApplicationDelegate, NSWindowDelegate {
  var window: NSWindow!
  var gameView: GameView!
  var renderer: WorkshopRenderer?
  var bridge: AgentBridge?
  var messageLabel: NSTextField!
  var notes: NSView!
  var rootURL: URL!
  var workshopPanel: WorkshopPanel?
  var inspectorWidth: NSLayoutConstraint?
  var reviewWindow: NSWindow?
  var pendingReviewURLs: [URL] = []
  var lookTimer: Timer?
  var lookFileData: Data?

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.regular)
    let menu = NSMenu()
    let appItem = NSMenuItem()
    menu.addItem(appItem)
    let appMenu = NSMenu()
    appMenu.addItem(
      withTitle: "Quit Soundstage", action: #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q")
    appItem.submenu = appMenu
    NSApp.mainMenu = menu
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1200, height: 760),
      styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
    )
    window.title = "Soundstage"
    window.minSize = NSSize(width: 1080, height: 680)
    window.center()
    window.delegate = self
    gameView = GameView(frame: window.contentView!.bounds)
    gameView.controller = self
    gameView.autoresizingMask = [.width, .height]
    gameView.setAccessibilityLabel(
      "Field workshop. Drag to orbit or look. Tab switches sky and object.")

    let root = NSView(frame: gameView.frame)
    root.wantsLayer = true
    root.layer?.backgroundColor = NSColor(calibratedWhite: 0.075, alpha: 1).cgColor
    window.contentView = root
    let editItem = NSMenuItem()
    menu.addItem(editItem)
    let edit = NSMenu(title: "Edit")
    editItem.submenu = edit
    for (title, selector, key) in [
      ("Undo", #selector(undoWorkshop), "z"), ("Redo", #selector(redoWorkshop), "Z"),
    ] {
      let item = edit.addItem(withTitle: title, action: selector, keyEquivalent: key)
      item.target = self
    }
    edit.addItem(.separator())
    for (title, selector, key) in [
      ("Cut", #selector(NSText.cut(_:)), "x"), ("Copy", #selector(NSText.copy(_:)), "c"),
      ("Paste", #selector(NSText.paste(_:)), "v"),
      ("Select All", #selector(NSText.selectAll(_:)), "a"),
    ] { edit.addItem(withTitle: title, action: selector, keyEquivalent: key) }

    buildHUD()
    window.makeKeyAndOrderFront(nil)
    window.makeFirstResponder(gameView)
    NSApp.activate(ignoringOtherApps: true)
    rootURL = URL(
      fileURLWithPath: ProcessInfo.processInfo.environment["SANCTUARY_WORKSPACE"] ?? Bundle.main
        .object(forInfoDictionaryKey: "SanctuaryWorkspace") as? String
        ?? FileManager.default.currentDirectoryPath
    ).appendingPathComponent(".soundstage")
    do {
      let r = try WorkshopRenderer(
        view: gameView)
      renderer = r
      bridge = try AgentBridge(renderer: r, root: rootURL)
      bridge?.onMessage = { [weak self] message in self?.messageLabel.stringValue = message }
      r.onUpdate = { [weak self] in self?.updateHUD() }

      let saved = rootURL.appendingPathComponent("last-study.json")
      if FileManager.default.fileExists(atPath: saved.path) {
        do {
          try r.restoreWorkshop(
            JSONDecoder().decode(WorkshopDocument.self, from: Data(contentsOf: saved)))
          messageLabel.stringValue = "Restored your last workshop session."
        } catch {
          messageLabel.stringValue =
            "Last study could not be restored: \(error.localizedDescription)"
        }
      }

      r.workshopSession.onMessage = bridge?.onMessage
      r.workshopSession.start()

      lookFileData = try? Data(contentsOf: SceneLook.url)
      lookTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
        self?.reloadProjectLook()
      }
      updateHUD()
      for url in pendingReviewURLs { openReview(url) }
      pendingReviewURLs.removeAll()
    } catch {
      messageLabel.stringValue = "Unable to start: \(error.localizedDescription)"
      fputs("Soundstage startup failed: \(error)\n", stderr)
      let alert = NSAlert()
      alert.messageText = "Soundstage could not start"
      alert.informativeText = error.localizedDescription
      alert.runModal()
      NSApp.terminate(nil)
    }
  }

  func label(
    _ string: String, size: CGFloat, weight: NSFont.Weight = .regular, color: NSColor = .white
  ) -> NSTextField {
    let l = NSTextField(labelWithString: string)
    l.font = NSFont.systemFont(ofSize: size, weight: weight)
    l.textColor = color
    l.translatesAutoresizingMaskIntoConstraints = false
    return l
  }
  func panel() -> NSVisualEffectView {
    let p = NSVisualEffectView()
    p.material = .hudWindow
    p.blendingMode = .withinWindow
    p.state = .active
    p.wantsLayer = true
    p.layer?.cornerRadius = 15
    p.layer?.masksToBounds = true
    p.translatesAutoresizingMaskIntoConstraints = false
    p.appearance = NSAppearance(named: .darkAqua)
    return p
  }
  func buildHUD() { buildWorkshopHUD() }
  func button(_ title: String, action: @escaping () -> Void) -> ActionButton {
    ActionButton(title) { [weak self] in
      action()
      self?.updateHUD()
      self?.window.makeFirstResponder(self?.gameView)
    }
  }
  func toggleScene() {
    guard let r = renderer else { return }
    try? r.selectSubject(r.studioObject == "sky" ? r.studioSource.id : "sky")
    updateHUD()
  }
  func toggleNotes() {
    notes.isHidden.toggle()
    inspectorWidth?.constant = notes.isHidden ? 0 : 310
  }
  func setCamera(_ name: String) {
    renderer?.setStudyView(name)
    updateHUD()
  }
  func updateHUD() { workshopPanel?.refresh() }
  func reloadProjectLook() {
    guard let r = renderer, !r.workshopSession.automating, r.workshopSession.reviewProcess == nil,
      r.workshopSession.pending == nil
    else { return }
    let data = try? Data(contentsOf: SceneLook.url)
    guard data != lookFileData else { return }
    lookFileData = data
    do {
      let look = try SceneLook.load()
      guard look != r.sceneLook else { return }

      try r.workshopSession.edit("Project look reload") { r.sceneLook = look }

      messageLabel.stringValue = "Shared art direction reloaded"
      updateHUD()
    } catch {
      messageLabel.stringValue =
        "Art direction error · keeping last valid look: \(error.localizedDescription)"
    }
  }
  @objc func undoWorkshop() {
    if let editor = window.firstResponder as? NSTextView, editor.isFieldEditor {
      editor.undoManager?.undo()
      return
    }
    workshopPanel?.sessionAction { try $0.undo() }
  }
  @objc func redoWorkshop() {
    if let editor = window.firstResponder as? NSTextView, editor.isFieldEditor {
      editor.undoManager?.redo()
      return
    }
    workshopPanel?.sessionAction { try $0.redo() }
  }
  func application(_ application: NSApplication, open urls: [URL]) {
    guard renderer != nil else {
      pendingReviewURLs += urls
      return
    }
    for url in urls { openReview(url) }
  }
  func openReview(_ url: URL) {
    guard url.pathExtension == "html" else { return }
    if reviewWindow == nil {
      reviewWindow = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 1100, height: 720),
        styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered,
        defer: false)
      reviewWindow?.isReleasedWhenClosed = false
      reviewWindow?.center()
    }
    let web = WKWebView(frame: reviewWindow!.contentView!.bounds)
    web.autoresizingMask = [.width, .height]
    reviewWindow!.title = "Soundstage · Capture review"
    reviewWindow!.contentView = web
    web.loadFileURL(url, allowingReadAccessTo: rootURL)
    reviewWindow!.makeKeyAndOrderFront(nil)
    renderer?.workshopSession.lastReview = url
    renderer?.workshopSession.saveReviewState()
  }
  func windowDidResignKey(_ notification: Notification) { renderer?.keys.removeAll() }
  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
  func applicationWillTerminate(_ notification: Notification) {
    renderer?.workshopSession.reviewProcess?.terminate()
    renderer?.view.isPaused = true
    if let renderer, let rootURL {
      let encoder = JSONEncoder()
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
      try? encoder.encode(renderer.workshopSession.reviewOriginal ?? renderer.workshopDocument())
        .write(to: rootURL.appendingPathComponent("last-study.json"), options: .atomic)
    }
    if let bridge {
      bridge.write(
        ["stopped": true, "sessionPID": ProcessInfo.processInfo.processIdentifier],
        to: bridge.root.appendingPathComponent("status.json"))
    }
  }
}

package enum SoundstageApplication {
  package static func run(projects: [GameProject]) {
    let args = CommandLine.arguments
    var selected = args.firstIndex(of: "--project").flatMap {
      $0 + 1 < args.count ? args[$0 + 1] : nil
    }
    if selected == nil,
      let data = try? Data(
        contentsOf: ProjectContext.workspace.appendingPathComponent(".soundstage/last-study.json")),
      let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    {
      selected = object["project"] as? String
    }
    ProjectContext.configure(projects, selected: selected)
    if args.contains("--check-renderer") {
      do {
        let r = try WorkshopRenderer(view: MTKView())
        print(
          "Renderer compiled on \(r.device.name); diffuse irradiance max error \(try r.verifyDiffuseLighting())"
        )
      } catch {
        fputs("\(error)\n", stderr)
        exit(1)
      }
      return
    }
    let app = NSApplication.shared
    let delegate = AppController()
    app.delegate = delegate
    withExtendedLifetime(delegate) { app.run() }
  }
}
