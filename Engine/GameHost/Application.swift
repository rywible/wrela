import AppKit
import FieldCore
import FieldEngine
import MetalKit

private final class PlayView: MTKView {
  weak var owner: GameApplicationController?
  private var lastDragPoint: NSPoint?
  override var acceptsFirstResponder: Bool { true }
  override func mouseDown(with event: NSEvent) {
    lastDragPoint = event.locationInWindow
    window?.makeFirstResponder(self)
  }
  override func mouseDragged(with event: NSEvent) {
    let point = event.locationInWindow
    if let old = lastDragPoint {
      owner?.session?.look(dx: Float(point.x - old.x), dy: Float(old.y - point.y))
    }
    lastDragPoint = point
  }
  override func mouseUp(with event: NSEvent) { lastDragPoint = nil }
  override func keyDown(with event: NSEvent) {
    // AppKit's main menu owns Command-key editing shortcuts. If an unavailable
    // command reaches the play view, it still must not become a movement key.
    if event.modifierFlags.contains(.command) {
      super.keyDown(with: event)
      return
    }
    guard let o = owner, let s = o.session else { return }
    switch event.keyCode {
    case 17: if !event.isARepeat { o.focusTextInput() }
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
    case 53:
      s.keys = []
      if s.game.cancelInteraction() { o.message.stringValue = "" }
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
private final class GameApplicationController: NSObject, NSApplicationDelegate, NSWindowDelegate,
  NSTextFieldDelegate
{
  var window: NSWindow!, view: PlayView!, session: GameSession?, bridge: GameBridge?
  var title = NSTextField(labelWithString: ""), objective = NSTextField(labelWithString: ""),
    detail = NSTextField(labelWithString: ""), message = NSTextField(labelWithString: "")
  var button = NSButton(title: "Interact", target: nil, action: nil)
  let textInput = NSTextField(string: "")
  let textSubmit = NSButton(title: "Send", target: nil, action: nil)
  var textRow: NSStackView!
  let controlRow = NSStackView()
  let controlPicker = NSPopUpButton(frame: .zero, pullsDown: false)
  let controlApply = NSButton(title: "Apply", target: nil, action: nil)
  private var displayedControls: [GameControl] = []
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

    let editItem = NSMenuItem()
    menu.addItem(editItem)
    let editMenu = NSMenu(title: "Edit")
    editItem.submenu = editMenu
    editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
    editMenu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
    editMenu.addItem(.separator())
    editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    editMenu.addItem(.separator())
    editMenu.addItem(
      withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
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
    textInput.delegate = self
    textInput.target = self
    textInput.action = #selector(submitText)
    // Return and Send submit explicitly. Merely moving focus to the Send button
    // must not submit once on editing end and then again with an empty field.
    (textInput.cell as? NSTextFieldCell)?.sendsActionOnEndEditing = false
    textInput.setAccessibilityLabel("Message")
    textSubmit.target = self
    textSubmit.action = #selector(submitText)
    textSubmit.bezelStyle = .rounded
    textRow = NSStackView(views: [textInput, textSubmit])
    textRow.orientation = .horizontal
    textRow.spacing = 8
    textRow.isHidden = true
    textInput.widthAnchor.constraint(greaterThanOrEqualToConstant: 240).isActive = true
    controlRow.orientation = .horizontal
    controlRow.spacing = 8
    controlRow.isHidden = true
    controlPicker.target = self
    controlPicker.action = #selector(selectControl)
    controlPicker.setAccessibilityLabel("Game action")
    controlPicker.widthAnchor.constraint(lessThanOrEqualToConstant: 340).isActive = true
    controlApply.target = self
    controlApply.action = #selector(activateSelectedControl)
    controlApply.bezelStyle = .rounded
    controlApply.setAccessibilityLabel("Apply selected game action")
    controlApply.isEnabled = false
    controlRow.addArrangedSubview(controlPicker)
    controlRow.addArrangedSubview(controlApply)
    let stack = NSStackView(views: [title, objective, detail, button, controlRow, textRow, message])
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
    for l in [detail, message] {
      l.lineBreakMode = .byWordWrapping
      (l.cell as? NSTextFieldCell)?.usesSingleLineMode = false
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
    let textPrompt = session?.game.textInputPrompt
    textRow.isHidden = textPrompt == nil
    textInput.placeholderString = textPrompt
    let controls = session?.game.controls ?? []
    if controls != displayedControls {
      let selectedID = controlPicker.selectedItem?.representedObject as? String
      controlPicker.removeAllItems()
      controlPicker.addItem(withTitle: "Choose an action")
      for control in controls {
        controlPicker.addItem(withTitle: control.title)
        controlPicker.item(at: controlPicker.numberOfItems - 1)?.representedObject = control.id
      }
      if let selectedID, let index = controls.firstIndex(where: { $0.id == selectedID }) {
        controlPicker.selectItem(at: index + 1)
      } else { controlPicker.selectItem(at: 0) }
      displayedControls = controls
    }
    controlRow.isHidden = controls.isEmpty
    controlApply.isEnabled = !controls.isEmpty && controlPicker.indexOfSelectedItem > 0
  }
  @objc func selectControl() {
    controlApply.isEnabled = controlPicker.indexOfSelectedItem > 0
  }
  @objc func activateSelectedControl() {
    guard let id = controlPicker.selectedItem?.representedObject as? String else { return }
    session?.keys = []
    do {
      let response = try session?.game.activateControl(id) ?? ""
      if let control = displayedControls.first(where: { $0.id == id }),
        control.presentation == .document {
        presentDocument(title: control.title, text: response)
      } else { message.stringValue = response }
    }
    catch { message.stringValue = error.localizedDescription }
    refresh()
    window.makeFirstResponder(view)
  }
  /// Games supply the content; the native host supplies a reusable readable document surface.
  private func presentDocument(title: String, text: String) {
    guard window.attachedSheet == nil else { return }
    let alert = NSAlert()
    alert.messageText = title
    alert.addButton(withTitle: "Close")
    let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 520, height: 320))
    scroll.hasVerticalScroller = true
    let document = NSTextView(frame: scroll.bounds)
    document.isEditable = false
    document.isSelectable = true
    document.font = .systemFont(ofSize: 14)
    document.textColor = .labelColor
    document.textContainerInset = NSSize(width: 12, height: 12)
    document.autoresizingMask = [.width]
    document.isVerticallyResizable = true
    document.textContainer?.widthTracksTextView = true
    document.string = text
    document.setAccessibilityLabel(title)
    scroll.documentView = document
    alert.accessoryView = scroll
    let wasPaused = session?.paused ?? false
    session?.paused = true
    alert.beginSheetModal(for: window) { [weak self] _ in
      self?.session?.paused = wasPaused
      self?.window.makeFirstResponder(self?.view)
    }
  }
  func focusTextInput() {
    guard session?.game.textInputPrompt != nil else { return }
    session?.keys = []
    window.makeFirstResponder(textInput)
  }
  func controlTextDidBeginEditing(_ notification: Notification) { session?.keys = [] }
  func controlTextDidEndEditing(_ notification: Notification) { session?.keys = [] }
  func control(
    _ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector
  ) -> Bool {
    if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
      if session?.game.cancelInteraction() == true {
        session?.keys = []
        refresh()
        window.makeFirstResponder(view)
        return true
      }
      session?.keys = []
      window.makeFirstResponder(view)
      return true
    }
    return false
  }
  @objc func submitText() {
    let text = textInput.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else { return }
    session?.keys = []
    do {
      message.stringValue = try session?.game.submitText(text) ?? ""
      textInput.stringValue = ""
    } catch {
      message.stringValue = error.localizedDescription
    }
    refresh()
    window.makeFirstResponder(view)
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
