import AppKit
import FieldCore
import FieldEngine
import MetalKit

private final class PlayView: MTKView {
  weak var owner: GameApplicationController?
  private var lastDragPoint: NSPoint?
  override var acceptsFirstResponder: Bool { true }
  override func mouseDown(with event: NSEvent) { window?.makeFirstResponder(self) }
  override func mouseDragged(with event: NSEvent) {
    let point = event.locationInWindow
    if let old = lastDragPoint {
      owner?.session?.look(dx: Float(point.x - old.x), dy: Float(old.y - point.y))
    }
    lastDragPoint = point
  }
  override func mouseUp(with event: NSEvent) { lastDragPoint = nil }
  override func keyDown(with event: NSEvent) {
    guard let o = owner, let s = o.session else { return }
    switch event.keyCode {
    case 14: if !event.isARepeat { o.interact() }
    case 3:
      if s.game.lighting.flashlight && !event.isARepeat {
        do { try s.game.command("flashlight", [:]) } catch {
          o.message.stringValue = error.localizedDescription
        }
      }
    case 35:
      s.paused.toggle()
      s.keys = []
    case 123: s.look(dx: -35, dy: 0)
    case 124: s.look(dx: 35, dy: 0)
    case 125: s.look(dx: 0, dy: 25)
    case 126: s.look(dx: 0, dy: -25)
    case 53: s.keys = []
    default:
      if !event.isARepeat && !s.paused,
        let d = [
          UInt16(13): V3(0, 0, 0.12), 1: V3(0, 0, -0.12), 0: V3(-0.12, 0, 0), 2: V3(0.12, 0, 0),
        ][event.keyCode]
      {
        s.game.move(d)
      }
      s.keys.insert(event.keyCode)
    }
    o.refresh()
  }
  override func keyUp(with event: NSEvent) { owner?.session?.keys.remove(event.keyCode) }
  override func flagsChanged(with event: NSEvent) {
    if event.modifierFlags.contains(.shift) {
      owner?.session?.keys.insert(56)
    } else {
      owner?.session?.keys.remove(56)
    }
  }
}
private final class GameApplicationController: NSObject, NSApplicationDelegate, NSWindowDelegate {
  var window: NSWindow!, view: PlayView!, session: GameSession?, bridge: GameBridge?
  var title = NSTextField(labelWithString: ""), objective = NSTextField(labelWithString: ""),
    detail = NSTextField(labelWithString: ""), message = NSTextField(labelWithString: "")
  var button = NSButton(title: "Interact", target: nil, action: nil)
  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.regular)
    let menu = NSMenu()
    let item = NSMenuItem()
    let appMenu = NSMenu()
    menu.addItem(item)
    item.submenu = appMenu
    appMenu.addItem(
      withTitle: "Quit \(ProjectContext.current.name)",
      action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    NSApp.mainMenu = menu
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1200, height: 675),
      styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
    )
    window.title = ProjectContext.current.name
    window.contentAspectRatio = NSSize(width: 16, height: 9)
    window.minSize = NSSize(width: 960, height: 560)
    window.delegate = self
    view = PlayView(frame: window.contentView!.bounds)
    view.owner = self
    view.autoresizingMask = [.width, .height]
    window.contentView = view
    let stack = NSStackView(views: [title, objective, detail, button, message])
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 10
    let panel = NSVisualEffectView()
    panel.material = .hudWindow
    panel.blendingMode = .withinWindow
    panel.state = .active
    panel.wantsLayer = true
    panel.layer?.cornerRadius = 14
    panel.layer?.masksToBounds = true
    panel.translatesAutoresizingMaskIntoConstraints = false
    stack.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(panel)
    panel.addSubview(stack)
    for l in [title, objective, detail, message] {
      l.textColor = .white
      l.font = .systemFont(ofSize: l === title ? 23 : 12)
      l.maximumNumberOfLines = 3
      l.preferredMaxLayoutWidth = 460
    }
    message.stringValue = "W A S D walk · Shift run · Drag / arrows look · E interact"
    button.target = self
    button.action = #selector(interact)
    button.bezelStyle = .rounded
    NSLayoutConstraint.activate([
      panel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
      panel.topAnchor.constraint(equalTo: view.topAnchor, constant: 24),
      panel.widthAnchor.constraint(lessThanOrEqualToConstant: 500),
      stack.leadingAnchor.constraint(equalTo: panel.leadingAnchor, constant: 16),
      stack.trailingAnchor.constraint(equalTo: panel.trailingAnchor, constant: -16),
      stack.topAnchor.constraint(equalTo: panel.topAnchor, constant: 16),
      stack.bottomAnchor.constraint(equalTo: panel.bottomAnchor, constant: -16),
    ])
    let cross = NSTextField(labelWithString: "·")
    cross.textColor = .white
    cross.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(cross)
    NSLayoutConstraint.activate([
      cross.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      cross.centerYAnchor.constraint(equalTo: view.centerYAnchor),
    ])
    window.center()
    window.makeKeyAndOrderFront(nil)
    window.makeFirstResponder(view)
    NSApp.activate(ignoringOtherApps: true)
    do {
      let s = try GameSession(view: view)
      session = s
      if s.game.lighting.flashlight { message.stringValue += " · F flashlight" }
      bridge = try GameBridge(
        s, root: ProjectContext.dataRoot)
      s.onUpdate = { [weak self] in self?.refresh() }
      bridge?.onMessage = { [weak self] in self?.message.stringValue = $0 }
      refresh()
    } catch {
      message.stringValue = "Unable to start: \(error)"
      fputs("\(error)\n", stderr)
    }
  }
  func refresh() {
    guard let h = session?.game.hud else { return }
    title.stringValue = h.title
    objective.stringValue = h.objective
    detail.stringValue = h.detail
    button.title = "E · " + (h.prompt ?? "Explore")
    button.setAccessibilityLabel(button.title)
    button.isEnabled = h.prompt != nil
  }
  @objc func interact() {
    do { message.stringValue = try session?.game.interact() ?? "" } catch {
      message.stringValue = error.localizedDescription
    }
    refresh()
    window.makeFirstResponder(view)
  }
  func windowDidResignKey(_ notification: Notification) { session?.keys = [] }
  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
  func applicationWillTerminate(_ notification: Notification) {
    do { try session?.game.save() } catch { fputs("Save failed: \(error)\n", stderr) }
    bridge?.stopped()
  }
}
package enum GameApplication {
  package static func run(project: GameProject) {
    ProjectContext.configure([project])
    let app = NSApplication.shared
    let delegate = GameApplicationController()
    app.delegate = delegate
    withExtendedLifetime(delegate) { app.run() }
  }
}
