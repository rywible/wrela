import Foundation
import FieldCore

/// A local, inspectable file protocol. Commands are atomic JSON files; every ID
/// gets a durable acknowledgement. Captures acknowledge only after GPU readback.
final class AgentBridge {
    let renderer:GardenRenderer
    let root:URL
    let inbox:URL
    let outbox:URL
    let captures:URL
    var timer:Timer?
    var onMessage:((String)->Void)?
    var activeIDs=Set<String>()

    init(renderer:GardenRenderer,root:URL) throws {
        self.renderer=renderer;self.root=root
        inbox=root.appendingPathComponent("inbox");outbox=root.appendingPathComponent("outbox");captures=root.appendingPathComponent("captures")
        for url in [root,inbox,outbox,captures] {try FileManager.default.createDirectory(at:url,withIntermediateDirectories:true)}
        // Commands belong to a session; an old pending command is rejected explicitly.
        let old=(try? FileManager.default.contentsOfDirectory(at:inbox,includingPropertiesForKeys:nil)) ?? []
        for url in old where url.pathExtension=="json" {
            if let data=try? Data(contentsOf:url),let c=(try? JSONSerialization.jsonObject(with:data)) as? [String:Any],let id=c["id"] as? String {
                respond(id,["ok":false,"error":"Session restarted before command completed"])
            }
            try? FileManager.default.removeItem(at:url)
        }
        publish()
        timer=Timer.scheduledTimer(withTimeInterval:0.1,repeats:true) { [weak self] _ in self?.poll() }
    }

