import Foundation
import FieldCore
import FieldCompiler
import simd

/// Editable field source, not baked geometry. Shared by the garden and workshop.
struct FieldExpression: Codable {
    var op:String
    var values:[Float] = []
    var children:[FieldExpression] = []
    var nodeCount:Int {1+children.reduce(0){$0+$1.nodeCount}}
    func shape(depth:Int=0) throws -> Shape {
        guard depth<24,values.allSatisfy(\.isFinite),values.allSatisfy({abs($0)<=100}),children.count<=2 else {throw RuntimeError.message("Field exceeds depth, child or coordinate limits")}
        func require(_ n:Int,_ c:Int) throws {guard values.count==n,children.count==c else {throw RuntimeError.message("Invalid arguments for field operation \(op)")}}
        func vector(_ offset:Int=0)->V3 {V3(values[offset],values[offset+1],values[offset+2])}
        let result:Shape
        switch op {
        case "sphere":try require(1,0);result = .sphere(values[0])
        case "box":try require(3,0);result = .box(vector())
        case "capsule":try require(7,0);result = .capsule(vector(),vector(3),values[6])
        case "torus":try require(2,0);result = .torus(values[0],values[1])
        case "move":try require(3,1);result = try children[0].shape(depth:depth+1).moved(vector())
        case "stretch":try require(3,1);result = try children[0].shape(depth:depth+1).stretched(vector())
        case "scale":try require(1,1);result = try children[0].shape(depth:depth+1).sized(values[0])
        case "union","subtract","blend":
            try require(op=="blend" ? 1:0,2)
            let a=try children[0].shape(depth:depth+1),b=try children[1].shape(depth:depth+1)
            result=op=="union" ? a.joined(b):(op=="subtract" ? a.cut(b):a.blended(b,radius:values[0]))
        default:throw RuntimeError.message("Unknown field operation \(op)")
        }
        try result.validate();return result
    }
}
struct AssetPart:Codable {
    var name:String
    var field:FieldExpression
    var color:[Float]
    var material:Int = 7
    var roughness:Float = 0.7
    var metallic:Float = 0
}
struct AssetSource:Codable {
    var version:Int = 1
    var id:String
    var name:String
    var generator:String
    var parameters:[String:Float] = [:]
    var parts:[AssetPart] = []
    var appearance:[String:Float] = [:]
    static var availableIDs:[String] {ids + (((try? FileManager.default.contentsOfDirectory(at:url("tree").deletingLastPathComponent(),includingPropertiesForKeys:nil)) ?? []).filter{$0.pathExtension=="json"}.map{$0.deletingPathExtension().lastPathComponent}.filter{!ids.contains($0)}).sorted()}
    static let ids=["tree","seed","stone","branch","calibration"]
    static var workspace:URL {URL(fileURLWithPath:ProcessInfo.processInfo.environment["SANCTUARY_WORKSPACE"] ?? Bundle.main.object(forInfoDictionaryKey:"SanctuaryWorkspace") as? String ?? FileManager.default.currentDirectoryPath)}
    static func url(_ id:String)->URL {workspace.appendingPathComponent("Authoring/Assets/\(id).json")}
    static let treeRanges:[String:ClosedRange<Float>]=["height":3...12,"width":2...10,"trunk":0.1...0.6,"spread":0.4...2,"leaves":200...4000]
    static func defaults(_ id:String)->AssetSource {
        AssetSource(id:id,name:["tree":"Sanctuary tree","seed":"Seed pod","stone":"Mossy stone","branch":"Branch structure","calibration":"Material spheres"][id] ?? id,generator:id,parameters:id=="tree" || id=="branch" ? ["height":5.6,"width":6,"trunk":0.24,"spread":1,"leaves":1800]:[:])
    }
    static func load(_ id:String) throws -> AssetSource {
        guard availableIDs.contains(id) else {throw RuntimeError.message("Unknown subject \(id)")}
        if FileManager.default.fileExists(atPath:url(id).path) {return try read(url(id))}
        return defaults(id)
    }
    static func read(_ url:URL) throws ->AssetSource {
        let data=try Data(contentsOf:url);guard data.count<262144 else {throw RuntimeError.message("Asset source must be smaller than 256 KiB")}
        let source=try JSONDecoder().decode(AssetSource.self,from:data);try source.validate();return source
    }
    func validate() throws {
        guard version==1,!id.isEmpty,id.count<64,id.allSatisfy({$0.isASCII && ($0.isLetter || $0.isNumber || $0=="-" || $0=="_")}),!name.isEmpty,name.count<100 else {throw RuntimeError.message("Invalid asset identity or version")}
        guard Self.ids.contains(generator) || generator=="fields" else {throw RuntimeError.message("Unknown asset generator")}
        for (key,value) in parameters {guard let range=Self.treeRanges[key],value.isFinite,range.contains(value) else {throw RuntimeError.message("Invalid shape parameter \(key)")}}
        for (key,value) in appearance {guard ["roughness","metallic","tint"].contains(key),value.isFinite,(key=="tint" ? (0.3...1.5).contains(value):(0...1).contains(value)) else {throw RuntimeError.message("Invalid asset appearance")}}
        if generator=="fields" {
            guard (1...12).contains(parts.count),parts.reduce(0,{$0+$1.field.nodeCount})<=128 else {throw RuntimeError.message("A field asset needs 1...12 parts and at most 128 field nodes")}
            for part in parts {
                guard part.color.count==3,part.color.allSatisfy({$0.isFinite && (0...1).contains($0)}),(0...8).contains(part.material),(0.05...1).contains(part.roughness),(0...1).contains(part.metallic) else {throw RuntimeError.message("Invalid part material")}
                let shape=try part.field.shape(),b=shape.bounds
                guard length(b.max-b.min)<=100,length(b.max-b.min)>=0.001 else {throw RuntimeError.message("Field bounds must span 1 mm...100 m")}
            }
        }
    }
    func treeFields()->(Shape,Shape) {
        let h=(parameters["height"] ?? 5.6)/5.6,w=(parameters["width"] ?? 6)/6,r=parameters["trunk"] ?? 0.24,spread=parameters["spread"] ?? 1
        let trunk=Shape.capsule(V3(0,0,0),V3(0.12,3.6,0.1),r)
            .joined(.capsule(V3(0,2.1,0),V3(1.2*spread,3.4,0.1),r*0.54))
            .joined(.capsule(V3(0,2.4,0),V3(-spread,3.65,0.2),r*0.58))
        let crown=Shape.sphere(1).stretched(V3(2.05,0.85,1.65)).moved(V3(0,4.2,0))
            .blended(Shape.sphere(1).stretched(V3(1.5,0.7,1.35)).moved(V3(1.4,3.9,0.2)),radius:0.3)
            .blended(Shape.sphere(1).stretched(V3(1.65,0.7,1.4)).moved(V3(-1.3,4.3,0.1)),radius:0.25)
            .blended(Shape.sphere(1).stretched(V3(1.45,0.7,1.25)).moved(V3(-0.25,4.95,0.1)),radius:0.25)
            .blended(Shape.sphere(1).stretched(V3(1.3,0.65,1.2)).moved(V3(0.1,4.0,-1.15)),radius:0.3)
        return (trunk.stretched(V3(w,h,w)),crown.stretched(V3(w,h,w)))
    }
    static var stoneField:Shape {Shape.sphere(0.9).blended(.sphere(0.65).moved(V3(0.55,-0.08,0.12)),radius:0.3).cut(.box(V3(2,1,2)).moved(V3(0,-1.35,0)))}
    func compile() throws ->[SceneBatch] {
        try compileParts().map {part in
            var b=part
            if let rough=appearance["roughness"] {b.roughness=max(0.05,rough)}
            if let metal=appearance["metallic"] {b.metallic=metal}
            for i in b.instances.indices {let kind=b.instances[i].tint.w;b.instances[i].tint *= appearance["tint"] ?? 1;b.instances[i].tint.w=kind}
            return b
        }
    }
    private func compileParts() throws ->[SceneBatch] {
        try validate()
        func batch(_ name:String,_ shape:Shape,_ tint:V3,_ kind:Float,_ roughness:Float = -1,_ metallic:Float=0) throws ->SceneBatch {
            var b=SceneBatch(name:name,mesh:try Mesher.compile(shape,resolution:48),instances:[Instance(tint:tint,kind:kind)])
            b.roughness=roughness;b.metallic=metallic;return b
        }
        switch generator {
        case "tree","branch":
            let (trunk,crown)=treeFields()
            var result=[try batch("Trunks",trunk,V3(0.31,0.25,0.18),4)]
            if generator=="tree" {
                let canopy=try batch("Canopies",crown,V3(0.40,0.64,0.31),2)
                result.append(canopy)
                result.append(SceneBatch(name:"Leaves",mesh:Mesher.leaves(on:canopy.mesh,count:Int(parameters["leaves"] ?? 1800)),instances:[Instance(tint:V3(0.40,0.64,0.31),kind:8)]))
            }
            return result
        case "seed":
            var seed=try batch("Seed pod",GardenWorld.podShape,V3(0.48,0.27,0.13),6)
            seed.instances[0].model=transform(.zero,V3(repeating:GardenWorld.podUnitScale),0)
            return [seed]
        case "stone":return [try batch("Stones",Self.stoneField,V3(0.53,0.57,0.51),5)]
        case "calibration":
            return try (0..<5).map {i in try batch("Sphere \(i+1)",Shape.sphere(0.45).moved(V3(Float(i-2)*1.1,0.45,0)),V3(repeating:0.55),7,[Float(0.08),0.25,0.5,0.9,0.04][i],i==4 ? 1:0)}
        default:return try parts.enumerated().map {i,p in try batch("\(i) · \(p.name)",p.field.shape(),V3(p.color[0],p.color[1],p.color[2]),Float(p.material),p.roughness,p.metallic)}
        }
    }
}

