import AVFoundation
import FieldCore
import Foundation

/// Game-authored tonal gesture; the shared audio engine supplies no species policy.
package struct ProceduralTone {
  package var frequency: Float
  package var endFrequency: Float
  package var duration: Float
  package var overtone: Float
  package init(frequency: Float, endFrequency: Float, duration: Float, overtone: Float = 0.18) {
    self.frequency = frequency; self.endFrequency = endFrequency
    self.duration = duration; self.overtone = overtone
  }
  package func samples(sampleRate: Int = 48_000) -> [Float] {
    guard frequency.isFinite, endFrequency.isFinite, duration.isFinite, overtone.isFinite,
      (8_000...96_000).contains(sampleRate) else { return [] }
    let seconds = min(2, max(0.04, duration))
    let count = max(2, Int(seconds * Float(sampleRate)))
    let start = min(3_000, max(80, frequency))
    let end = min(3_000, max(80, endFrequency))
    let harmonic = min(0.35, max(0, overtone))
    return (0..<count).map { i in
      let phase = Float(i) / Float(count - 1)
      let t = phase * seconds
      // Integrated linear chirp; frequency changes do not introduce discontinuities.
      let angle = 2 * Float.pi * (start * t + (end - start) * t * t / (2 * seconds))
      let envelope = pow(sin(Float.pi * phase), 2)
      return (sin(angle) + harmonic * sin(2 * angle)) / (1 + harmonic) * envelope
    }
  }
}

package struct AudioCue {
  package var kind: String, position: V3, gain: Float
  package var tone: ProceduralTone?
  package init(kind: String, position: V3, gain: Float, tone: ProceduralTone? = nil) {
    self.kind = kind
    self.position = position
    self.gain = gain
    self.tone = tone
  }
}
/// Small procedural spatial cues. Games select meaning; the engine places sound.
package final class AudioScene {
  let engine = AVAudioEngine(), environment = AVAudioEnvironmentNode()
  var voices: [AVAudioPlayerNode] = [], cursor = 0
  let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
  package init() throws {
    engine.attach(environment)
    engine.connect(environment, to: engine.mainMixerNode, format: nil)
    environment.distanceAttenuationParameters.referenceDistance = 1
    for _ in 0..<6 {
      let v = AVAudioPlayerNode()
      engine.attach(v)
      engine.connect(v, to: environment, format: format)
      v.renderingAlgorithm = .HRTF
      voices.append(v)
    }
    try engine.start()
  }
  package func listener(_ camera: PlayerCamera) {
    environment.listenerPosition = AVAudio3DPoint(
      x: camera.position.x, y: camera.position.y, z: camera.position.z)
    environment.listenerAngularOrientation = AVAudio3DAngularOrientation(
      yaw: camera.yaw * 180 / .pi, pitch: camera.pitch * 180 / .pi, roll: 0)
  }
  package func play(_ cue: AudioCue) {
    guard cue.gain.isFinite else { return }
    let tonalSamples = cue.tone?.samples()
    let duration: Float = cue.kind == "impact" ? 0.65 : cue.kind == "scrape" ? 0.8 : 0.35
    let count = tonalSamples.map { AVAudioFrameCount($0.count) } ?? AVAudioFrameCount(duration * 48000)
    guard count > 0 else { return }
    guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: count),
      let samples = buffer.floatChannelData?[0]
    else { return }
    buffer.frameLength = count
    var rng = SeededRandom(seed: UInt64(cursor + 91))
    for i in 0..<Int(count) {
      if let tonalSamples {
        samples[i] = tonalSamples[i] * min(0.4, max(0, cue.gain))
        continue
      }
      let t = Float(i) / 48000
      let noise = rng.range(-1, 1)
      let envelope = min(1, t * 150) * exp(-t * (cue.kind == "drip" ? 22 : 6))
      let wave: Float =
        cue.kind == "drip"
        ? sin(t * (1600 + 900 * t) * 2 * .pi) : sin(t * 65 * 2 * .pi) * 0.5 + noise * 0.3
      samples[i] = wave * envelope * min(0.4, max(0, cue.gain))
    }
    let v = voices[cursor % voices.count]
    cursor += 1
    v.stop()
    v.position = AVAudio3DPoint(x: cue.position.x, y: cue.position.y, z: cue.position.z)
    v.scheduleBuffer(buffer)
    v.play()
  }
}
