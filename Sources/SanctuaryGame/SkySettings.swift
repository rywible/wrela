import Foundation
import FieldCore

struct SkySettings {
    var altitude:Float
    var azimuth:Float
    var coverage:Float
    var density:Float
    var haze:Float
    var cloudSeed:Float
    var sunDirection:V3 {let a=altitude * .pi/180,z=azimuth * .pi/180;return V3(sin(z)*cos(a),sin(a),-cos(z)*cos(a))}
    var snapshot:[String:Float] {["altitude":altitude,"azimuth":azimuth,"coverage":coverage,"density":density,"haze":haze,"cloudSeed":cloudSeed]}
    static func preset(_ name:String)->Self {
        switch name {
        case "sunset":return Self(altitude:2.5,azimuth:-35,coverage:0.40,density:0.85,haze:1.2,cloudSeed:4)
        case "golden":return Self(altitude:9,azimuth:-35,coverage:0.44,density:0.9,haze:1.0,cloudSeed:4)
        case "afterglow":return Self(altitude:-1.2,azimuth:-35,coverage:0.38,density:0.75,haze:0.8,cloudSeed:4)
        case "rain":return Self(altitude:25,azimuth:-35,coverage:0.95,density:2.4,haze:1.5,cloudSeed:4)
        case "overcast":return Self(altitude:45,azimuth:-35,coverage:0.85,density:1.7,haze:1.2,cloudSeed:4)
        case "indoor":return Self(altitude:58,azimuth:-145,coverage:0,density:1,haze:1,cloudSeed:4)
        default:return Self(altitude:48,azimuth:-35,coverage:0.42,density:0.8,haze:0.7,cloudSeed:4)
        }
    }
}
