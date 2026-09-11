import AVFoundation
import FieldCore

package struct AudioCue {
  package var kind: String, position: V3, gain: Float
  package init(kind: String, position: V3, gain: Float) {
    self.kind = kind
    self.position = position
    self.gain = gain
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
    let duration: Float = cue.kind == "impact" ? 0.65 : cue.kind == "scrape" ? 0.8 : 0.35
    let count = AVAudioFrameCount(duration * 48000)
    guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: count),
      let samples = buffer.floatChannelData?[0]
    else { return }
    buffer.frameLength = count
    var rng = SeededRandom(seed: UInt64(cursor + 91))
    for i in 0..<Int(count) {
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
