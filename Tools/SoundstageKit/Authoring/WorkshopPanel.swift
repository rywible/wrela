import AppKit
import FieldCore
import FieldEngine

final class WorkshopPopup: NSPopUpButton {
  var changed: ((String) -> Void)?
  init(_ name: String, _ choices: [String], _ changed: @escaping (String) -> Void) {
    super.init(frame: .zero, pullsDown: false)
    addItems(withTitles: choices)
    self.changed = changed
    target = self
    action = #selector(selectValue)
    setAccessibilityLabel(name)
    font = .systemFont(ofSize: 12)
  }
  required init?(coder: NSCoder) { fatalError() }
  @objc func selectValue() { if let value = titleOfSelectedItem { changed?(value) } }
}
final class WorkshopSlider: NSSlider {
  var changed: ((Float) -> Void)?
  init(
    _ name: String, _ value: Float, _ range: ClosedRange<Float>,
    _ changed: @escaping (Float) -> Void
  ) {
    super.init(frame: .zero)
    minValue = Double(range.lowerBound)
    maxValue = Double(range.upperBound)
    floatValue = value
    self.changed = changed
    target = self
    action = #selector(change)
    isContinuous = false
    setAccessibilityLabel(name)
  }
  required init?(coder: NSCoder) { fatalError() }
  @objc func change() { changed?(floatValue) }
}
final class WorkshopNumber: NSTextField, NSTextFieldDelegate {
  var changed: ((String) -> Void)?
  init(_ name: String, _ changed: @escaping (String) -> Void) {
    super.init(frame: .zero)
    self.changed = changed
    delegate = self
    font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
    alignment = .right
    setAccessibilityLabel(name + " value")
    translatesAutoresizingMaskIntoConstraints = false
    widthAnchor.constraint(equalToConstant: 64).isActive = true
  }
  required init?(coder: NSCoder) { fatalError() }
  func controlTextDidEndEditing(_ obj: Notification) { changed?(stringValue) }
}
final class WorkshopDocumentView: NSView { override var isFlipped: Bool { true } }
final class WorkshopPanel: NSView {
  weak var owner: AppController?
  var sliders: [String: WorkshopSlider] = [:]
  var controlLabels: [String: NSTextField] = [:]
  var resetButtons: [String: ActionButton] = [:]
  var projectPopup: WorkshopPopup!
  func definition(_ key: String) -> ScalarControl? {
    guard let source = owner?.renderer?.studioSource else { return nil }
    if key.hasPrefix("shape.") {
      return source.controls.first { $0.key == String(key.dropFirst(6)) }
    }
    if key.hasPrefix("motion.") {
      return source.animation?.controls.first { $0.key == String(key.dropFirst(7)) }
    }
    return nil
  }
  var generatedControls: [String: NSView] = [:]
  var animationControls: [String: NSView] = [:]
  var activeTab = "Object"
  var sectionItems: [String: [NSView]] = [:]
  var values: [String: NSTextField] = [:]
  var subject: WorkshopPopup!
  var partPopup: WorkshopPopup!
  var parentPopup: WorkshopPopup!
  var motionPopup: WorkshopPopup!
  var scenarioPopup: WorkshopPopup!
  var isolateButton: ActionButton!
  var followButton: ActionButton!
  var motionStatus: NSTextField!
  var behaviorStatus: NSTextField!
  var rig: WorkshopPopup!
  var sky: WorkshopPopup!
  var arrangement: WorkshopPopup!
  var shapeGroup: NSStackView!
  var anatomyGroup: NSStackView!
  var rigGroup: NSStackView!
  var identity: NSTextField!
  var metrics: NSTextField!
  var live: ActionButton!
  var spin: ActionButton!
  var marker: ActionButton!
  var ground: ActionButton!
  var viewPopup: WorkshopPopup!
  var undoButton: ActionButton!
  var redoButton: ActionButton!
  var captureButton: ActionButton!
  var nameField: NSTextField!
  var checkpointPopup: WorkshopPopup!
  var baselinePopup: WorkshopPopup!
  var watchButton: ActionButton!
  var watchStatus: NSTextField!
  init(_ owner: AppController) {
    self.owner = owner
    super.init(frame: .zero)
    translatesAutoresizingMaskIntoConstraints = false
    wantsLayer = true
    layer?.backgroundColor = NSColor(calibratedWhite: 0.09, alpha: 0.94).cgColor
    layer?.cornerRadius = 12
    appearance = NSAppearance(named: .darkAqua)
    let navigation = NSStackView()
    navigation.translatesAutoresizingMaskIntoConstraints = false
    navigation.spacing = 4
    addSubview(navigation)
    let scroll = NSScrollView()
    scroll.translatesAutoresizingMaskIntoConstraints = false
    scroll.hasVerticalScroller = true
    scroll.drawsBackground = false
    let stack = NSStackView()
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 9
    stack.translatesAutoresizingMaskIntoConstraints = false
    let document = WorkshopDocumentView()
    document.translatesAutoresizingMaskIntoConstraints = false
    document.addSubview(stack)
    scroll.documentView = document
    addSubview(scroll)
    NSLayoutConstraint.activate([
      scroll.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
      scroll.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
      scroll.topAnchor.constraint(equalTo: navigation.bottomAnchor, constant: 12),
      scroll.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -14),
      document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
      stack.leadingAnchor.constraint(equalTo: document.leadingAnchor),
      stack.trailingAnchor.constraint(equalTo: document.trailingAnchor, constant: -4),
      stack.topAnchor.constraint(equalTo: document.topAnchor),
      stack.bottomAnchor.constraint(equalTo: document.bottomAnchor),
    ])
    NSLayoutConstraint.activate([
      navigation.topAnchor.constraint(equalTo: topAnchor, constant: 12),
      navigation.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
    ])
    navigation.addArrangedSubview(
      WorkshopPopup(
        "Authoring section", ["Object", "Parts", "Motion", "Behavior", "Lighting", "Look", "Files"]
      ) { [weak self] name in
        guard let self else { return }
        self.activeTab = name
        self.refresh()
        self.layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: .zero)
        scroll.reflectScrolledClipView(scroll.contentView)
      })
    var section = "Object"
    func add(_ view: NSView) {
      stack.addArrangedSubview(view)
      sectionItems[section, default: []].append(view)
    }
    func label(_ name: String) -> NSTextField {
      owner.label(name, size: 10, weight: .semibold, color: .init(white: 0.65, alpha: 1))
    }
    func row(_ views: [NSView]) -> NSStackView {
      let r = NSStackView(views: views)
      r.spacing = 5
      return r
    }
    func button(_ name: String, _ action: @escaping (WorkshopRenderer) throws -> Void)
      -> ActionButton
    {
      ActionButton(name) { [weak self] in self?.perform(name, action) }
    }
    func slider(
      _ title: String, _ key: String, _ value: Float, _ range: ClosedRange<Float>,
      _ action: @escaping (WorkshopRenderer, Float) throws -> Void
    ) -> NSStackView {
      let text = owner.label(title, size: 11)
      let number = WorkshopNumber(title) { [weak self] value in
        guard let self else { return }
        let text = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let inherited =
          (key == "roughness" || key == "metallic")
          && (text.lowercased() == "asset" || text == "-1")
        let parsed: Float? = inherited ? -1 : Float(text)
        let range = self.definition(key)?.range ?? range
        guard let parsed, parsed.isFinite, range.contains(parsed) || inherited else {
          owner.messageLabel.stringValue =
            "\(self.definition(key)?.title ?? title) must be \(range.lowerBound)…\(range.upperBound)."
          self.refresh()
          return
        }
        self.perform(self.definition(key)?.title ?? title) { try action($0, parsed) }
      }
      let slider = WorkshopSlider(title, value, range) { [weak self] value in
        self?.perform(self?.definition(key)?.title ?? title) { try action($0, value) }
      }
      let reset = ActionButton("↺") { [weak self] in
        self?.perform("Reset " + (self?.definition(key)?.title ?? title)) { r in
          let preset = StudioRig.preset(r.studioRig.name)
          let defaults: [String: Float] = [
            "rig.x": preset.x, "rig.y": preset.y, "rig.z": preset.z,
            "rig.intensity": preset.intensity, "rig.size": preset.size, "rig.warmth": preset.warmth,
            "rig.fill": preset.fill, "scale": 1, "roughness": -1, "metallic": -1, "wetness": 0,
            "exposure": 1, "wind": 1,
          ]
          let initial =
            key.hasPrefix("look.")
            ? SceneLook().values[String(key.dropFirst(5))] ?? value
            : key.hasPrefix("shape.")
              ? AssetSource.defaults(r.studioSource.generator).parameters[String(key.dropFirst(6))]
                ?? value : defaults[key] ?? value
          try action(r, self?.definition(key)?.initial ?? initial)
        }
      }
      reset.setAccessibilityLabel("Reset " + title)
      reset.toolTip = "Reset " + title
      controlLabels[key] = text
      resetButtons[key] = reset
      sliders[key] = slider
      values[key] = number
      let group = NSStackView(views: [row([text, number, reset]), slider])
      group.orientation = .vertical
      group.alignment = .leading
      group.spacing = 1
      slider.widthAnchor.constraint(equalToConstant: 246).isActive = true
      return group
    }
    projectPopup = WorkshopPopup("Project", ProjectContext.projects.map(\.id)) { [weak self] id in
      guard let self, let r = self.owner?.renderer else { return }
      do {
        try r.switchProject(id)
        self.refresh()
      } catch { self.owner?.messageLabel.stringValue = error.localizedDescription }
    }
    add(projectPopup)
    add(label("SUBJECT"))
    subject = WorkshopPopup("Subject", AssetSource.availableIDs + ["sky"]) { [weak self] name in
      self?.perform { try $0.selectSubject(name) }
    }
    add(subject)
    identity = owner.label("", size: 11)
    identity.maximumNumberOfLines = 3
    add(identity)
    add(
      row([
        button("Save asset") { r in
          let url = try r.saveAsset()
          owner.messageLabel.stringValue = "Saved source · \(url.lastPathComponent)"
        },
        button("Reload source") { r in
          try r.applySource(AssetSource.read(r.studioSourceURL))
          r.workshopSession.resetWatch()
        },
      ]))
    shapeGroup = NSStackView()
    shapeGroup.orientation = .vertical
    shapeGroup.alignment = .leading
    shapeGroup.spacing = 6
    let definitions = ProjectContext.projects.flatMap { $0.generators }.flatMap { $0.controls }
    var seen = Set<String>()
    for control in definitions where seen.insert(control.key).inserted {
      let row = slider(control.title, "shape." + control.key, control.initial, control.range) {
        try $0.editShape([control.key: $1])
      }
      generatedControls[control.key] = row
      shapeGroup.addArrangedSubview(row)
    }
    add(shapeGroup)
    anatomyGroup = NSStackView()
    arrangement = WorkshopPopup("Arrangement", ["single", "grove"]) { [weak self] value in
      self?.perform {
        $0.studioLayout.arrangement = value
        $0.fitStudioCamera()
      }
    }
    add(arrangement)
    marker = button("Scale reference: off") {
      $0.studioLayout.reference.toggle()
      $0.fitStudioCamera()
    }
    ground = button("Ground: on") { $0.studioLayout.ground.toggle() }
    add(row([marker, ground]))
    add(slider("Subject scale", "scale", 1, 0.25...4) { $0.setStudioScale($1) })
    section = "Parts"
    add(label("NAMED PARTS · METRES / DEGREES"))
    partPopup = WorkshopPopup("Selected part", ["All parts"]) { [weak self] id in
      self?.perform("Select part") { try $0.selectPart(id == "All parts" ? nil : id) }
    }
    add(partPopup)
    isolateButton = button("Isolate: off") { r in
      try r.selectPart(r.creatureWorkshop.selected, isolated: !r.creatureWorkshop.isolated)
    }
    add(row([isolateButton, button("Frame selected part") { try $0.framePart() }]))
    add(label("ATTACHED TO"))
    parentPopup = WorkshopPopup("Parent attachment", ["none"]) { [weak self] id in
      self?.perform("Attach part") { try $0.editPart([:], parent: id) }
    }
    add(parentPopup)
    for (key, title, initial, range) in [
      ("x", "Part offset X", Float(0), Float(-2)...2), ("y", "Part offset Y", 0, -2...2),
      ("z", "Part offset Z", 0, -2...2),
      ("pitch", "Part pitch", 0, -90...90), ("yaw", "Part yaw", 0, -180...180),
      ("roll", "Part roll", 0, -90...90),
      ("scale", "Part scale", 1, 0.25...4), ("pivotX", "Pivot X", 0, -10...10),
      ("pivotY", "Pivot Y", 0, -10...10), ("pivotZ", "Pivot Z", 0, -10...10),
      ("roughness", "Part roughness", 0.7, 0.05...1), ("metallic", "Part metallic", 0, 0...1),
    ] { add(slider(title, "part." + key, initial, range) { try $0.editPart([key: $1]) }) }
    add(button("Save part edits") { _ = try $0.saveAsset() })
    section = "Motion"
    add(label("ANIMATION"))
    motionPopup = WorkshopPopup("Motion mode", ["bind", "behavior"]) {
      [weak self] mode in
      self?.perform("Motion \(mode)") { r in
        try r.editCreatureWorkshop(mode: mode)
        r.paused = true
      }
    }
    add(motionPopup)
    motionStatus = owner.label("", size: 11)
    motionStatus.maximumNumberOfLines = 4
    add(motionStatus)
    add(
      slider("Timeline · seconds", "motion.time", 0, 0...60) {
        try $0.editCreatureWorkshop(seconds: $1)
      })
    add(
      slider("Playback speed", "motion.rate", 1, 0.1...2) { try $0.editCreatureWorkshop(rate: $1) })
    add(
      row([
        button("Previous frame") { r in
          try r.editCreatureWorkshop(seconds: max(0, r.creatureWorkshop.seconds - 1 / 60))
        },
        button("Next frame") { r in
          try r.editCreatureWorkshop(seconds: min(60, r.creatureWorkshop.seconds + 1 / 60))
        },
      ]))
    add(button("Restart motion") { try $0.editCreatureWorkshop(seconds: 0) })
    var motionKeys = Set<String>()
    for control in ProjectContext.projects.flatMap({ $0.generators }).flatMap({
      $0.animation?.controls ?? []
    }) where motionKeys.insert(control.key).inserted {
      let row = slider(control.title, "motion." + control.key, control.initial, control.range) {
        try $0.editMotion([control.key: $1])
      }
      animationControls[control.key] = row
      add(row)
    }
    add(button("Save motion recipe") { _ = try $0.saveAsset() })
    add(
      ActionButton("Capture motion strip") { [weak self] in
        self?.sessionAction { try $0.captureReview(motion: true) }
      })
    section = "Behavior"
    add(label("BEHAVIOR SCENARIOS"))
    scenarioPopup = WorkshopPopup(
      "Behavior scenario",
      ProjectContext.current.generators.flatMap { $0.animation?.scenarios ?? [] }
    ) {
      [weak self] scenario in
      self?.perform("Scenario \(scenario)") { r in
        try r.editCreatureWorkshop(scenario: scenario)
        r.paused = true
      }
    }
    add(scenarioPopup)
    add(
      row([
        button("Restart scenario") { r in
          try r.editCreatureWorkshop(scenario: r.creatureWorkshop.scenario)
          r.paused = true
        },
        button("Run 10 seconds") { r in try r.editCreatureWorkshop(mode: "behavior", seconds: 10) },
      ]))
    add(
      slider("Scenario seed", "behavior.seed", 17, 0...10000) {
        try $0.editCreatureWorkshop(seed: UInt32($1.rounded()))
      })
    followButton = button("Follow creature: on") {
      try $0.editCreatureWorkshop(follow: !$0.creatureWorkshop.follow)
    }
    add(
      row([
        followButton,
        button("Frame encounter") { r in
          try r.editCreatureWorkshop(mode: "behavior", follow: false)
        },
      ]))
    add(label("VISITOR · BLUE CALM / RED RUNNING"))
    for (key, title) in [("x", "Visitor X · m"), ("z", "Visitor Z · m")] {
      add(
        slider(title, "stimulus." + key, key == "x" ? 0 : 3, -20...20) { r, v in
          var input = r.creatureWorkshop.input
          if key == "x" { input.player.x = v } else { input.player.y = v }
          r.creatureWorkshop.stimulusOverride = input
        })
    }
    add(
      row([
        button("Toggle running") { r in
          var input = r.creatureWorkshop.input
          input.running.toggle()
          r.creatureWorkshop.stimulusOverride = input
        },
        button("Toggle visibility") { r in
          var input = r.creatureWorkshop.input
          input.visible.toggle()
          r.creatureWorkshop.stimulusOverride = input
        },
      ]))
    behaviorStatus = owner.label("", size: 11)
    behaviorStatus.maximumNumberOfLines = 18
    add(behaviorStatus)
    add(
      ActionButton("Review behavior sequence") { [weak self] in
        self?.sessionAction { try $0.captureReview(motion: true) }
      })
    section = "Lighting"
    add(label("LIGHTING"))
    rig = WorkshopPopup("Lighting rig", StudioRig.names) { [weak self] value in
      self?.perform { $0.studioRig = StudioRig.preset(value) }
    }
    add(rig)
    sky = WorkshopPopup(
      "Outdoor sky", ["noon", "golden", "sunset", "afterglow", "overcast", "rain"]
    ) { [weak self] value in self?.perform { $0.lighting = value } }
    add(sky)
    rigGroup = NSStackView()
    rigGroup.orientation = .vertical
    rigGroup.alignment = .leading
    rigGroup.spacing = 5
    for (key, title, range) in [
      ("x", "Light horizontal", Float(-4)...4), ("y", "Light height", Float(0.1)...4),
      ("z", "Light depth", Float(-4)...4), ("intensity", "Light intensity", Float(0)...40),
      ("size", "Source size", Float(0.005)...1), ("warmth", "Warmth", Float(0)...1),
      ("fill", "Ambient fill", Float(0)...0.5),
    ] {
      rigGroup.addArrangedSubview(
        slider(title, "rig." + key, 0, range) { r, v in
          switch key {
          case "x": r.studioRig.x = v
          case "y": r.studioRig.y = v
          case "z": r.studioRig.z = v
          case "intensity": r.studioRig.intensity = v
          case "size": r.studioRig.size = v
          case "warmth": r.studioRig.warmth = v
          default: r.studioRig.fill = v
          }
        })
    }
    rigGroup.addArrangedSubview(
      owner.label("Position and size follow subject extent.", size: 10, color: .gray))
    add(rigGroup)
    section = "Object"
    add(label("MATERIAL"))
    add(
      button("Use asset material") {
        $0.studioLayout.roughness = -1
        $0.studioLayout.metallic = -1
        $0.studioLayout.tint = 1
        $0.wetness = 0
      })
    add(slider("Roughness", "roughness", 0.8, 0.05...1) { r, v in r.studioLayout.roughness = v })
    add(slider("Metallic", "metallic", 0, 0...1) { r, v in r.studioLayout.metallic = v })
    add(slider("Wetness", "wetness", 0, 0...1) { $0.wetness = $1 })
    add(slider("Exposure", "exposure", 1, 0.5...1.5) { $0.exposure = $1 })
    add(slider("Wind", "wind", 1, 0...2) { $0.wind = $1 })
    section = "Look"
    add(label("SCENE LOOK · OBJECTS + LIGHT + SKY"))
    for (key, title) in [
      ("detail", "Surface detail"), ("roughnessBias", "Roughness bias"),
      ("surfaceSaturation", "Material color"), ("sunStrength", "Sun strength"),
      ("ambientStrength", "Ambient strength"), ("skySaturation", "Sky color"),
      ("saturation", "Overall color"), ("contrast", "Contrast"), ("warmth", "Warmth grade"),
      ("bloom", "Glare"),
    ] {
      add(
        slider(title, "look." + key, SceneLook().values[key]!, SceneLook.ranges[key]!) { r, value in
          try r.sceneLook.set([key: value])
        })
    }
    add(
      button("Publish project look") { r in
        try r.sceneLook.publish()
        owner.messageLabel.stringValue = "Project look published · shared by game and workshop"
      })
    add(
      owner.label(
        "Shared project art direction\nStyle board compares shape, material, lighting and sky.",
        size: 10, color: .lightGray))
    section = "Files"
    add(label("SOURCE & CHECKPOINTS"))
    watchButton = ActionButton("Auto reload: on") { [weak self] in
      guard let self, let r = owner.renderer else { return }
      r.workshopSession.watching.toggle()
      r.workshopSession.resetWatch()
      self.refresh()
    }
    add(watchButton)
    watchStatus = owner.label("", size: 10, color: .lightGray)
    watchStatus.maximumNumberOfLines = 2
    add(watchStatus)
    nameField = NSTextField()
    nameField.placeholderString = "Name this checkpoint or baseline"
    nameField.setAccessibilityLabel("Checkpoint or baseline name")
    nameField.widthAnchor.constraint(equalToConstant: 246).isActive = true
    add(nameField)
    add(
      row([
        ActionButton("Checkpoint") { [weak self] in
          self?.sessionAction { _ = try $0.checkpoint(self?.nameField.stringValue ?? "") }
        },
        ActionButton("Pin review") { [weak self] in
          self?.sessionAction { _ = try $0.pinBaseline(self?.nameField.stringValue ?? "") }
        },
      ]))
    checkpointPopup = WorkshopPopup("Saved checkpoints", ["Choose checkpoint"]) { _ in }
    add(checkpointPopup)
    add(
      ActionButton("Restore checkpoint") { [weak self] in
        self?.sessionAction {
          try $0.restoreCheckpoint(self?.checkpointPopup.titleOfSelectedItem ?? "")
        }
      })
    baselinePopup = WorkshopPopup("Review baseline", ["None"]) { [weak self] name in
      self?.sessionAction { try $0.selectBaseline(name) }
    }
    add(baselinePopup)
    add(
      row([
        ActionButton("Save study") { [weak self] in self?.study(save: true) },
        ActionButton("Load study") { [weak self] in self?.study(save: false) },
      ]))
    metrics = owner.label("", size: 10, color: .lightGray)
    metrics.maximumNumberOfLines = 4
    add(metrics)
  }
  required init?(coder: NSCoder) { fatalError() }
  func perform(_ name: String = "Edit", _ action: (WorkshopRenderer) throws -> Void) {
    guard let r = owner?.renderer else { return }
    do { try r.workshopSession.edit(name) { try action(r) } } catch {
      owner?.messageLabel.stringValue = error.localizedDescription
    }
    refresh()
    owner?.window.makeFirstResponder(owner?.gameView)
  }
  func sessionAction(_ action: (WorkshopSession) throws -> Void) {
    guard let owner, let r = owner.renderer else { return }
    do {
      guard !r.workshopSession.automating, r.workshopSession.reviewProcess == nil else {
        throw RuntimeError.message("Wait for the review to finish")
      }
      try action(r.workshopSession)
    } catch { owner.messageLabel.stringValue = error.localizedDescription }
    refresh()
  }
  func toolbar() -> NSStackView {
    func b(_ title: String, _ action: @escaping (WorkshopRenderer) throws -> Void) -> ActionButton {
      ActionButton(title) { [weak self] in self?.perform(title, action) }
    }
    let views = WorkshopPopup(
      "Inspection view",
      [
        "Custom", "front", "quarter", "back", "left", "right", "above", "detail", "base", "crown",
        "sunward", "away", "zenith",
      ]
    ) { [weak self] name in
      if name != "Custom" { self?.perform("View " + name) { $0.setStudyView(name) } }
    }
    viewPopup = views
    undoButton = ActionButton("Undo") { [weak self] in self?.sessionAction { try $0.undo() } }
    redoButton = ActionButton("Redo") { [weak self] in self?.sessionAction { try $0.redo() } }
    let row1 = NSStackView(views: [
      views,
      b("Fit subject") {
        $0.studioObject = $0.studioSource.id
        $0.studioLayout.focus = 0.5
        $0.fitStudioCamera()
      }, undoButton, redoButton,
      ActionButton("Style board") { [weak self] in
        self?.sessionAction { try $0.captureReview(styleBoard: true) }
      },
    ])
    row1.spacing = 8
    live = b("Play") { $0.paused.toggle() }
    spin = b("Turntable: off") { $0.studioLayout.turntable.toggle() }
    captureButton = ActionButton("Capture & review") { [weak self] in
      self?.sessionAction { try $0.captureReview() }
    }
    let row2 = NSStackView(views: [
      live, spin,
      b("Step 1s") { r in
        r.paused = true
        for _ in 0..<60 { r.step(1 / 60) }
      }, captureButton,
      ActionButton("Open review") { [weak self] in
        if let url = self?.owner?.renderer?.workshopSession.lastReview {
          self?.owner?.openReview(url)
        }
      },
    ])
    row2.spacing = 8
    let stack = NSStackView(views: [row1, row2])
    stack.orientation = .vertical
    stack.spacing = 8
    return stack
  }
  func study(save: Bool) {
    guard let owner else { return }
    if save {
      let panel = NSSavePanel()
      panel.nameFieldStringValue = (owner.renderer?.studioSource.id ?? "field") + "-study.json"
      panel.directoryURL = owner.rootURL.appendingPathComponent("studies")
      panel.beginSheetModal(for: owner.window) { response in
        if response == .OK, let url = panel.url {
          self.perform { r in
            let e = JSONEncoder()
            e.outputFormatting = [.prettyPrinted, .sortedKeys]
            try e.encode(r.workshopDocument()).write(to: url, options: .atomic)
            owner.messageLabel.stringValue = "Saved study · \(url.lastPathComponent)"
          }
        }
      }
    } else {
      let panel = NSOpenPanel()
      panel.directoryURL = owner.rootURL.appendingPathComponent("studies")
      panel.allowsMultipleSelection = false
      panel.beginSheetModal(for: owner.window) { response in
        if response == .OK, let url = panel.url {
          self.perform { r in
            try r.restoreWorkshop(
              JSONDecoder().decode(WorkshopDocument.self, from: Data(contentsOf: url)))
          }
        }
      }
    }
  }
  func refresh() {
    guard let r = owner?.renderer else { return }
    for (tab, items) in sectionItems { for item in items { item.isHidden = tab != activeTab } }
    projectPopup.selectItem(withTitle: ProjectContext.current.id)
    let ids = AssetSource.availableIDs + ["sky"]
    if subject.itemTitles != ids {
      subject.removeAllItems()
      subject.addItems(withTitles: ids)
    }
    subject.selectItem(withTitle: r.studioObject)
    rig.selectItem(withTitle: r.studioRig.name)
    sky.selectItem(withTitle: r.lighting == "morning" ? "noon" : r.lighting)
    arrangement.selectItem(withTitle: r.studioLayout.arrangement)
    shapeGroup.isHidden = activeTab != "Object" || r.studioObject == "sky"
    for key in Array(generatedControls.keys).map({ "shape." + $0 })
      + Array(animationControls.keys).map({ "motion." + $0 })
    {
      if let d = definition(key) {
        sliders[key]?.minValue = Double(d.range.lowerBound)
        sliders[key]?.maxValue = Double(d.range.upperBound)
        sliders[key]?.setAccessibilityLabel(d.title)
        values[key]?.setAccessibilityLabel(d.title + " value")
        controlLabels[key]?.stringValue = d.title
        resetButtons[key]?.setAccessibilityLabel("Reset " + d.title)
        resetButtons[key]?.toolTip = "Reset " + d.title
      }
    }
    for (key, row) in generatedControls {
      row.isHidden = !r.studioSource.controls.contains { $0.key == key }
    }
    for (key, row) in animationControls {
      row.isHidden =
        activeTab != "Motion"
        || !(r.studioSource.animation?.controls.contains { $0.key == key } ?? false)
    }
    rigGroup.isHidden = activeTab != "Lighting" || r.studioRig.name == "outdoor"
    sky.isEnabled = r.studioRig.name == "outdoor"
    identity.stringValue =
      r.studioObject == "sky"
      ? "Atmosphere · no subject"
      : String(
        format: "%@\n%.3g m tall · %.3g m wide", r.studioSource.name, r.studioHeight,
        (r.studioBounds.max.x - r.studioBounds.min.x) * r.studioLayout.scale)
    var map = [
      "scale": r.studioLayout.scale, "roughness": r.studioLayout.roughness,
      "metallic": r.studioLayout.metallic, "wetness": r.wetness, "wind": r.wind,
      "exposure": r.exposure,
    ]
    let lab = r.creatureWorkshop
    func choices(_ popup: WorkshopPopup, _ items: [String], _ chosen: String) {
      if popup.itemTitles != items {
        popup.removeAllItems()
        popup.addItems(withTitles: items)
      }
      popup.selectItem(withTitle: chosen)
    }
    choices(
      partPopup, ["All parts"] + r.studioSource.resolvedParts.map(\.key),
      lab.selected ?? "All parts")
    let selectedPart = r.studioSource.resolvedParts.first { $0.key == lab.selected }
    let joint = selectedPart?.joint ?? PartJoint(id: selectedPart?.key ?? "")
    choices(
      parentPopup, ["none"] + r.studioSource.resolvedParts.map(\.key).filter { $0 != lab.selected },
      joint.parent ?? "none")
    parentPopup.isEnabled = selectedPart != nil
    isolateButton.title = lab.isolated ? "Isolate: on" : "Isolate: off"
    isolateButton.setAccessibilityLabel(isolateButton.title)
    followButton.title = lab.follow ? "Follow creature: on" : "Follow creature: off"
    followButton.setAccessibilityLabel(followButton.title)
    choices(motionPopup, lab.modes, lab.mode)
    motionPopup.isEnabled = r.studioSource.motion != nil
    choices(scenarioPopup, r.studioSource.animation?.scenarios ?? [], lab.scenario)
    scenarioPopup.isEnabled = r.studioSource.motion != nil
    let clipLength =
      r.studioSource.animation?.duration(r.studioSource.motion ?? MotionParameters()) ?? 1
    sliders["motion.time"]?.maxValue = Double(
      !["bind", "idle", "behavior"].contains(lab.mode) ? clipLength : 60)
    map["motion.time"] =
      !["bind", "idle", "behavior"].contains(lab.mode)
      ? lab.seconds.truncatingRemainder(dividingBy: clipLength) : lab.seconds
    map["motion.rate"] = lab.rate
    map["behavior.seed"] = Float(lab.seed)
    map["stimulus.x"] = lab.input.player.x
    map["stimulus.z"] = lab.input.player.y
    for (key, value) in (r.studioSource.motion ?? MotionParameters()).values {
      map["motion." + key] = value
    }
    let partValues: [String: Float] = [
      "x": joint.offset.x, "y": joint.offset.y, "z": joint.offset.z,
      "pivotX": joint.pivot.x, "pivotY": joint.pivot.y, "pivotZ": joint.pivot.z,
      "pitch": joint.rotation.x, "yaw": joint.rotation.y, "roll": joint.rotation.z,
      "scale": joint.scale,
      "roughness": selectedPart?.roughness ?? 0.7, "metallic": selectedPart?.metallic ?? 0,
    ]
    for (key, value) in partValues { map["part." + key] = value }
    for (key, control) in sliders where key.hasPrefix("part.") {
      control.isEnabled = selectedPart != nil
      values[key]?.isEnabled = selectedPart != nil
    }
    motionStatus.stringValue =
      r.studioSource.motion == nil
      ? "Select an animated subject."
      : String(
        format: "%.3f s · %@\nShared with %@", lab.seconds, lab.mode, ProjectContext.current.name)
    behaviorStatus.stringValue =
      (lab.recording.map {
        "RECORDED GAME STATE · \($0)\nPlay continues in the studio.\nRestart resets the scenario.\n\n"
      } ?? "")
      + "\(lab.actor.state.uppercased()) · \(lab.definition?.signalName ?? "Signal") \(String(format:"%.2f",lab.actor.fear))\n\(lab.actor.reason)\nVisitor: \(lab.input.running ? "running":"calm"), \(lab.input.visible ? "visible":"occluded")\nOrange: target · green: food\nGray: obstacle\n\n"
      + lab.actor.events.suffix(6).map {
        ($0.tick.map { String(format: "%.2fs · ", $0 == 0 ? 0 : Float($0) / 60) } ?? "")
          + "\($0.state): \($0.reason)"
      }.joined(separator: "\n")
    for (key, value) in r.sceneLook.values { map["look." + key] = value }
    for (key, value) in r.studioSource.parameters { map["shape." + key] = value }
    for (key, value) in [
      "x": r.studioRig.x, "y": r.studioRig.y, "z": r.studioRig.z,
      "intensity": r.studioRig.intensity, "size": r.studioRig.size, "warmth": r.studioRig.warmth,
      "fill": r.studioRig.fill,
    ] { map["rig." + key] = value }
    for (key, value) in map {
      let inherited = value < 0 && (key == "roughness" || key == "metallic")
      sliders[key]?.floatValue = inherited ? (key == "roughness" ? 0.8 : 0) : value
      if values[key]?.currentEditor() == nil {
        values[key]?.stringValue =
          inherited ? "Asset" : String(format: key == "shape.leaves" ? "%.0f" : "%.6g", value)
      }
    }
    choices(
      viewPopup,
      [
        "Custom", "front", "quarter", "back", "left", "right", "above", "detail", "base", "crown",
        "sunward", "away", "zenith",
      ] + (r.studioSource.definition?.views.map(\.name) ?? []), r.studioViewName)
    let session = r.workshopSession
    undoButton.isEnabled = !session.undoStack.isEmpty && !session.automating
    redoButton.isEnabled = !session.redoStack.isEmpty && !session.automating
    undoButton.toolTip = "Undo \(session.undoStack.last?.name ?? "")"
    redoButton.toolTip = "Redo \(session.redoStack.last?.name ?? "")"
    captureButton.isEnabled = session.reviewProcess == nil && !session.automating
    watchButton.title = session.watching ? "Auto reload: on" : "Auto reload: off"
    watchButton.setAccessibilityLabel(watchButton.title)
    watchStatus.stringValue = session.watchMessage
    let checkpoints = ["Choose checkpoint"] + session.namedFiles("checkpoints").map(\.0)
    if checkpointPopup.itemTitles != checkpoints {
      let chosen = checkpointPopup.titleOfSelectedItem
      checkpointPopup.removeAllItems()
      checkpointPopup.addItems(withTitles: checkpoints)
      if let chosen { checkpointPopup.selectItem(withTitle: chosen) }
    }
    let baselines = ["None"] + session.namedFiles("baselines").map(\.0)
    if baselinePopup.itemTitles != baselines {
      baselinePopup.removeAllItems()
      baselinePopup.addItems(withTitles: baselines)
    }
    let selected =
      session.namedFiles("baselines").first { _, url in
        guard let data = try? Data(contentsOf: url),
          let item = try? JSONDecoder().decode([String: String].self, from: data)
        else { return false }
        return item["report"] == session.selectedBaseline?.path
      }?.0 ?? "None"
    baselinePopup.selectItem(withTitle: selected)
    live.title = r.paused ? "Play" : "Pause"
    spin.title = r.studioLayout.turntable ? "Turntable: on" : "Turntable: off"
    marker.title = r.studioLayout.reference ? "Scale reference: on" : "Scale reference: off"
    ground.isEnabled = r.studioRig.name != "room"
    ground.title =
      r.studioRig.name == "room"
      ? "Room floor" : (r.studioLayout.ground ? "Ground: on" : "Ground: off")
    for control in [live, spin, marker, ground] { control?.setAccessibilityLabel(control?.title) }
    metrics.stringValue = String(
      format: "1080p · GPU %.1f ms · %d triangles\nLast field build %.0f ms · time %.1f s",
      r.stats(r.gpuTimes)["median"] ?? 0, r.visibleTriangles, r.studioBuildMilliseconds,
      r.simulationTime)
  }
}
extension AppController {
  func buildWorkshopHUD() {
    let root = window.contentView!
    workshopPanel = WorkshopPanel(self)
    notes = workshopPanel
    root.addSubview(notes)
    let canvas = NSView()
    canvas.translatesAutoresizingMaskIntoConstraints = false
    root.addSubview(canvas)
    gameView.removeFromSuperview()
    canvas.addSubview(gameView)
    gameView.translatesAutoresizingMaskIntoConstraints = false
    gameView.setAccessibilityLabel("Object viewport. Drag to orbit, scroll to zoom, R to fit.")
    let dock = workshopPanel!.toolbar()
    dock.translatesAutoresizingMaskIntoConstraints = false
    root.addSubview(dock)
    dock.heightAnchor.constraint(equalToConstant: 64).isActive = true
    messageLabel = label(
      "Drag to orbit · Scroll to zoom · R to fit · ⌘Z undo", size: 11, color: .lightGray)
    messageLabel.maximumNumberOfLines = 2
    root.addSubview(messageLabel)
    let title = label("S O U N D S T A G E  /  WORKSHOP", size: 12, weight: .semibold)
    root.addSubview(title)
    inspectorWidth = notes.widthAnchor.constraint(equalToConstant: 310)
    let fitWidth = gameView.widthAnchor.constraint(equalTo: canvas.widthAnchor)
    fitWidth.priority = .defaultHigh
    let fitHeight = gameView.heightAnchor.constraint(equalTo: canvas.heightAnchor)
    fitHeight.priority = .defaultHigh
    NSLayoutConstraint.activate([
      notes.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -12),
      notes.topAnchor.constraint(equalTo: root.topAnchor, constant: 12),
      notes.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -12), inspectorWidth!,
      canvas.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 12),
      canvas.trailingAnchor.constraint(equalTo: notes.leadingAnchor, constant: -12),
      canvas.topAnchor.constraint(equalTo: root.topAnchor, constant: 44),
      canvas.bottomAnchor.constraint(equalTo: dock.topAnchor, constant: -12),
      gameView.centerXAnchor.constraint(equalTo: canvas.centerXAnchor),
      gameView.centerYAnchor.constraint(equalTo: canvas.centerYAnchor),
      gameView.widthAnchor.constraint(lessThanOrEqualTo: canvas.widthAnchor),
      gameView.heightAnchor.constraint(lessThanOrEqualTo: canvas.heightAnchor),
      gameView.widthAnchor.constraint(equalTo: gameView.heightAnchor, multiplier: 16 / 9), fitWidth,
      fitHeight,
      dock.centerXAnchor.constraint(equalTo: canvas.centerXAnchor),
      dock.bottomAnchor.constraint(equalTo: messageLabel.topAnchor, constant: -10),
      title.topAnchor.constraint(equalTo: root.topAnchor, constant: 18),
      title.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 18),
      messageLabel.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -12),
      messageLabel.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 18),
      messageLabel.trailingAnchor.constraint(lessThanOrEqualTo: notes.leadingAnchor, constant: -12),
    ])
  }
}
