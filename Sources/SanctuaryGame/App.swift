import AppKit
import MetalKit
import FieldCore

final class GameView:MTKView {
    weak var controller:AppController?
    private var lastDragPoint:NSPoint?
    override var acceptsFirstResponder:Bool {true}
    override func mouseDown(with event:NSEvent) {window?.makeFirstResponder(self);lastDragPoint=event.locationInWindow}
    override func rightMouseDown(with event:NSEvent) {mouseDown(with:event)}
    override func mouseDragged(with event:NSEvent) {
        let p=event.locationInWindow
        if let previous=lastDragPoint {controller?.renderer?.look(dx:Float(p.x-previous.x),dy:Float(previous.y-p.y))}
        lastDragPoint=p
    }
    override func mouseUp(with event:NSEvent) {lastDragPoint=nil}
    override func rightMouseUp(with event:NSEvent) {lastDragPoint=nil}
    override func rightMouseDragged(with event:NSEvent) {mouseDragged(with:event)}
    override func scrollWheel(with event:NSEvent) {
        if let r=controller?.renderer,r.scene=="studio" {r.orbitDistance=r.boundedStudioDistance(r.orbitDistance*exp(Float(event.scrollingDeltaY)*0.025))}
    }
    override func keyDown(with event:NSEvent) {
        guard let c=controller,let r=c.renderer else {return}
        switch event.keyCode {
        case 48: c.toggleScene()
        case 3: c.bridge?.capture()
        case 15: r.resetCamera()
        case 35: r.paused.toggle();r.keys.removeAll()
        case 18: r.lighting="morning"
        case 19: r.lighting="sunset"
        case 20: r.lighting="overcast"
        case 4: c.toggleNotes()
        case 123: r.look(dx:-35,dy:0)
        case 124: r.look(dx:35,dy:0)
        case 125: r.look(dx:0,dy:25)
        case 126: r.look(dx:0,dy:-25)
        case 53: r.keys.removeAll();window?.makeFirstResponder(self)
        default:
            // A computer-use key tap can begin and end between two fixed ticks.
            // Give its initial press a small step; held keys still move at dt speed.
            if !event.isARepeat && !r.keys.contains(event.keyCode) && !r.paused {
                let tap:[UInt16:V3]=[13:V3(0,0,0.12),1:V3(0,0,-0.12),0:V3(-0.12,0,0),2:V3(0.12,0,0)]
                if let movement=tap[event.keyCode] {r.move(local:movement)}
            }
            r.keys.insert(event.keyCode)
        }
        c.updateHUD()
    }
    override func keyUp(with event:NSEvent) {controller?.renderer?.keys.remove(event.keyCode)}
    override func flagsChanged(with event:NSEvent) {
        if event.modifierFlags.contains(.shift) {controller?.renderer?.keys.insert(56)} else {controller?.renderer?.keys.remove(56)}
    }
}

final class ActionButton:NSButton {
    var actionBlock:(()->Void)?
    init(_ title:String,action:@escaping ()->Void) {
        super.init(frame:.zero);self.title=title;actionBlock=action
        target=self;self.action=#selector(invoke);bezelStyle = .rounded
        font=NSFont.systemFont(ofSize:12,weight:.medium);controlSize = .regular
        setAccessibilityLabel(title)
    }
    required init?(coder:NSCoder) {fatalError()}
    @objc func invoke() {actionBlock?()}
}

final class AppController:NSObject,NSApplicationDelegate,NSWindowDelegate {
    var soundstageMode:Bool {Bundle.main.object(forInfoDictionaryKey:"SanctuaryMode") as? String == "soundstage"}
    var window:NSWindow!
    var gameView:GameView!
    var renderer:GardenRenderer?
    var bridge:AgentBridge?
    var titleLabel:NSTextField!
    var subtitleLabel:NSTextField!
    var statsLabel:NSTextField!
    var messageLabel:NSTextField!
    var sceneButton:ActionButton!
    var notes:NSView!
    var scaleSlider:NSSlider!
    var scaleLabel:NSTextField!
    var rootURL:URL!
    var crosshair:NSTextField!
    var workshopPanel:WorkshopPanel?

