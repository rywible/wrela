import FieldCore
import SanctuaryContent
import simd

/// Acting on the production brain's time/phase. No independent navigation, clock or gait state.
/// The existing production root hop/hover supplies elevation; these are joint-local poses only.
enum SanctuaryMountMotion {
  static func poses(for species: WildlifeSpecies, time: Float, phase: Float?, gaze: Float,
    mood: String, parameters: [String: Float] = [:]) -> [String: JointPose] {
    species == .moonhart ? moonhart(time: time, phase: phase, gaze: gaze, mood: mood)
      : species == .cloudRay ? cloudRay(time: time, gaze: gaze, mood: mood) : [:]
  }

  /// The production signal advances only after accepted movement. Its 64 m wrap contains
  /// exactly eighty 0.8 m strides, so reopening/wrapping does not reset the gait phase.
  /// This bounded first gait is level-support, distance based IK, not a terrain foot planner.
  static func mountedPoses(signal: WildlifeMountedTravelSignal, actorYaw: Float,
    morphologyScale: Float, time: Float, gaze: Float, mood: String) -> [String: JointPose] {
    var result = moonhart(time: time, phase: nil, gaze: gaze, mood: mood)
    guard signal.active, signal.cycleDistance.isFinite, actorYaw.isFinite,
      morphologyScale.isFinite, (0.62...1).contains(morphologyScale) else { return result }
    // Zero is also the valid 64 m wrap. Every active attachment uses the same
    // stance, including a first attachment or a stop exactly at that boundary.
    let stride: Float = 0.8
    let duty: Float = 0.55
    let localStride = stride / morphologyScale
    // Signal heading uses camera-style +sin X; actorYaw is the rendered right-handed
    // Y rotation. Inverting that root maps the world segment to heading + actorYaw.
    let heading = signal.heading + actorYaw
    let direction = V3(sin(heading), 0, -cos(heading))
    let bodyOffset = V3(0, -0.075, 0)
    result["Wildlife moonhart body"] = JointPose(offset: bodyOffset)
    let offsets: [Float] = [0, 0.5, 0.75, 0.25]
    for leg in SanctuaryMountDesign.legs {
      let cycle = signal.cycleDistance / stride + offsets[leg.id]
      let phase = cycle - floor(cycle)
      let along: Float
      let lift: Float
      if phase < duty {
        // On a straight segment, the local target recedes exactly one world metre per
        // accepted metre: stance has no deliberate fore/aft skating.
        along = localStride * (duty * 0.5 - phase)
        lift = 0
      } else {
        let swing = (phase - duty) / (1 - duty)
        let ease = swing * swing * (3 - 2 * swing)
        along = localStride * duty * (ease - 0.5)
        // Stopping plants the current target without running an independent idle cycle.
        lift = signal.isMoving ? sin(swing * .pi) * 0.085 : 0
      }
      let target = leg.foot + direction * along + V3(0, lift, 0)
      let root = leg.hip + bodyOffset
      let solved = TwoBoneIK.solve(root: root, target: target,
        pole: leg.knee + bodyOffset + V3(0, 0, 0.35),
        upper: length(leg.knee - leg.hip), lower: length(leg.foot - leg.knee))
      let upper = simd_quatf(from: normalize(leg.knee - leg.hip),
        to: normalize(solved.joint - root))
      let lower = simd_quatf(from: normalize(leg.foot - leg.knee),
        to: normalize(solved.end - solved.joint))
      result[leg.upperName] = JointPose(rotation: euler(upper))
      result[leg.lowerName] = JointPose(rotation: euler(upper.inverse * lower))
      result[leg.hoofName] = JointPose(rotation: euler(lower.inverse))
    }
    return result
  }