    func publish() {
        var s=renderer.snapshot();s["heartbeat"]=Date().timeIntervalSince1970
        s["protocolVersion"]=1;s["captureDirectory"]=captures.path
        write(s,to:root.appendingPathComponent("status.json"))
    }
    func write(_ value:[String:Any],to url:URL) {
        do {try JSONSerialization.data(withJSONObject:value,options:[.prettyPrinted,.sortedKeys]).write(to:url,options:.atomic)}
        catch {onMessage?("Agent report failed: \(error.localizedDescription)")}
    }
    func respond(_ id:String,_ value:[String:Any]) {
        if value["ok"] as? Bool == true {renderer.workshopSession.commit()} else {renderer.workshopSession.pending=nil}
        var result=value;if result["state"] != nil {result["state"]=renderer.snapshot()};result["id"]=id;result["frame"]=renderer.frame
        write(result,to:outbox.appendingPathComponent(id+".json"));activeIDs.remove(id)
    }
    func poll() {
        let files=((try? FileManager.default.contentsOfDirectory(at:inbox,includingPropertiesForKeys:nil)) ?? []).filter{$0.pathExtension=="json"}.sorted{$0.lastPathComponent<$1.lastPathComponent}
        for url in files {
            do {
                let data=try Data(contentsOf:url)
                guard let c=try JSONSerialization.jsonObject(with:data) as? [String:Any],let id=c["id"] as? String,
                      UUID(uuidString:id) != nil else {throw RuntimeError.message("Command requires a UUID id")}
                guard !activeIDs.contains(id) else {continue}
                activeIDs.insert(id)
                try FileManager.default.removeItem(at:url)
                do {try execute(c,id:id)} catch {respond(id,["ok":false,"error":error.localizedDescription])}
            } catch {onMessage?("Invalid agent command: \(error.localizedDescription)");try? FileManager.default.removeItem(at:url)}
        }
        publish()
    }
    func number(_ c:[String:Any],_ key:String,_ fallback:Float) throws -> Float {
        guard let value=c[key] else {return fallback}
        guard let n=value as? NSNumber,n.floatValue.isFinite else {throw RuntimeError.message("\(key) must be finite")}
        return n.floatValue
    }
    func execute(_ c:[String:Any],id:String) throws {
        guard let action=c["action"] as? String else {throw RuntimeError.message("Missing action")}
        let edits:Set<String>=["studioObject","subject","rig","shape","layout","assetLoad","assetSave","assetReload","loadStudy","lighting","view","sky","reset","camera","pause","step","parameters","look"]
        if edits.contains(action) {renderer.workshopSession.commit();renderer.workshopSession.begin(action)}
        switch action {
        case "undo":try renderer.workshopSession.undo()
        case "redo":try renderer.workshopSession.redo()
        case "watchSource":
            guard let value=c["value"] as? Bool else {throw RuntimeError.message("watchSource requires a boolean")}
            renderer.workshopSession.watching=value;renderer.workshopSession.resetWatch()
        case "automation":
            guard let value=c["value"] as? Bool else {throw RuntimeError.message("automation requires a boolean")}
            renderer.workshopSession.commit();renderer.workshopSession.automating=value
            if !value {renderer.workshopSession.resetWatch()}
        case "checkpoint":
            guard let name=c["name"] as? String else {throw RuntimeError.message("Provide a checkpoint name")}
            respond(id,["ok":true,"path":try renderer.workshopSession.checkpoint(name).path]);return
        case "restoreCheckpoint":
            guard let name=c["name"] as? String else {throw RuntimeError.message("Provide a checkpoint name")}
            try renderer.workshopSession.restoreCheckpoint(name)
        case "pinBaseline":
            guard let name=c["name"] as? String else {throw RuntimeError.message("Provide a baseline name")}
            respond(id,["ok":true,"path":try renderer.workshopSession.pinBaseline(name).path]);return
        case "selectBaseline":
            guard let name=c["name"] as? String else {throw RuntimeError.message("Provide a baseline name")}
            try renderer.workshopSession.selectBaseline(name)
        case "captureReview":try renderer.workshopSession.captureReview()
        case "styleBoard":try renderer.workshopSession.captureReview(styleBoard:true)
        case "publishLook":try renderer.sceneLook.publish()
        case "look":
            var values:[String:Float]=[:]
            for key in c.keys where key != "action" && key != "id" {values[key]=try number(c,key,0)}
            try renderer.sceneLook.set(values)
        case "status": break
        case "scene":
            guard let s=c["value"] as? String,["garden","studio"].contains(s) else {throw RuntimeError.message("Scene must be garden or studio")}
            guard !renderer.isSoundstage || s=="studio" else {throw RuntimeError.message("The soundstage contains no garden; use Sanctuary for playtesting")}
            renderer.scene=s;renderer.keys.removeAll()
        case "studioObject","subject":
            guard let name=c["value"] as? String else {throw RuntimeError.message("Subject requires a name")}
            try renderer.selectSubject(name)
        case "catalog":respond(id,["ok":true,"catalog":GardenRenderer.workshopCatalog]);return
        case "rig":
            var rig=renderer.studioRig
            if let name=c["value"] as? String {guard StudioRig.names.contains(name) else {throw RuntimeError.message("Unknown rig")};rig=StudioRig.preset(name)}
            rig.x=try number(c,"x",rig.x);rig.y=try number(c,"y",rig.y);rig.z=try number(c,"z",rig.z)
            rig.intensity=try number(c,"intensity",rig.intensity);rig.size=try number(c,"size",rig.size);rig.warmth=try number(c,"warmth",rig.warmth);rig.fill=try number(c,"fill",rig.fill)
            try rig.validate();renderer.studioRig=rig
        case "shape":
            var values:[String:Float]=[:]
            for key in c.keys where key != "action" && key != "id" {guard AssetSource.treeRanges[key] != nil else {throw RuntimeError.message("Unknown shape parameter \(key)")};values[key]=try number(c,key,0)}
            try renderer.editShape(values)
        case "layout":
            var layout=renderer.studioLayout
            if let name=c["arrangement"] as? String {layout.arrangement=name}
            layout.scale=try number(c,"scale",layout.scale);layout.focus=try number(c,"focus",layout.focus);layout.roughness=try number(c,"roughness",layout.roughness);layout.metallic=try number(c,"metallic",layout.metallic);layout.tint=try number(c,"tint",layout.tint)
            for key in ["reference","ground","turntable"] where c[key] != nil {
                guard let value=c[key] as? Bool else {throw RuntimeError.message("\(key) requires a boolean")}
                switch key {case "reference":layout.reference=value;case "ground":layout.ground=value;default:layout.turntable=value}
            }
            try layout.validate();let distance=renderer.orbitDistance*renderer.studioUnit;renderer.studioLayout=layout;renderer.orbitDistance=renderer.boundedStudioDistance(distance/renderer.studioUnit)
        case "assetLoad":
            guard let path=c["path"] as? String else {throw RuntimeError.message("Provide an asset source path")}
            let url=path.hasPrefix("/") ? URL(fileURLWithPath:path):AssetSource.workspace.appendingPathComponent(path)
            try renderer.applySource(AssetSource.read(url));renderer.importedSourceURL=url;renderer.workshopSession.resetWatch();renderer.resetCamera()
        case "assetSave":
            respond(id,["ok":true,"path":try renderer.saveAsset().path,"state":renderer.snapshot()]);return
        case "assetReload":try renderer.applySource(AssetSource.read(renderer.studioSourceURL))
        case "saveStudy","loadStudy":
            guard let path=c["path"] as? String,!path.isEmpty else {throw RuntimeError.message("Provide a study path")}
            let url=path.hasPrefix("/") ? URL(fileURLWithPath:path):root.appendingPathComponent("studies").appendingPathComponent(path)
            if action=="saveStudy" {
                try FileManager.default.createDirectory(at:url.deletingLastPathComponent(),withIntermediateDirectories:true)
                let encoder=JSONEncoder();encoder.outputFormatting = [.prettyPrinted,.sortedKeys]
                try encoder.encode(renderer.workshopDocument()).write(to:url,options:.atomic)
            } else {
                let data=try Data(contentsOf:url);guard data.count<1048576 else {throw RuntimeError.message("Study is too large")}
                try renderer.restoreWorkshop(JSONDecoder().decode(WorkshopDocument.self,from:data))
            }
            respond(id,["ok":true,"path":url.path,"state":renderer.snapshot()]);return
        case "reloadAssets":
            let candidate=try GardenWorld(soundstageOnly:renderer.isSoundstage)
            let uploaded=candidate.batches.map(renderer.upload)
            renderer.world=candidate;renderer.batches=uploaded
            if renderer.scene=="studio",FileManager.default.fileExists(atPath:renderer.studioSourceURL.path) {try renderer.applySource(AssetSource.read(renderer.studioSourceURL))}
        case "profiling":
            guard let value=c["value"] as? Bool else {throw RuntimeError.message("Profiling requires a boolean")}
            renderer.profiling=value;renderer.clearMetrics()
        case "meshlets":
            guard let value=c["value"] as? Bool else {throw RuntimeError.message("Meshlets requires a boolean")}
            renderer.useMeshlets=value;renderer.clearMetrics()
        case "lighting":
            guard let s=c["value"] as? String,["morning","noon","golden","sunset","afterglow","overcast","rain","indoor"].contains(s) else {throw RuntimeError.message("Unknown lighting preset")}
            renderer.lighting=s
        case "view":
            guard let name=c["value"] as? String,["front","quarter","back","left","right","above","detail","base","crown","sunward","away","zenith"].contains(name) else {throw RuntimeError.message("Unknown view")}
            renderer.setStudyView(name)
        case "sky":
            var s=renderer.skySettings
            s.altitude=try number(c,"altitude",s.altitude);s.azimuth=try number(c,"azimuth",s.azimuth)
            s.coverage=try number(c,"coverage",s.coverage);s.density=try number(c,"density",s.density)
            s.haze=try number(c,"haze",s.haze);s.cloudSeed=try number(c,"cloudSeed",s.cloudSeed)
            guard (-12...85).contains(s.altitude),(-360...360).contains(s.azimuth),(0...1).contains(s.coverage),(0...3).contains(s.density),(0.1...3).contains(s.haze),(0...10000).contains(s.cloudSeed) else {throw RuntimeError.message("Sky range: altitude -12...85°, azimuth ±360°, coverage 0...1, density 0...3, haze .1...3, cloudSeed 0...10000")}
            renderer.skySettings=s
        case "reset": renderer.resetCamera()
        case "camera":
            let yaw=try number(c,"yaw",renderer.yaw),pitch=try number(c,"pitch",renderer.pitch)
            let x=try number(c,"x",renderer.camera.x),z=try number(c,"z",renderer.camera.z)
            guard abs(x)<=130,z >= -135,z <= 120 else {throw RuntimeError.message("Camera is outside the garden")}
            let orbit=try number(c,"orbit",renderer.orbit),elevation=try number(c,"elevation",renderer.orbitElevation),distance=try number(c,"metres",try number(c,"distance",renderer.orbitDistance)*renderer.studioUnit)/renderer.studioUnit
            renderer.yaw=yaw;renderer.pitch=clamp(pitch,-1.35,1.35)
            renderer.camera=V3(x,renderer.world.terrain.height(x,z)+1.72,z)
            if c["orbit"] != nil || c["elevation"] != nil {renderer.studioViewName="Custom"}
            renderer.orbit=orbit;renderer.orbitElevation=clamp(elevation,-0.1,1.55);renderer.orbitDistance=renderer.boundedStudioDistance(distance)
        case "move":
            let x=try number(c,"x",0),z=try number(c,"z",0)
            guard abs(x)<=25,abs(z)<=25 else {throw RuntimeError.message("Movement per command is limited to 25 metres per axis")}
            renderer.move(local:V3(x,0,z))
        case "pause":
            guard let paused=c["value"] as? Bool else {throw RuntimeError.message("Pause requires a boolean value")}
            renderer.paused=paused;renderer.keys.removeAll()
        case "step":
            guard renderer.paused else {throw RuntimeError.message("Pause before stepping")}
            let frames=try number(c,"frames",1)
            guard (1...600).contains(frames),frames.rounded(.towardZero)==frames else {throw RuntimeError.message("Step requires an integer from 1...600 frames")}
            let count=Int(frames)
            for _ in 0..<count {renderer.step(1/60)}
        case "parameters":
            let exposure=try number(c,"exposure",renderer.exposure),wind=try number(c,"wind",renderer.wind),scale=try number(c,"scale",renderer.podScale)
            guard (0.5...1.5).contains(exposure),(0...2).contains(wind),(0.6...1.7).contains(scale) else {throw RuntimeError.message("Parameter outside supported range: exposure .5...1.5, wind 0...2, scale .6...1.7")}
            let wetness=try number(c,"wetness",renderer.wetness)
            guard c["gi"] == nil else {throw RuntimeError.message("The old GI switch was removed; lighting uses atmosphere irradiance")}
            guard (0...1).contains(wetness) else {throw RuntimeError.message("Wetness must be between 0 and 1")}
            renderer.wetness=wetness
            renderer.exposure=exposure;renderer.wind=wind;renderer.podScale=scale
            if c["scale"] != nil && renderer.scene=="studio" && renderer.studioObject=="seed" {renderer.setStudioScale(scale)}
        case "query":
            let p=V3(try number(c,"x",0),try number(c,"y",0),try number(c,"z",0))
            guard abs(p.x)<=10000,abs(p.y)<=10000,abs(p.z)<=10000 else {throw RuntimeError.message("Query coordinates must be within 10,000 metres of the origin")}
            let d=renderer.world.studioShape.value(at:(p-renderer.podPosition)/(renderer.podScale*GardenWorld.podUnitScale))*(renderer.podScale*GardenWorld.podUnitScale)
            respond(id,["ok":true,"terrainValue":renderer.world.terrain.value(at:p),"terrainHeight":renderer.world.terrain.height(p.x,p.z),"terrainSemantics":"implicit","podDistanceBound":d,"podPosition":[renderer.podPosition.x,renderer.podPosition.y,renderer.podPosition.z]])
            return
        case "clearMetrics": renderer.clearMetrics()
        case "reloadShaders":
            try renderer.buildPipelines(sourceURL:root.deletingLastPathComponent().appendingPathComponent("Sources/SanctuaryGame/Resources/Garden.metal"))
            renderer.clearMetrics();onMessage?("Shaders reloaded. Camera and world preserved.")
        case "verifySky":
            guard renderer.paused else {throw RuntimeError.message("Pause before verifying sky traversal")}
            respond(id,["ok":true,"verification":try renderer.atmosphere.verifyEmptySpace(renderer.queue)]);return
        case "verifyFields":
            let report=try renderer.verifyFields()
            respond(id,["ok":true,"verification":report]);return
        case "capture":
            guard renderer.captureRequest == nil else {throw RuntimeError.message("A capture is already pending; await its acknowledgement")}
            let label=(c["label"] as? String ?? "capture").filter{$0.isLetter || $0.isNumber || $0=="-" || $0=="_"}.prefix(64)
            let url=captures.appendingPathComponent("\(label)-\(id.prefix(8)).png")
            // Freeze metadata on the actual rendered capture frame in the renderer.
            renderer.captureRequest=(url,renderer.snapshot(),{[weak self] result in
                switch result {
                case let .success(path):self?.respond(id,["ok":true,"path":path.path,"metadata":path.deletingPathExtension().appendingPathExtension("json").path]);self?.onMessage?("Captured \(path.lastPathComponent)")
                case let .failure(error):self?.respond(id,["ok":false,"error":error.localizedDescription])
                }
            })
            return
        default: throw RuntimeError.message("Unknown action: \(action)")
        }
        renderer.onUpdate?();respond(id,["ok":true,"state":renderer.snapshot()])
    }
    func capture() {
        let id=UUID().uuidString
        do {try execute(["action":"capture","label":renderer.scene+"-"+renderer.lighting],id:id)}
        catch {onMessage?(error.localizedDescription)}
    }
}