// Omitted optional JSON fields take the same defaults as Swift-authored fields.
extension FieldExpression {
    enum CodingKeys:String,CodingKey {case op,values,children}
    init(from decoder:Decoder) throws {
        let c=try decoder.container(keyedBy:CodingKeys.self);op=try c.decode(String.self,forKey:.op)
        values=try c.decodeIfPresent([Float].self,forKey:.values) ?? [];children=try c.decodeIfPresent([Self].self,forKey:.children) ?? []
    }
}
extension AssetPart {
    enum CodingKeys:String,CodingKey {case name,field,color,material,roughness,metallic}
    init(from decoder:Decoder) throws {
        let c=try decoder.container(keyedBy:CodingKeys.self);name=try c.decode(String.self,forKey:.name);field=try c.decode(FieldExpression.self,forKey:.field)
        color=try c.decodeIfPresent([Float].self,forKey:.color) ?? [0.55,0.55,0.55]
        material=try c.decodeIfPresent(Int.self,forKey:.material) ?? 7;roughness=try c.decodeIfPresent(Float.self,forKey:.roughness) ?? 0.7;metallic=try c.decodeIfPresent(Float.self,forKey:.metallic) ?? 0
    }
}
extension AssetSource {
    enum CodingKeys:String,CodingKey {case version,id,name,generator,parameters,parts,appearance}
    init(from decoder:Decoder) throws {
        let c=try decoder.container(keyedBy:CodingKeys.self);version=try c.decodeIfPresent(Int.self,forKey:.version) ?? 1
        id=try c.decode(String.self,forKey:.id);name=try c.decode(String.self,forKey:.name);generator=try c.decodeIfPresent(String.self,forKey:.generator) ?? "fields"
        parameters=try c.decodeIfPresent([String:Float].self,forKey:.parameters) ?? [:];parts=try c.decodeIfPresent([AssetPart].self,forKey:.parts) ?? []
        appearance=try c.decodeIfPresent([String:Float].self,forKey:.appearance) ?? [:]
    }
}