  private static func moonhart(time: Float, phase: Float?, gaze: Float, mood: String) -> [String: JointPose] {
    let blink = CreatureMotion().blink(at: time)
    let p = min(1, max(0, phase ?? 0))
    let prep: Float = phase == nil || p >= 0.22 ? 0 : sin(p / 0.22 * .pi)
    let landing: Float = phase == nil || p <= 0.72 ? 0 : sin((p - 0.72) / 0.28 * .pi)
    let air = phase.map { CreatureMotion.flight($0) } ?? 0
    let bodyOffset = V3(0, -0.055 * prep - 0.045 * landing, 0)
    let greeting: Float = mood == "greeting" ? 1 : 0
    let alert: Float = mood == "fleeing" ? 1 : 0
    var result: [String: JointPose] = [
      "Wildlife moonhart body": JointPose(offset: bodyOffset),
      "Wildlife moonhart head": JointPose(rotation: V3(
        sin(time * 0.8) * 1.2 - greeting * 4 + alert * 4,
        clamp(gaze, -19, 19) + sin(time * 0.37) * 1.8, sin(time * 0.43) * 0.6)),
      "Wildlife moonhart tail": JointPose(rotation: V3(-air * 8, sin(time * 2.1) * (4 + greeting * 7), 0)),
    ]
    for side: Float in [-1, 1] {
      let suffix = "\(side)"
      result["Wildlife moonhart eye \(suffix)"] = JointPose(scale: V3(1, max(0.05, 1 - blink * 0.95), 1))
      let flick = pow(max(0, sin(time * 1.04 + side * 1.8)), 14)
      result["Wildlife moonhart ear \(suffix)"] = JointPose(rotation: V3(
        flick * 6 - alert * 10, side * (flick * 9 + greeting * 3), side * alert * 4))
    }
    for leg in SanctuaryMountDesign.legs {
      let front = leg.id < 2
      let root = leg.hip + bodyOffset
      let lift: Float = front ? 0.105 : 0.14
      // Production translation occurs only during flight. Both stance intervals keep each
      // target at the bind contact; flexing joints absorb the body anticipation/landing.
      let target = leg.foot + V3(0, lift * air, (front ? 0.12 : -0.10) * air)
      let pole = leg.knee + bodyOffset + V3(0, 0, 0.35)
      let solved = TwoBoneIK.solve(root: root, target: target, pole: pole,
        upper: length(leg.knee - leg.hip), lower: length(leg.foot - leg.knee))
      let upper = angle(from: leg.knee - leg.hip, to: solved.joint - root)
      let lower = angle(from: leg.foot - leg.knee, to: solved.end - solved.joint)
      result[leg.upperName] = JointPose(rotation: V3(upper, 0, 0))
      result[leg.lowerName] = JointPose(rotation: V3(lower - upper, 0, 0))
      result[leg.hoofName] = JointPose(rotation: V3(-lower, 0, 0))
    }
    return result
  }

  private static func cloudRay(time: Float, gaze: Float, mood: String) -> [String: JointPose] {
    let blink = CreatureMotion().blink(at: time + 0.8)
    let wave = time * 0.72
    let attention = clamp(gaze, -20, 20) / 20
    var result: [String: JointPose] = [:]
    for side: Float in [-1, 1] {
      let bank = attention * 1.6
      // Continuous mantle skin weights blend the small central saddle into the full wings.
      // A slow downstroke, restrained bank and delayed tail replace synchronized disc rocking.
      result["Wildlife cloudRay wing \(side)"] = JointPose(rotation: V3(
        sin(wave - 0.65) * 1.6,
        0, side * (sin(wave - 0.35) * 10 + 2.5) + bank))
      result["Wildlife cloudRay eye \(side)"] = JointPose(scale: V3(1, max(0.06, 1 - blink * 0.94), 1))
      result["Wildlife cloudRay iris \(side)"] = JointPose(offset: V3(attention * 0.004, 0, 0))
    }
    result["Wildlife cloudRay trailing tail"] = JointPose(rotation: V3(
      sin(wave - 1.1) * 3.5, sin(wave * 0.73 - 0.9) * 4, 0))
    return result
  }

  private static func angle(from a: V3, to b: V3) -> Float {
    let delta = atan2(b.z, b.y) - atan2(a.z, a.y)
    return atan2(sin(delta), cos(delta)) * (180 / .pi)
  }

  private static func euler(_ rotation: simd_quatf) -> V3 {
    let q = rotation.vector
    return V3(
      atan2(2 * (q.w * q.x + q.y * q.z), 1 - 2 * (q.x * q.x + q.y * q.y)),
      asin(clamp(2 * (q.w * q.y - q.z * q.x), -1, 1)),
      atan2(2 * (q.w * q.z + q.x * q.y), 1 - 2 * (q.y * q.y + q.z * q.z))) * (180 / .pi)
  }
}
