import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// Vesper: an original masked processional beast. Every surface is authored here.
enum ProcessionalRecipe {
  static let ivory = V3(0.72, 0.65, 0.49), gold = V3(0.55, 0.37, 0.14)
  static let dark = V3(0.19, 0.20, 0.17), cloth = V3(0.30, 0.40, 0.36)
  static let controls: [ScalarControl] = [
    .init("groomDensity", "Groom density", 1, 0.4...1.7),
    .init("groomSpread", "Groom spread", 1, 0.5...1.8),
    .init("groomCurl", "Fibre curl · m", 0.025, 0...0.08),
    .init("limbMass", "Limb muscle volume", 1, 0.7...1.5),
    .init("maskWidth", "Mask breadth", 1, 0.75...1.3),
    .init("hornSweep", "Horn sweep", 1, 0.65...1.4),
    .init("maneLength", "Mane length", 1, 0.6...1.45),
    .init("mantleWidth", "Mantle breadth", 1, 0.8...1.25),
  ]
  static var generator: AssetGenerator {
    AssetGenerator(
      id: "vesper", name: "Vesper · Processional Beast", controls: controls,
      animation: animation, parts: parts, compile: compile)
  }
  static func parts(_ params: [String: Float]) -> [AssetPart] {
    ProcessionalMotion.joints.map { joint in
      let ext: V3
      switch joint.id {
      case "mask", "mane", "horns", "eyes": ext = V3(1, 0.95, 0.9)
      case "body", "mantle", "trim": ext = V3(1.3, 1.1, 2.2)
      default: ext = V3(0.45, 0.65, 0.5)
      }
      return AssetPart(
        name: joint.id, joint: joint, field: .ellipsoid(joint.pivot, ext),
        color: [ivory.x, ivory.y, ivory.z], roughness: 0.7)
    }
  }
  static let motionControls: [ScalarControl] = [
    .init("amplitude", "Gesture amplitude", 1, 0.3...1.15),
    .init("tempo", "Performance tempo", 1, 0.65...1.4),
    .init("followThrough", "Mane follow-through", 1, 0...1.5),
  ]
  static func frame(_ clip: String, _ time: Float, _ p: MotionParameters)
    -> ProcessionalMotion.Frame
  {
    ProcessionalMotion.frame(
      clip: clip, time: time, amplitude: p.values["amplitude"] ?? 1,
      tempo: p.values["tempo"] ?? 1, followThrough: p.values["followThrough"] ?? 1)
  }
  static var animation: AnimationDefinition {
    var definition = AnimationDefinition(
      controls: motionControls, clips: ["idle", "procession"],
      scenarios: ["rehearsal", "visitor", "startle"],
      signalName: "Attention", duration: { 12 / ($0.values["tempo"] ?? 1) },
      poses: { clip, time, state, p in
        frame(
          clip == "behavior" ? (state.state == "resting" ? "idle" : "procession") : clip,
          clip == "behavior" ? Float(state.tick) / 60 : time, p
        ).poses
      },
      root: { _, _, _, _ in matrix_identity_float4x4 },
      initialize: { _ in
        var state = BehaviorSnapshot()
        state.state = "resting"
        state.reason = "Awaiting a visitor"
        return state
      },
      step: { state, input, p in
        var state = state
        let engaged = input.visible && length(input.player) < 7
        if engaged || state.state != "resting" { state.tick += 1 }
        if engaged {
          state.state = "performing"
          state.reason =
            input.running
            ? "A sudden approach begins the ritual" : "The mask acknowledges the visitor"
        }
        if !engaged && state.tick >= Int((720 / (p.values["tempo"] ?? 1)).rounded()) {
          state.tick = 0
          state.state = "resting"
          state.reason = "The completed phrase settles into stillness"
        }
        state.fear = engaged ? 1 : 0
        state.target = input.player
        return state
      },
      input: { scenario, time in
        var i = PreviewInput()
        i.player = SIMD2(0, scenario == "rehearsal" ? -4 : (time < 1 ? -9 : -4))
        i.running = scenario == "startle"
        return i
      }, bounds: { _ in Bounds(V3(-4, 0, -5), V3(4, 5, 4)) },
      validate: { p in
        for (key, value) in p.values {
          guard let c = motionControls.first(where: { $0.key == key }), value.isFinite,
            c.range.contains(value)
          else { throw PerformanceError.invalid }
        }
      })
    for front in [true, false] {
      for side: Float in [-1, 1] {
        let id = ProcessionalMotion.limbID(front, side)
        definition.contactChains.append(ContactChain(id:id,parent:front ? "neck":"haunch",
          upper:id+"-upper",lower:id+"-lower",foot:id+"-paw",pole:V3(side*0.4,0,2),sole:0.18))
        definition.rigLinks += [
          (front ? "neck" : "haunch", id + "-upper"), (id + "-upper", id + "-lower"),
          (id + "-lower", id + "-paw"),
        ]
      }
    }
    definition.collisionBodies = [
      RigCapsule("chest",joint:"body",a:V3(0,1.8,-0.65),b:V3(0,1.7,0.5),radius:0.57,ignore:["haunch","neck"]),
      RigCapsule("haunch",joint:"haunch",a:V3(0,1.55,0.9),b:V3(0,1.55,1.6),radius:0.53,ignore:["neck"]),
      RigCapsule("neck",joint:"neck",a:V3(0,2.1,-0.85),b:V3(0,2.7,-1.4),radius:0.4,ignore:["mask"]),
      RigCapsule("mask",joint:"mask",a:V3(-0.35,3,-1.8),b:V3(0.35,3,-1.8),radius:0.43)
    ]
    definition.secondaryRig = secondaryRig
    definition.contactTargets = {clip,time,state,p in
      let f=frame(clip == "behavior" ? (state.state == "resting" ? "idle":"procession"):clip,
        clip == "behavior" ? Float(state.tick)/60:time,p)
      return Dictionary(uniqueKeysWithValues:f.targets.map {($0.key,ContactTarget($0.value,planted:f.contacts[$0.key] == true))})
    }
    definition.motionEnvelope = Bounds(V3(-3,0,-3.3),V3(3,6,3.8))
    definition.beatDuration = 12
    definition.beats = ProcessionalMotion.beats
    definition.inspect = { clip, time, state, params in
      let effectiveClip = clip == "behavior" ? (state.state == "resting" ? "idle" : "procession") : clip
      let f = frame(effectiveClip, clip == "behavior" ? Float(state.tick)/60 : time, params)
      var values: [String: Float] = ["maximumReachErrorMetres": f.maximumReachError]
      for (id, p) in f.feet {
        values[id + ".height"] = p.y
        values[id + ".contact"] = f.contacts[id] == true ? 1 : 0
      }
      return values
    }
    return definition
  }
  static func compile(_ source: AssetSource) throws -> [SceneBatch] {
    let width = source.parameters["maskWidth"] ?? 1
    let sweep = source.parameters["hornSweep"] ?? 1
    let mantle = source.parameters["mantleWidth"] ?? 1
    var batches: [SceneBatch] = []
    func emit(
      _ id: String, _ mesh: Mesh, _ rough: Float = 0.7, _ metallic: Float = 0, _ skin: Bool = false
    ) {
      var b = SceneBatch(
        name: id, mesh: mesh,
        instances: [
          Instance(
            kind: ["mane","beard"].contains(id) ? 13 : id == "mantle"
              ? 10
              : ["mask", "jaw", "mane", "beard"].contains(id)
                ? 11 : ["horns", "trim"].contains(id) ? 12 : 7)
        ], roughness: rough, doubleSided: true, metallic: metallic)
      if skin && ["mane", "beard", "tail"].contains(id) {
        b.skinJoints = [id == "mane" ? "mask" : id == "beard" ? "jaw" : "haunch", id]
        b.skinWeights = mesh.vertices.map { vertex in
          let t: Float =
            id == "beard"
            ? (2.63 - vertex.position.y) / 0.85
            : id == "mane" ? (vertex.position.z + 1.5) / 1.25 : (vertex.position.z - 1.9) / 1.6
          return SkinWeight(0, 1, blend: t * t)
        }
      } else if skin {
        b.skinJoints = ["neck", "body", "haunch"]
        b.skinWeights = mesh.vertices.map { vertex in
          let z = vertex.position.z
          if z < 0.5 { return SkinWeight(0, 1, blend: (z + 1.1) / 1.6) }
          return SkinWeight(1, 2, blend: (z - 0.5) / 1.2)
        }
      }
      if id == "mantle" || id == "trim" {
        b.skinJoints=secondaryRig(source.parameters).nodes.filter{$0.id.hasPrefix("fabric-")}.map(\.id)
        b.skinWeights=clothWeights(mesh,parameters:source.parameters)
      }
      if let p = source.partOverrides[id] {
        if let rough = p.roughness { b.roughness = rough }
        if let metal = p.metallic { b.metallic = metal }
      }
      batches.append(b)
    }
    func ell(_ p: V3, _ r: V3, _ c: V3 = ivory, _ detail: Int = 20) -> Mesh {
      ParametricMesh.ellipsoid(p, r, color: c, detail: detail)
    }
    func tube(_ points: [V3], _ radius: Float, _ c: V3, _ taper: Bool = true, _ segments: Int = 28, _ profile: ((Float,Float)->Float)? = nil)
      -> Mesh
    {
      let n = Float(points.count - 1)
      return ParametricMesh.tube(
        segments: segments, sides: 10,
        center: { t in
          let f = min(t * n, n - 0.00001)
          let i = Int(f)
          let u = f - Float(i)
          let a = points[max(0, i - 1)]
          let b = points[i]
          let c = points[min(points.count - 1, i + 1)]
          let d = points[min(points.count - 1, i + 2)]
          let linear: V3 = (c - a) * u
          let quadratic: V3 = (a * 2 - b * 5 + c * 4 - d) * (u * u)
          let cubic: V3 = (-a + b * 3 - c * 3 + d) * (u * u * u)
          return (b * 2 + linear + quadratic + cubic) * 0.5
        }, radius: { t in max(0.002, radius * (taper ? pow(1 - t, 0.8) : 1)) }, color: c,profile:profile)
    }
    // Continuous underbody and mantle share longitudinal skin weights.
    emit(
      "body",
      ParametricMesh.surface(
        u: 56, v: 72,
        position: { u, t in
          let z = -1.25 + t * 3.4
          let a = u * 2 * Float.pi
          let bulge = pow(sin(Float.pi * (0.03 + t * 0.94)), 0.42)
          let y = 2.2 - t * 0.65
          return V3(cos(a) * 0.67 * bulge, y + sin(a) * 0.70 * bulge, z)
        }, tint: { u, t in dark * (0.8 + 0.2 * sin(u * 19 + t * 11)) }), 0.85, 0, true)
    emit("mantle",ProcessionalFinery.mantle(width:mantle,cloth:V3(0.18,0.27,0.245),gold:gold),0.89,0,true)
    emit("trim",ProcessionalFinery.trim(width:mantle,gold:gold),0.62,0.30,true)
    let face=try carvedFace(width:width)
    emit("mask",face.mask,0.67,0)
    batches[batches.count-1].surfaceReduction=(0.20,0.001)
    emit("jaw",face.jaw,0.72)
    batches[batches.count-1].surfaceReduction=(0.20,0.001)
    emit("eyes",face.eyes,0.16,0)
    var horns: [Mesh] = []
    for s: Float in [-1, 1] {
      horns.append(
        tube(
          [
            V3(s * 0.46, 3.48, -1.65), V3(s * 0.85, 3.85, -1.32), V3(s * 1.12 * sweep, 4.28, -1.13),
            V3(s * 0.97 * sweep, 4.62, -1.33), V3(s * 0.68 * sweep, 4.70, -1.63),
          ], 0.18, gold * 0.7, true, 110,{t,a in
            let rings:Float=0.075*sin(t*150+sin(a*3)*0.2)*pow(1-t,0.6)
            return 1+rings+0.035*cos(a*7+t*4)
          }))
      horns.append(
        tube(
          [
            V3(s * 0.58, 3.41, -1.36), V3(s * 1.13, 3.62, -0.96), V3(s * 1.48 * sweep, 3.93, -0.80),
          ], 0.12, ivory * 0.73, true, 40))
      for i in 0..<6 {
        let t = Float(i) / 6
        horns.append(
          ell(
            V3(s * (0.49 + t * 0.34), 3.52 + t * 0.39, -1.62 + t * 0.28),
            V3(0.14 - 0.03 * t, 0.047, 0.13 - 0.02 * t), gold * 1.15, 12))
      }
    }
    emit("horns", ParametricMesh.joined(horns), 0.47, 0.5)
    let maneGroom=GuideGroom.compile(maneGuides(source.parameters),fibres:Int(48*(source.parameters["groomDensity"] ?? 1)),width:0.12*(source.parameters["groomSpread"] ?? 1),radius:0.009,curl:source.parameters["groomCurl"] ?? 0.025,color:ivory*0.82)
    emit("mane",maneGroom.mesh,0.94)
    batches[batches.count-1].skinJoints=maneGroom.joints
    batches[batches.count-1].skinWeights=maneGroom.weights
    let beardGroom=GuideGroom.compile(beardGuides(source.parameters),fibres:48,width:0.065,radius:0.006,color:ivory*0.85)
    emit("beard",beardGroom.mesh,0.94)
    batches[batches.count-1].skinJoints=beardGroom.joints
    batches[batches.count-1].skinWeights=beardGroom.weights
    var tail = [
      tube(
        [V3(0, 1.65, 1.9), V3(0.1, 1.6, 2.6), V3(0.38, 1.95, 3.1), V3(0.65, 2.35, 3.25)], 0.14,
        dark, true, 48)
    ]
    for i in 0..<18 {
      let a = Float(i) * 2 * Float.pi / 18
      tail.append(
        tube(
          [
            V3(0.58, 2.25, 3.2), V3(0.72 + cos(a) * 0.13, 2.52 + sin(a) * 0.12, 3.38),
            V3(0.95 + cos(a) * 0.12, 2.57, 3.65),
          ], 0.052, ivory * 0.7, true, 18))
    }
    emit("tail", ParametricMesh.joined(tail), 0.85, 0, true)
    for front in [true, false] {
      for side: Float in [-1, 1] {
        let id = ProcessionalMotion.limbID(front, side)
        let r = ProcessionalMotion.restLimb(front, side)
        let mass=source.parameters["limbMass"] ?? 1
        let upperMid:V3=(r.0+r.1)*0.5
        let lowerMid:V3=(r.1+r.2)*0.5
        let controls:[V3]=[r.0,upperMid,r.1,lowerMid,r.2]
        let segments=56,sides=14
        var limb=ParametricMesh.tube(segments:segments,sides:sides,center:{t in
          let f=min(t*4,3.99999),i=Int(f),u=f-Float(i)
          let a=controls[max(0,i-1)],b=controls[i],c=controls[min(4,i+1)],d=controls[min(4,i+2)]
          let linear:V3=(c-a)*u
          let quadratic:V3=(a*2-b*5+c*4-d)*(u*u)
          let cubic:V3=(-a+b*3-c*3+d)*(u*u*u)
          return (b*2+linear+quadratic+cubic)*0.5
        },radius:{t in
          let muscle:Float=0.12+(front ? 0.15:0.18)*exp(-pow((t-0.22)/0.24,2))
          let tendon:Float=0.04*exp(-pow((t-0.76)/0.17,2))
          return (muscle+tendon)*mass
        },color:ivory*0.55,profile:{t,a in
          let muscle:Float=0.13*cos(a*2)*exp(-pow((t-0.22)/0.3,2))
          let tendons:Float=0.06*cos(a*3+0.4)*sin(Float.pi*t)
          return 1+muscle+tendons
        })
        var weights:[SkinWeight]=[]
        for i in limb.vertices.indices {
          let t=Float(i/(sides+1))/Float(segments)
          let blend=min(1,max(0,(t-0.35)/0.3)),w=blend*blend*(3-2*blend)
          weights.append(SkinWeight(0,1,blend:w))
          let color=dark*(1-w)+ivory*0.52*w
          limb.vertices[i].color=SIMD4(color,1)
        }
        emit(id+"-upper",limb,0.83)
        batches[batches.count-1].skinJoints=[id+"-upper",id+"-lower"]
        batches[batches.count-1].skinWeights=weights
        // The stable lower-part identity remains an articulated knee ornament.
        emit(id+"-lower",ell(r.1,V3(0.16,0.14,0.17)*mass,gold*0.65),0.72)
        var paw = [ell(r.2 + V3(0, -0.02, -0.13), V3(0.23, 0.15, 0.34), ivory * 0.72)]
        for toe in -1...1 {
          let p = r.2 + V3(Float(toe) * 0.13, -0.035, -0.30)
          paw.append(
            tube([p, p + V3(0, -0.055, -0.19), p + V3(0, -0.1, -0.25)], 0.063, dark, true, 14))
        }
        emit(id + "-paw", ParametricMesh.joined(paw), 0.67)
      }
    }
    return batches
  }
}