    func applicationDidFinishLaunching(_ notification:Notification) {
        NSApp.setActivationPolicy(.regular)
        let menu=NSMenu();let appItem=NSMenuItem();menu.addItem(appItem)
        let appMenu=NSMenu();appMenu.addItem(withTitle:"Quit Sanctuary",action:#selector(NSApplication.terminate(_:)),keyEquivalent:"q");appItem.submenu=appMenu;NSApp.mainMenu=menu
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:1200,height:675),styleMask:[.titled,.closable,.miniaturizable,.resizable],backing:.buffered,defer:false)
        window.title=soundstageMode ? "Sanctuary · Soundstage":"Sanctuary · Field Garden";window.contentAspectRatio=NSSize(width:16,height:9);window.minSize=NSSize(width:960,height:560)
        window.center();window.delegate=self
        gameView=GameView(frame:window.contentView!.bounds);gameView.controller=self;gameView.autoresizingMask=[.width,.height]
        gameView.setAccessibilityLabel("Field garden. W A S D to walk. Drag to look. Tab switches object studio.")
        window.contentView=gameView
        buildHUD()
        window.makeKeyAndOrderFront(nil);window.makeFirstResponder(gameView)
        NSApp.activate(ignoringOtherApps:true)
        rootURL=URL(fileURLWithPath:ProcessInfo.processInfo.environment["SANCTUARY_WORKSPACE"] ?? Bundle.main.object(forInfoDictionaryKey:"SanctuaryWorkspace") as? String ?? FileManager.default.currentDirectoryPath).appendingPathComponent(soundstageMode ? ".soundstage":".sanctuary")
        do {
            let r=try GardenRenderer(view:gameView);renderer=r
            bridge=try AgentBridge(renderer:r,root:rootURL)
            bridge?.onMessage={[weak self] message in self?.messageLabel.stringValue=message}
            r.onUpdate={[weak self] in self?.updateHUD()}
            if soundstageMode {
                let saved=rootURL.appendingPathComponent("last-study.json")
                if FileManager.default.fileExists(atPath:saved.path) {
                    do {try r.restoreWorkshop(JSONDecoder().decode(WorkshopDocument.self,from:Data(contentsOf:saved)));messageLabel.stringValue="Restored your last workshop session."}
                    catch {messageLabel.stringValue="Last study could not be restored: \(error.localizedDescription)"}
                }
            }
            updateHUD()
        } catch {
            messageLabel.stringValue="Unable to start: \(error.localizedDescription)"
            fputs("Sanctuary startup failed: \(error)\n",stderr)
            let alert=NSAlert();alert.messageText="Sanctuary could not start";alert.informativeText=error.localizedDescription;alert.runModal()
            NSApp.terminate(nil)
        }
    }

    func label(_ string:String,size:CGFloat,weight:NSFont.Weight = .regular,color:NSColor = .white) -> NSTextField {
        let l=NSTextField(labelWithString:string);l.font=NSFont.systemFont(ofSize:size,weight:weight);l.textColor=color
        l.translatesAutoresizingMaskIntoConstraints=false;return l
    }
    func panel() -> NSVisualEffectView {
        let p=NSVisualEffectView();p.material = .hudWindow;p.blendingMode = .withinWindow;p.state = .active
        p.wantsLayer=true;p.layer?.cornerRadius=15;p.layer?.masksToBounds=true;p.translatesAutoresizingMaskIntoConstraints=false
        p.appearance=NSAppearance(named:.darkAqua);return p
    }
    func buildHUD() {
        if soundstageMode {buildWorkshopHUD();return}
        let header=panel();gameView.addSubview(header)
        let eyebrow=label("S A N C T U A R Y",size:10,weight:.semibold,color:NSColor(calibratedRed:0.83,green:0.87,blue:0.68,alpha:1))
        titleLabel=label("The field garden",size:25,weight:.medium)
        titleLabel.font=NSFont(name:"NewYork-Regular",size:25) ?? NSFont.systemFont(ofSize:25,weight:.medium)
        subtitleLabel=label("A small beginning for a living world",size:11,color:.init(white:0.85,alpha:1))
        let headings=NSStackView(views:[eyebrow,titleLabel,subtitleLabel]);headings.orientation = .vertical;headings.alignment = .leading;headings.spacing=5
        headings.translatesAutoresizingMaskIntoConstraints=false;header.addSubview(headings)
        NSLayoutConstraint.activate([header.leadingAnchor.constraint(equalTo:gameView.leadingAnchor,constant:24),header.topAnchor.constraint(equalTo:gameView.topAnchor,constant:24),headings.leadingAnchor.constraint(equalTo:header.leadingAnchor,constant:18),headings.trailingAnchor.constraint(equalTo:header.trailingAnchor,constant:-22),headings.topAnchor.constraint(equalTo:header.topAnchor,constant:14),headings.bottomAnchor.constraint(equalTo:header.bottomAnchor,constant:-16)])

        notes=panel();gameView.addSubview(notes)
        let noteTitle=label(soundstageMode ? "SOUNDSTAGE":"FIELD NOTES",size:10,weight:.semibold,color:.init(white:0.7,alpha:1))
        sceneButton=ActionButton("Open object studio") {[weak self] in self?.toggleScene()}
        let presets=NSStackView(views:[button("Noon"){[weak self] in self?.renderer?.lighting="morning"},button("Sunset"){[weak self] in self?.renderer?.lighting="sunset"},button("Cloud"){[weak self] in self?.renderer?.lighting="overcast"}]);presets.spacing=4
        let weather=NSStackView(views:[button("Rain"){[weak self] in self?.renderer?.lighting="rain"},button("Indoors"){[weak self] in self?.renderer?.lighting="indoor"},button("Wet / dry"){[weak self] in guard let r=self?.renderer else{return};r.wetness=r.wetness>0.5 ? 0:1}]);weather.spacing=4
        let subjects=NSStackView(views:[button("Seed"){[weak self] in try? self?.renderer?.selectSubject("seed")},button("Stone"){[weak self] in try? self?.renderer?.selectSubject("stone")},button("Branch"){[weak self] in try? self?.renderer?.selectSubject("branch")}]);subjects.spacing=4
        scaleLabel=label("Seed pod · scale 1.00",size:12)
        scaleSlider=NSSlider(value:1,minValue:0.6,maxValue:1.7,target:self,action:#selector(scaleChanged))
        scaleSlider.isContinuous=true;scaleSlider.setAccessibilityLabel("Seed pod scale")
        let cameraButtons=NSStackView(views:[button(soundstageMode ? "Front":"Arrival"){[weak self] in self?.setCamera("arrival")},button(soundstageMode ? "Quarter":"Meadow"){[weak self] in self?.setCamera("meadow")},button(soundstageMode ? "Above":"Gate"){[weak self] in self?.setCamera("gate")}]);cameraButtons.spacing=4
        let capture=button("Capture frame  ↗"){[weak self] in self?.bridge?.capture()}
        let reset=button("Reset view"){[weak self] in self?.renderer?.resetCamera()}
        statsLabel=label("Compiling the garden…",size:10,color:.init(white:0.76,alpha:1));statsLabel.maximumNumberOfLines=3
        let skyViews=NSStackView(views:[button("Sunward"){[weak self] in self?.renderer?.setStudyView("sunward")},button("Away"){[weak self] in self?.renderer?.setStudyView("away")},button("Zenith"){[weak self] in self?.renderer?.setStudyView("zenith")}]);skyViews.spacing=4
        let twilight=NSStackView(views:[button("Golden"){[weak self] in self?.renderer?.lighting="golden"},button("Afterglow"){[weak self] in self?.renderer?.lighting="afterglow"}]);twilight.spacing=4
        let stack=NSStackView(views:[noteTitle,sceneButton,label("LIGHT",size:9,weight:.medium,color:.gray),presets,twilight,weather,subjects,scaleLabel,scaleSlider,label(soundstageMode ? "VIEWS":"WAYPOINTS",size:9,weight:.medium,color:.gray),cameraButtons,skyViews,button("Live / pause"){[weak self] in self?.renderer?.paused.toggle()},capture,reset,statsLabel])
        stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=10;stack.translatesAutoresizingMaskIntoConstraints=false
        notes.addSubview(stack)
        NSLayoutConstraint.activate([notes.trailingAnchor.constraint(equalTo:gameView.trailingAnchor,constant:-24),notes.topAnchor.constraint(equalTo:gameView.topAnchor,constant:24),notes.widthAnchor.constraint(equalToConstant:244),stack.leadingAnchor.constraint(equalTo:notes.leadingAnchor,constant:16),stack.trailingAnchor.constraint(equalTo:notes.trailingAnchor,constant:-16),stack.topAnchor.constraint(equalTo:notes.topAnchor,constant:16),stack.bottomAnchor.constraint(equalTo:notes.bottomAnchor,constant:-16),scaleSlider.widthAnchor.constraint(equalToConstant:210)])

        let footer=panel();gameView.addSubview(footer)
        let controls=label(soundstageMode ? "Drag  orbit     Scroll  zoom     Tab  sky / object     P  live / pause     F  capture     H  notes":"W A S D  walk     ⇧  run     Drag / arrows  look     Tab  studio     F  capture     H  notes",size:11,weight:.medium)
        messageLabel=label(soundstageMode ? "A separate space for field, material, and sky studies.":"Follow the path toward the old gate.",size:10,color:.init(white:0.76,alpha:1))
        let footerStack=NSStackView(views:[controls,messageLabel]);footerStack.orientation = .vertical;footerStack.alignment = .leading;footerStack.spacing=5;footerStack.translatesAutoresizingMaskIntoConstraints=false
        footer.addSubview(footerStack)
        NSLayoutConstraint.activate([footer.leadingAnchor.constraint(equalTo:gameView.leadingAnchor,constant:24),footer.bottomAnchor.constraint(equalTo:gameView.bottomAnchor,constant:-20),footerStack.leadingAnchor.constraint(equalTo:footer.leadingAnchor,constant:16),footerStack.trailingAnchor.constraint(equalTo:footer.trailingAnchor,constant:-16),footerStack.topAnchor.constraint(equalTo:footer.topAnchor,constant:12),footerStack.bottomAnchor.constraint(equalTo:footer.bottomAnchor,constant:-12)])
        crosshair=label("·",size:24,weight:.light,color:.init(white:1,alpha:0.65));gameView.addSubview(crosshair)
        NSLayoutConstraint.activate([crosshair.centerXAnchor.constraint(equalTo:gameView.centerXAnchor),crosshair.centerYAnchor.constraint(equalTo:gameView.centerYAnchor)])
    }
    func button(_ title:String,action:@escaping ()->Void) -> ActionButton {
        ActionButton(title) {[weak self] in action();self?.updateHUD();self?.window.makeFirstResponder(self?.gameView)}
    }
    @objc func scaleChanged() {renderer?.podScale=scaleSlider.floatValue;updateHUD();window.makeFirstResponder(gameView)}
    func toggleScene() {guard let r=renderer else {return};if soundstageMode {r.scene="studio";try? r.selectSubject(r.studioObject=="sky" ? r.studioSource.id:"sky");updateHUD();return};r.scene=r.scene=="garden" ? "studio":"garden";r.keys.removeAll();updateHUD();window.makeFirstResponder(gameView)}
    func toggleNotes() {notes.isHidden.toggle()}
    func setCamera(_ name:String) {
        guard let r=renderer else {return}
        if soundstageMode {r.setStudyView(["arrival":"front","meadow":"quarter","gate":"above"][name] ?? "front");updateHUD();return}
        r.scene="garden"
        switch name {
        case "meadow":r.camera=V3(-13,0,-12);r.yaw = -0.18;r.pitch = -0.03
        case "gate":r.camera=V3(r.world.terrain.pathX(-48),0,-48);r.yaw=0;r.pitch=0.12
        default:r.resetCamera()
        }
        r.camera.y=r.world.terrain.height(r.camera.x,r.camera.z)+1.72
        updateHUD()
    }
    func updateHUD() {
        guard let r=renderer else {return}
        if soundstageMode {workshopPanel?.refresh();return}
        let studio=r.scene=="studio"
        crosshair.isHidden=studio
        titleLabel.stringValue=studio ? (r.studioObject=="sky" ? "Atmosphere studies":"Material studies"):"The field garden"
        subtitleLabel.stringValue=studio ? "One field · many ways of seeing":"A small beginning for a living world"
        sceneButton.title=soundstageMode ? (r.studioObject=="sky" ? "Return to object":"Study the sky") : (studio ? "Return to the garden":"Open object studio")
        sceneButton.setAccessibilityLabel(sceneButton.title)
        scaleLabel.stringValue=String(format:"Seed pod · %.0f mm tall",GardenWorld.podMetres*r.podScale*1000);scaleSlider.floatValue=r.podScale
        let gpu=r.stats(r.gpuTimes)["median"] ?? 0
        statsLabel.stringValue=String(format:"1920 × 1080 · native Metal\nGPU %.1f ms · %@ · %@",gpu,r.lighting,r.paused ? "paused":"live")
    }
    func windowDidResignKey(_ notification:Notification) {renderer?.keys.removeAll()}
    func applicationShouldTerminateAfterLastWindowClosed(_ sender:NSApplication)->Bool {true}
    func applicationWillTerminate(_ notification:Notification) {
        renderer?.view.isPaused=true
        if soundstageMode,let renderer,let rootURL {
            let encoder=JSONEncoder();encoder.outputFormatting = [.prettyPrinted,.sortedKeys]
            try? encoder.encode(renderer.workshopDocument()).write(to:rootURL.appendingPathComponent("last-study.json"),options:.atomic)
        }
        if let bridge {bridge.write(["stopped":true,"sessionPID":ProcessInfo.processInfo.processIdentifier],to:bridge.root.appendingPathComponent("status.json"))}
    }
}

@main
enum SanctuaryMain {
    static func main() {
        if CommandLine.arguments.contains("--check-renderer") {
            do {let r=try GardenRenderer(view:MTKView());print("Renderer compiled on \(r.device.name)")} catch {fputs("\(error)\n",stderr);exit(1)}
            return
        }
        let app=NSApplication.shared
        let delegate=AppController();app.delegate=delegate
        withExtendedLifetime(delegate) {app.run()}
    }
}
