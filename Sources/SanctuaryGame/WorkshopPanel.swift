import AppKit
import FieldCore

final class WorkshopPopup:NSPopUpButton {
    var changed:((String)->Void)?
    init(_ name:String,_ choices:[String],_ changed:@escaping(String)->Void) {
        super.init(frame:.zero,pullsDown:false);addItems(withTitles:choices);self.changed=changed
        target=self;action=#selector(selectValue);setAccessibilityLabel(name);font = .systemFont(ofSize:12)
    }
    required init?(coder:NSCoder){fatalError()}
    @objc func selectValue(){if let value=titleOfSelectedItem {changed?(value)}}
}
final class WorkshopSlider:NSSlider {
    var changed:((Float)->Void)?
    init(_ name:String,_ value:Float,_ range:ClosedRange<Float>,_ changed:@escaping(Float)->Void) {
        super.init(frame:.zero);minValue=Double(range.lowerBound);maxValue=Double(range.upperBound);floatValue=value
        self.changed=changed;target=self;action=#selector(change);isContinuous=false;setAccessibilityLabel(name)
    }
    required init?(coder:NSCoder){fatalError()}
    @objc func change(){changed?(floatValue)}
}
final class WorkshopDocumentView:NSView {override var isFlipped:Bool {true}}
final class WorkshopPanel:NSView {
    weak var owner:AppController?
    var sliders:[String:WorkshopSlider]=[:]
    var values:[String:NSTextField]=[:]
    var subject:WorkshopPopup!
    var rig:WorkshopPopup!
    var sky:WorkshopPopup!
    var arrangement:WorkshopPopup!
    var shapeGroup:NSStackView!
    var rigGroup:NSStackView!
    var identity:NSTextField!
    var metrics:NSTextField!
    var live:ActionButton!
    var spin:ActionButton!
    var marker:ActionButton!
    var ground:ActionButton!
    init(_ owner:AppController) {
        self.owner=owner;super.init(frame:.zero);translatesAutoresizingMaskIntoConstraints=false
        wantsLayer=true;layer?.backgroundColor=NSColor(calibratedWhite:0.09,alpha:0.94).cgColor;layer?.cornerRadius=12
        appearance=NSAppearance(named:.darkAqua)
        let scroll=NSScrollView();scroll.translatesAutoresizingMaskIntoConstraints=false;scroll.hasVerticalScroller=true;scroll.drawsBackground=false
        let stack=NSStackView();stack.orientation = .vertical;stack.alignment = .leading;stack.spacing=9;stack.translatesAutoresizingMaskIntoConstraints=false
        let document=WorkshopDocumentView();document.translatesAutoresizingMaskIntoConstraints=false;document.addSubview(stack);scroll.documentView=document;addSubview(scroll)
        NSLayoutConstraint.activate([scroll.leadingAnchor.constraint(equalTo:leadingAnchor,constant:14),scroll.trailingAnchor.constraint(equalTo:trailingAnchor,constant:-10),scroll.topAnchor.constraint(equalTo:topAnchor,constant:14),scroll.bottomAnchor.constraint(equalTo:bottomAnchor,constant:-14),document.widthAnchor.constraint(equalTo:scroll.contentView.widthAnchor),stack.leadingAnchor.constraint(equalTo:document.leadingAnchor),stack.trailingAnchor.constraint(equalTo:document.trailingAnchor,constant:-4),stack.topAnchor.constraint(equalTo:document.topAnchor),stack.bottomAnchor.constraint(equalTo:document.bottomAnchor)])
        func label(_ name:String)->NSTextField {owner.label(name,size:10,weight:.semibold,color:.init(white:0.65,alpha:1))}
        func row(_ views:[NSView])->NSStackView {let r=NSStackView(views:views);r.spacing=5;return r}
        func button(_ name:String,_ action:@escaping(GardenRenderer)throws->Void)->ActionButton {
            ActionButton(name){[weak self] in self?.perform(action)}
        }
        func slider(_ title:String,_ key:String,_ value:Float,_ range:ClosedRange<Float>,_ action:@escaping(GardenRenderer,Float)throws->Void)->NSStackView {
            let text=owner.label(title,size:11),number=owner.label("",size:10,color:.lightGray)
            let slider=WorkshopSlider(title,value,range){[weak self] value in self?.perform{try action($0,value)}}
            sliders[key]=slider;values[key]=number
            let group=NSStackView(views:[row([text,number]),slider]);group.orientation = .vertical;group.alignment = .leading;group.spacing=1
            slider.widthAnchor.constraint(equalToConstant:246).isActive=true
            return group
        }
        stack.addArrangedSubview(label("SUBJECT"))
        subject=WorkshopPopup("Subject",AssetSource.availableIDs+["sky"]){[weak self] name in self?.perform{try $0.selectSubject(name)}};stack.addArrangedSubview(subject)
        identity=owner.label("",size:11);identity.maximumNumberOfLines=3;stack.addArrangedSubview(identity)
        stack.addArrangedSubview(row([button("Save asset"){r in let url=try r.saveAsset();owner.messageLabel.stringValue="Saved source · \(url.lastPathComponent)"},button("Reload source"){r in try r.applySource(AssetSource.read(r.studioSourceURL))}]))
        shapeGroup=NSStackView();shapeGroup.orientation = .vertical;shapeGroup.alignment = .leading;shapeGroup.spacing=6
        for (key,title) in [("height","Growth height · m"),("width","Crown width · m"),("trunk","Trunk radius · m"),("spread","Branch spread"),("leaves","Leaf count")] {
            shapeGroup.addArrangedSubview(slider(title,"shape."+key,AssetSource.defaults("tree").parameters[key]!,AssetSource.treeRanges[key]!){r,v in try r.editShape([key:key=="leaves" ? v.rounded():v])})
        }
        stack.addArrangedSubview(shapeGroup)
        arrangement=WorkshopPopup("Arrangement",["single","grove"]){[weak self] value in self?.perform{$0.studioLayout.arrangement=value;$0.fitStudioCamera()}};stack.addArrangedSubview(arrangement)
        marker=button("Scale reference: off"){$0.studioLayout.reference.toggle();$0.fitStudioCamera()};ground=button("Ground: on"){$0.studioLayout.ground.toggle()};stack.addArrangedSubview(row([marker,ground]))
        stack.addArrangedSubview(slider("Subject scale","scale",1,0.25...4){$0.setStudioScale($1)})
        stack.addArrangedSubview(label("LIGHTING"))
        rig=WorkshopPopup("Lighting rig",StudioRig.names){[weak self] value in self?.perform{$0.studioRig=StudioRig.preset(value)}};stack.addArrangedSubview(rig)
        sky=WorkshopPopup("Outdoor sky",["noon","golden","sunset","afterglow","overcast","rain"]){[weak self] value in self?.perform{$0.lighting=value}};stack.addArrangedSubview(sky)
        rigGroup=NSStackView();rigGroup.orientation = .vertical;rigGroup.alignment = .leading;rigGroup.spacing=5
        for (key,title,range) in [("x","Light horizontal",Float(-4)...4),("y","Light height",Float(0.1)...4),("z","Light depth",Float(-4)...4),("intensity","Light intensity",Float(0)...40),("size","Source size",Float(0.005)...1),("warmth","Warmth",Float(0)...1),("fill","Ambient fill",Float(0)...0.5)] {
            rigGroup.addArrangedSubview(slider(title,"rig."+key,0,range){r,v in
                switch key {case "x":r.studioRig.x=v;case "y":r.studioRig.y=v;case "z":r.studioRig.z=v;case "intensity":r.studioRig.intensity=v;case "size":r.studioRig.size=v;case "warmth":r.studioRig.warmth=v;default:r.studioRig.fill=v}
            })
        }
        rigGroup.addArrangedSubview(owner.label("Position and size follow subject extent.",size:10,color:.gray));stack.addArrangedSubview(rigGroup)
        stack.addArrangedSubview(label("INSPECT"))
        for names in [["front","quarter","back"],["left","right","above"],["base","detail","crown"],["sunward","away","zenith"]] {stack.addArrangedSubview(row(names.map{name in button(name.capitalized){$0.setStudyView(name)}}))}
        stack.addArrangedSubview(row([button("Fit subject"){$0.studioObject=$0.studioSource.id;$0.studioLayout.focus=0.5;$0.resetCamera()},button("Use asset material"){$0.studioLayout.roughness = -1;$0.studioLayout.metallic = -1;$0.studioLayout.tint=1;$0.wetness=0}]))
        stack.addArrangedSubview(slider("Roughness","roughness",0.8,0.05...1){r,v in r.studioLayout.roughness=v})
        stack.addArrangedSubview(slider("Metallic","metallic",0,0...1){r,v in r.studioLayout.metallic=v})
        stack.addArrangedSubview(slider("Wetness","wetness",0,0...1){$0.wetness=$1})
        stack.addArrangedSubview(slider("Exposure","exposure",1,0.5...1.5){$0.exposure=$1})
        stack.addArrangedSubview(slider("Wind","wind",1,0...2){$0.wind=$1})
        live=button("Play"){$0.paused.toggle()};spin=button("Turntable: off"){$0.studioLayout.turntable.toggle()};stack.addArrangedSubview(row([live,spin,button("Step 1s"){r in r.paused=true;for _ in 0..<60 {r.step(1/60)}}]))
        stack.addArrangedSubview(row([ActionButton("Capture"){owner.bridge?.capture()},ActionButton("Save study"){[weak self] in self?.study(save:true)},ActionButton("Load study"){[weak self] in self?.study(save:false)}]))
        metrics=owner.label("",size:10,color:.lightGray);metrics.maximumNumberOfLines=4;stack.addArrangedSubview(metrics)
    }
    required init?(coder:NSCoder){fatalError()}
    func perform(_ action:(GardenRenderer)throws->Void) {
        guard let r=owner?.renderer else{return}
        do {try action(r)} catch {owner?.messageLabel.stringValue=error.localizedDescription}
        refresh();owner?.window.makeFirstResponder(owner?.gameView)
    }
    func study(save:Bool) {
        guard let owner else{return}
        if save {
            let panel=NSSavePanel();panel.nameFieldStringValue="tree-study.json";panel.directoryURL=owner.rootURL.appendingPathComponent("studies")
            panel.beginSheetModal(for:owner.window){response in if response == .OK,let url=panel.url {self.perform{r in let e=JSONEncoder();e.outputFormatting = [.prettyPrinted,.sortedKeys];try e.encode(r.workshopDocument()).write(to:url,options:.atomic);owner.messageLabel.stringValue="Saved study · \(url.lastPathComponent)"}}}
        } else {
            let panel=NSOpenPanel();panel.directoryURL=owner.rootURL.appendingPathComponent("studies");panel.allowsMultipleSelection=false
            panel.beginSheetModal(for:owner.window){response in if response == .OK,let url=panel.url {self.perform{r in try r.restoreWorkshop(JSONDecoder().decode(WorkshopDocument.self,from:Data(contentsOf:url)))}}}
        }
    }
    func refresh() {
        guard let r=owner?.renderer else{return}
        for id in AssetSource.availableIDs where !subject.itemTitles.contains(id) {subject.addItem(withTitle:id)}
        if !subject.itemTitles.contains(r.studioObject) {subject.addItem(withTitle:r.studioObject)}
        subject.selectItem(withTitle:r.studioObject);rig.selectItem(withTitle:r.studioRig.name);sky.selectItem(withTitle:r.lighting=="morning" ? "noon":r.lighting);arrangement.selectItem(withTitle:r.studioLayout.arrangement)
        shapeGroup.isHidden = !(r.studioSource.generator=="tree" || r.studioSource.generator=="branch") || r.studioObject=="sky"
        rigGroup.isHidden=r.studioRig.name=="outdoor";sky.isEnabled=r.studioRig.name=="outdoor"
        identity.stringValue=r.studioObject=="sky" ? "Atmosphere · no subject":String(format:"%@\n%.3g m tall · %.3g m wide",r.studioSource.name,r.studioHeight,(r.studioBounds.max.x-r.studioBounds.min.x)*r.studioLayout.scale)
        var map=["scale":r.studioLayout.scale,"roughness":r.studioLayout.roughness,"metallic":r.studioLayout.metallic,"wetness":r.wetness,"wind":r.wind,"exposure":r.exposure]
        for (key,value) in r.studioSource.parameters {map["shape."+key]=value}
        for (key,value) in ["x":r.studioRig.x,"y":r.studioRig.y,"z":r.studioRig.z,"intensity":r.studioRig.intensity,"size":r.studioRig.size,"warmth":r.studioRig.warmth,"fill":r.studioRig.fill] {map["rig."+key]=value}
        for (key,value) in map {
            let inherited=value<0 && (key=="roughness" || key=="metallic")
            sliders[key]?.floatValue=inherited ? (key=="roughness" ? 0.8:0):value
            values[key]?.stringValue=inherited ? "Asset":String(format:"%.2f",value)
        }
        live.title=r.paused ? "Play":"Pause";spin.title=r.studioLayout.turntable ? "Turntable: on":"Turntable: off";marker.title=r.studioLayout.reference ? "Scale reference: on":"Scale reference: off";ground.isEnabled=r.studioRig.name != "room";ground.title=r.studioRig.name=="room" ? "Room floor":(r.studioLayout.ground ? "Ground: on":"Ground: off")
        for control in [live,spin,marker,ground] {control?.setAccessibilityLabel(control?.title)}
        metrics.stringValue=String(format:"1080p · GPU %.1f ms · %d triangles\nLast field build %.0f ms · time %.1f s",r.stats(r.gpuTimes)["median"] ?? 0,r.visibleTriangles,r.studioBuildMilliseconds,r.simulationTime)
    }
}
extension AppController {
    func buildWorkshopHUD() {
        workshopPanel=WorkshopPanel(self);notes=workshopPanel;gameView.addSubview(notes)
        messageLabel=label("Drag to orbit · Scroll to zoom · R to fit · P to play · F to capture",size:11,color:.white)
        messageLabel.maximumNumberOfLines=2;gameView.addSubview(messageLabel)
        let title=label("S A N C T U A R Y  /  WORKSHOP",size:12,weight:.semibold);gameView.addSubview(title)
        NSLayoutConstraint.activate([notes.trailingAnchor.constraint(equalTo:gameView.trailingAnchor,constant:-16),notes.topAnchor.constraint(equalTo:gameView.topAnchor,constant:16),notes.bottomAnchor.constraint(equalTo:gameView.bottomAnchor,constant:-16),notes.widthAnchor.constraint(equalToConstant:290),title.topAnchor.constraint(equalTo:gameView.topAnchor,constant:22),title.leadingAnchor.constraint(equalTo:gameView.leadingAnchor,constant:22),messageLabel.bottomAnchor.constraint(equalTo:gameView.bottomAnchor,constant:-18),messageLabel.leadingAnchor.constraint(equalTo:gameView.leadingAnchor,constant:22),messageLabel.trailingAnchor.constraint(lessThanOrEqualTo:notes.leadingAnchor,constant:-12)])
    }
}
