import Foundation

/// A gentle authored weather score, evaluated only from saved play time.
/// It is not a thermodynamic model, precipitation simulation, or real-world clock.
public enum SanctuaryClimate: Sendable {
  /// Forty minutes of active play from one late morning to the next.
  public static let dayDuration: Double = 40 * 60
  /// The cabin opening begins in warm late morning, leaving a long first daylight stretch.
  public static let openingTimeOfDay: Float = 0.43

  public struct Sample: Codable, Equatable, Sendable {
    /// Normalized local time: 0 is midnight and 0.5 is noon.
    public var timeOfDay: Float
    /// 0 is clear and 1 is fully overcast cloud cover.
    public var cloud: Float
    /// 0 is dry and 1 is the peak of this gentle rain event.
    public var rain: Float
    /// 0 is still air and 1 is the authored upper wind bound.
    public var wind: Float

    public init(timeOfDay: Float, cloud: Float, rain: Float, wind: Float) {
      self.timeOfDay = timeOfDay
      self.cloud = cloud
      self.rain = rain
      self.wind = wind
    }
  }

  /// Pure replay input. Pass `Expedition.age` (active simulated seconds), never wall time.
  public static func sample(elapsedPlaySeconds: Double, seed: UInt32) -> Sample {
    let elapsed = elapsedPlaySeconds.isFinite ? elapsedPlaySeconds : 0
    var withinDay = elapsed.truncatingRemainder(dividingBy: dayDuration)
    if withinDay < 0 { withinDay += dayDuration }
    let phase = Float(withinDay / dayDuration) * 2 * .pi
    let timeOfDay = wrapped(openingTimeOfDay + Float(withinDay / dayDuration))

    let broad = 0.5 + 0.5 * sin(phase + seededPhase(seed, salt: 17))
    let detail = 0.5 + 0.5 * sin(phase * 2 + seededPhase(seed, salt: 71))
    let cloud = bounded(0.20 + 0.34 * broad + 0.16 * detail)

    let showerShape = smoothstep(0.57, 0.70, cloud)
    let showerPulse = 0.55 + 0.45 * (0.5 + 0.5 * sin(phase * 3 + seededPhase(seed, salt: 131)))
    let rain = bounded(showerShape * showerPulse)

    let gust = 0.5 + 0.5 * sin(phase * 4 + seededPhase(seed, salt: 211))
    let wind = bounded(0.18 + 0.35 * cloud + 0.14 * gust + 0.10 * rain)
    return Sample(timeOfDay: timeOfDay, cloud: cloud, rain: rain, wind: wind)
  }

  private static func seededPhase(_ seed: UInt32, salt: UInt32) -> Float {
    let mixed = ((seed &+ salt) &* 1_664_525) &+ 1_013_904_223
    return Float(mixed & 0x00ff_ffff) / Float(0x01_00_00_00) * 2 * .pi
  }
  private static func wrapped(_ value: Float) -> Float {
    let remainder = value.truncatingRemainder(dividingBy: 1)
    return remainder < 0 ? remainder + 1 : remainder
  }
  private static func bounded(_ value: Float) -> Float { min(1, max(0, value)) }
  private static func smoothstep(_ low: Float, _ high: Float, _ value: Float) -> Float {
    let t = bounded((value - low) / (high - low))
    return t * t * (3 - 2 * t)
  }
}
