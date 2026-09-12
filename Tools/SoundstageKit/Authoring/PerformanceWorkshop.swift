import FieldCore
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  var performanceBeats: [PerformanceBeat] {
    if let score = studioSource.performance { return score.beats }
    guard let animation = studioSource.animation else { return [] }
    let scale =
      animation.duration(studioSource.motion ?? MotionParameters()) / animation.beatDuration
    return animation.beats.map { PerformanceBeat($0.name, $0.time * scale, $0.intent) }
  }
  var performanceDuration: Float {
    studioSource.craft.arrangement?.duration ?? studioSource.performance?.duration ?? studioSource.animation?.duration(
      studioSource.motion ?? MotionParameters()) ?? 1
  }
  var performanceTime: Float {
    if studioSource.craft.arrangement?.looping == false,studioSource.craft.arrangement?.clip == creatureWorkshop.mode {return min(creatureWorkshop.seconds,performanceDuration)}
    return creatureWorkshop.seconds.truncatingRemainder(dividingBy: performanceDuration)
  }
  var performanceBeat: PerformanceBeat? { performanceBeats.last { $0.time <= performanceTime } }
  var performanceSnapshot: [String: Any] {
    let m = studioSource.motion ?? MotionParameters()
    let matrices = creatureWorkshop.matrices(source: studioSource)
    var anchors: [String: [Float]] = [:]
    for joint in studioSource.joints {
      let p = (matrices[joint.id] ?? matrix_identity_float4x4) * SIMD4(joint.pivot, 1)
      anchors[joint.id] = [p.x, p.y, p.z]
    }
    return [
      "jointPositions": anchors,
      "contacts": (try? JSONSerialization.jsonObject(with:JSONEncoder().encode(creatureWorkshop.evaluatedPose(source:studioSource).contacts))) ?? [],
      "surfaceEdits":studioSource.surfaceEdits.count,
      "diagnosticsScope": "Generator diagnostics below; contacts are measured after pose corrections and contact solving",
      "clip": creatureWorkshop.mode, "time": performanceTime, "duration": performanceDuration,
      "beat": performanceBeat?.name ?? "", "intent": performanceBeat?.intent ?? "",
      "tracks": studioSource.performance?.tracks.count ?? 0,
      "skinnedParts": studioBatches.filter { !$0.skinJoints.isEmpty }.map(\.name),
      "beats": performanceBeats.map {
        ["name": $0.name, "time": $0.time, "intent": $0.intent] as [String: Any]
      },
      "diagnostics": studioSource.animation?.inspect?(
        creatureWorkshop.mode, creatureWorkshop.seconds, creatureWorkshop.actor, m) ?? [:],
    ]
  }
  func jumpToBeat(_ name: String) throws {
    guard let beat = performanceBeats.first(where: { $0.name == name }),
      let clip = studioSource.performance?.clip ?? studioSource.animation?.clips.last
    else {
      throw RuntimeError.message("Unknown performance beat")
    }
    try editCreatureWorkshop(mode: clip, seconds: beat.time)
  }
  func editPerformanceKey(
    joint: String, channel: String, time: Float, value: Float, remove: Bool = false
  ) throws {
    guard studioSource.animation?.clips.contains(creatureWorkshop.mode) == true else {
      throw RuntimeError.message("Choose a motion clip before authoring a key")
    }
    guard studioSource.joints.contains(where: { $0.id == joint }),
      ["x", "y", "z", "pitch", "yaw", "roll"].contains(channel)
    else { throw PerformanceError.invalid }
    var source = studioSource
    var score =
      source.performance
      ?? PerformanceScore(
        clip: creatureWorkshop.mode,
        duration: performanceDuration, beats: performanceBeats)
    guard score.clip == creatureWorkshop.mode, time.isFinite, value.isFinite,
      time >= 0, time <= score.duration
    else { throw PerformanceError.invalid }
    var track =
      score.tracks.first { $0.joint == joint && $0.channel == channel }
      ?? PoseTrack(joint, channel, [])
    track.curve.keys.removeAll { abs($0.time - time) < 0.0001 }
    if !remove { track.curve.keys.append(MotionKey(time, value)) }
    track.curve.keys.sort { $0.time < $1.time }
    score.tracks.removeAll { $0.joint == joint && $0.channel == channel }
    if !track.curve.keys.isEmpty { score.tracks.append(track) }
    source.performance = score
    try source.validate()
    try applySource(source)
  }
}
