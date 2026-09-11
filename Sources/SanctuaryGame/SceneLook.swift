import Foundation
import simd

/// One shared art-direction layer above physical atmosphere and material inputs.
struct SceneLook:Codable,Equatable {
    var detail:Float=0.8
    var roughnessBias:Float=0.025
    var surfaceSaturation:Float=0.96
    var sunStrength:Float=1
    var ambientStrength:Float=1.08
    var skySaturation:Float=0.98
    var saturation:Float=1
    var contrast:Float=0.98
    var warmth:Float=0.015
    var bloom:Float=0.25
    static let ranges:[String:ClosedRange<Float>]=["detail":0...1.5,"roughnessBias":(-0.25)...0.25,"surfaceSaturation":0...1.5,"sunStrength":0.25...2,"ambientStrength":0.25...2,"skySaturation":0...1.5,"saturation":0...1.5,"contrast":0.75...1.25,"warmth":(-0.25)...0.25,"bloom":0...0.6]
    var values:[String:Float] { ["detail":detail,"roughnessBias":roughnessBias,"surfaceSaturation":surfaceSaturation,"sunStrength":sunStrength,"ambientStrength":ambientStrength,"skySaturation":skySaturation,"saturation":saturation,"contrast":contrast,"warmth":warmth,"bloom":bloom] }
    mutating func set(_ edits:[String:Float]) throws {
        var candidate=values
        for (key,value) in edits {guard let range=Self.ranges[key],value.isFinite,range.contains(value) else {throw RuntimeError.message("Invalid scene-look value: \(key)")};candidate[key]=value}
        self=try JSONDecoder().decode(Self.self,from:JSONEncoder().encode(candidate))
    }
    func validate() throws {var copy=self;try copy.set(values)}
    static var url:URL {AssetSource.workspace.appendingPathComponent("Authoring/ArtDirection.json")}
    static func load() throws ->Self {
        let data=try Data(contentsOf:url)
        guard data.count<262144,let object=try JSONSerialization.jsonObject(with:data) as? [String:Any],let look=object["renderLook"] else {throw RuntimeError.message("Art direction requires renderLook")}
        let result=try JSONDecoder().decode(Self.self,from:JSONSerialization.data(withJSONObject:look));try result.validate();return result
    }
    func publish() throws {
        try validate();let data=try Data(contentsOf:Self.url)
        guard var object=try JSONSerialization.jsonObject(with:data) as? [String:Any] else {throw RuntimeError.message("Invalid art-direction document")}
        object["renderLook"]=try JSONSerialization.jsonObject(with:JSONEncoder().encode(self))
        try JSONSerialization.data(withJSONObject:object,options:[.prettyPrinted,.sortedKeys]).write(to:Self.url,options:.atomic)
    }
    var surface:SIMD4<Float> {SIMD4(detail,roughnessBias,surfaceSaturation,0)}
    var light:SIMD4<Float> {SIMD4(sunStrength,ambientStrength,skySaturation,0)}
    var grade:SIMD4<Float> {SIMD4(saturation,contrast,warmth,bloom)}
}
