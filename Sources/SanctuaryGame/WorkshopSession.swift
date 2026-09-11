import AppKit
import CryptoKit

/// Authoring state is separate from renderer caches and from published asset files.
final class WorkshopSession {
    unowned let renderer:GardenRenderer
    struct Edit {var name:String;var before:WorkshopDocument;var after:WorkshopDocument}
    var undoStack:[Edit]=[],redoStack:[Edit]=[]
    var pending:(String,WorkshopDocument)?
    var automating=false
    var watching=true
    var watchMessage="Watching source"
    var watchURL:URL?
    var observed:Data?
    var applied:Data?
    var changedAt=Date.distantPast
    var timer:Timer?
    var onMessage:((String)->Void)?
    var lastReview:URL?
    var selectedBaseline:URL?
    var reviewProcess:Process?
    var reviewMessage=""
    var reviewOriginal:WorkshopDocument?
    var root:URL {AssetSource.workspace.appendingPathComponent(".soundstage")}
    init(_ renderer:GardenRenderer) {self.renderer=renderer}
    func start() {
        if let data=try? Data(contentsOf:root.appendingPathComponent("review-state.json")),let paths=try? JSONDecoder().decode([String:String].self,from:data) {
            lastReview=paths["last"].map{URL(fileURLWithPath:$0)}
            selectedBaseline=paths["baseline"].map{URL(fileURLWithPath:$0)}
        }
        resetWatch()
        timer=Timer.scheduledTimer(withTimeInterval:0.2,repeats:true){[weak self] _ in self?.pollSource()}
    }
    func bytes(_ doc:WorkshopDocument)->Data {
        let e=JSONEncoder();e.outputFormatting = [.sortedKeys];return (try? e.encode(doc)) ?? Data()
    }
    func begin(_ name:String) {
        guard renderer.isSoundstage,!automating,pending==nil else{return}
        pending=(name,renderer.workshopDocument())
    }
    func commit() {
        guard let (name,before)=pending else{return};pending=nil
        let after=renderer.workshopDocument()
        guard bytes(before) != bytes(after) else{return}
        undoStack.append(Edit(name:name,before:before,after:after));if undoStack.count>50 {undoStack.removeFirst()};redoStack.removeAll()
    }
    func edit(_ name:String,_ action:()throws->Void) throws {
        guard !automating,reviewProcess==nil else {throw RuntimeError.message("A review is capturing. Wait for it to finish before editing.")}
        commit();begin(name)
        do {try action();commit()} catch {pending=nil;throw error}
    }
    func undo() throws {
        guard !automating else {throw RuntimeError.message("Wait for the review to finish")};commit()
        guard let edit=undoStack.last else{return}
        try renderer.restoreWorkshop(edit.before);undoStack.removeLast();redoStack.append(edit);resetWatch()
        onMessage?("Undid \(edit.name)")
    }
    func redo() throws {
        guard !automating else {throw RuntimeError.message("Wait for the review to finish")}
        guard let edit=redoStack.last else{return}
        try renderer.restoreWorkshop(edit.after);redoStack.removeLast();undoStack.append(edit);resetWatch()
        onMessage?("Redid \(edit.name)")
    }
    func resetWatch() {
        watchURL=renderer.studioSourceURL;observed=try? Data(contentsOf:renderer.studioSourceURL);applied=observed
        watchMessage=watching ? "Watching \(renderer.studioSourceURL.lastPathComponent)":"Auto reload off"
    }
    func pollSource() {
        guard renderer.isSoundstage,watching,!automating,reviewProcess==nil,pending==nil else{return}
        let url=renderer.studioSourceURL
        if url != watchURL {resetWatch();return}
        let data=try? Data(contentsOf:url)
        if data != observed {observed=data;changedAt=Date();return}
        guard data != applied,Date().timeIntervalSince(changedAt)>0.45 else{return}
        applied=data // Attempt once per stable file revision, including malformed revisions.
        do {
            guard let data,data.count<=262144 else {throw RuntimeError.message("Source is missing or exceeds 256 KiB")}
            let source=try JSONDecoder().decode(AssetSource.self,from:data)
            let skyOnly=renderer.studioObject=="sky"
            try edit("Source reload") {try renderer.applySource(source);if renderer.studioSourceURL != url {renderer.importedSourceURL=url};if skyOnly {renderer.studioObject="sky"}}
            watchMessage="Reloaded \(url.lastPathComponent)";onMessage?(watchMessage)
        } catch {watchMessage="Source error · keeping last valid object";onMessage?("\(watchMessage): \(error.localizedDescription)")}
        renderer.onUpdate?()
    }
    func write(_ doc:WorkshopDocument,to url:URL) throws {
        try FileManager.default.createDirectory(at:url.deletingLastPathComponent(),withIntermediateDirectories:true)
        try bytes(doc).write(to:url,options:.atomic)
    }
    func namedFiles(_ folder:String)->[(String,URL)] {
        let dir=root.appendingPathComponent(folder)
        return ((try? FileManager.default.contentsOfDirectory(at:dir,includingPropertiesForKeys:nil)) ?? []).filter{$0.pathExtension=="json"}.sorted{$0.lastPathComponent<$1.lastPathComponent}.map{($0.deletingPathExtension().lastPathComponent,$0)}
    }
    func namedURL(_ name:String,_ folder:String) throws ->URL {
        let clean=name.trimmingCharacters(in:.whitespacesAndNewlines)
        guard !clean.isEmpty,clean.count<=64,!clean.contains("/"),!clean.contains("\\"),clean != ".",clean != ".." else {throw RuntimeError.message("Use a name of 1–64 characters without slashes")}
        let url=root.appendingPathComponent(folder).appendingPathComponent(clean+".json")
        guard !FileManager.default.fileExists(atPath:url.path) else {throw RuntimeError.message("That name already exists. Choose a new checkpoint name.")}
        return url
    }
    func checkpoint(_ name:String) throws ->URL {
        let url=try namedURL(name,"checkpoints");try write(renderer.workshopDocument(),to:url);onMessage?("Saved checkpoint · \(name)");return url
    }
    func restoreCheckpoint(_ name:String) throws {
        guard let url=namedFiles("checkpoints").first(where:{$0.0==name})?.1 else {throw RuntimeError.message("Unknown checkpoint")}
        let doc=try JSONDecoder().decode(WorkshopDocument.self,from:Data(contentsOf:url))
        try edit("Restore \(name)"){try renderer.restoreWorkshop(doc)};resetWatch()
    }
    func saveReviewState() {
        var state:[String:String]=[:];state["last"]=lastReview?.path;state["baseline"]=selectedBaseline?.path
        try? JSONEncoder().encode(state).write(to:root.appendingPathComponent("review-state.json"),options:.atomic)
    }
    func pinBaseline(_ name:String) throws ->URL {
        guard let lastReview,FileManager.default.fileExists(atPath:lastReview.path) else {throw RuntimeError.message("Capture a review first")}
        let url=try namedURL(name,"baselines")
        try FileManager.default.createDirectory(at:url.deletingLastPathComponent(),withIntermediateDirectories:true)
        try JSONEncoder().encode(["report":lastReview.deletingLastPathComponent().appendingPathComponent("report.json").path]).write(to:url,options:.atomic)
        selectedBaseline=lastReview.deletingLastPathComponent().appendingPathComponent("report.json");saveReviewState();onMessage?("Pinned baseline · \(name)");return url
    }
    func selectBaseline(_ name:String) throws {
        if name=="None" {selectedBaseline=nil;saveReviewState();return}
        guard let url=namedFiles("baselines").first(where:{$0.0==name})?.1 else {throw RuntimeError.message("Unknown baseline")}
        let value=try JSONDecoder().decode([String:String].self,from:Data(contentsOf:url))
        guard let path=value["report"],FileManager.default.fileExists(atPath:path) else {throw RuntimeError.message("Baseline report is missing")}
        selectedBaseline=URL(fileURLWithPath:path);saveReviewState()
    }
    func captureReview(styleBoard:Bool=false) throws {
        guard reviewProcess==nil,!automating else {throw RuntimeError.message("A review is already running")}
        commit();reviewOriginal=renderer.workshopDocument()
        let process=Process(),log=root.appendingPathComponent("review-job.log")
        FileManager.default.createFile(atPath:log.path,contents:nil)
        let handle=try FileHandle(forWritingTo:log)
        process.executableURL=URL(fileURLWithPath:"/usr/bin/python3")
        var arguments=[AssetSource.workspace.appendingPathComponent("scripts/workshop-study").path,styleBoard ? "--style-board":"--current","--label",styleBoard ? "style-board":"authoring"]
        if !styleBoard,let selectedBaseline {
            if let data=try? Data(contentsOf:selectedBaseline),let report=(try? JSONSerialization.jsonObject(with:data)) as? [String:Any],let records=report["records"] as? [[String:Any]],let last=records.last?["variant"] as? String,records.filter({$0["variant"] as? String==last}).count>1 {
                arguments.removeAll{$0=="--current"}
            }
            arguments += ["--reference",selectedBaseline.path]
        }
        process.arguments=arguments;process.standardOutput=handle;process.standardError=handle
        process.terminationHandler={[weak self] p in
            try? handle.close()
            DispatchQueue.main.async {
                guard let self else{return};self.reviewProcess=nil
                if p.terminationStatus != 0,let original=self.reviewOriginal {try? self.renderer.restoreWorkshop(original)}
                self.reviewOriginal=nil;self.automating=false
                let output=(try? String(contentsOf:log,encoding:.utf8)) ?? ""
                if p.terminationStatus==0,let line=output.split(separator:"\n").last,let data=String(line).data(using:.utf8),let result=(try? JSONSerialization.jsonObject(with:data)) as? [String:Any],let path=result["viewer"] as? String {
                    self.lastReview=URL(fileURLWithPath:path);self.saveReviewState();self.reviewMessage="Review ready"
                } else {self.reviewMessage="Review failed · \(output.suffix(400))"}
                self.onMessage?(self.reviewMessage);self.renderer.onUpdate?()
            }
        }
        try process.run();reviewProcess=process;reviewMessage="Capturing review…";onMessage?(reviewMessage)
    }
    var snapshot:[String:Any] {
        ["undoDepth":undoStack.count,"redoDepth":redoStack.count,"canUndo":!undoStack.isEmpty,"canRedo":!redoStack.isEmpty,"undo":undoStack.last?.name ?? "","redo":redoStack.last?.name ?? "","watching":watching,"watchStatus":watchMessage,"automating":automating,"checkpoints":namedFiles("checkpoints").map(\.0),"baselines":namedFiles("baselines").map(\.0),"lastReview":lastReview?.path ?? "","baseline":selectedBaseline?.path ?? "","reviewRunning":reviewProcess != nil,"reviewStatus":reviewMessage]
    }
}
