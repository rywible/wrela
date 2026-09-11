import Foundation
import simd

/// One shared art-direction layer above physical atmosphere and material inputs.
package struct SceneLook: Codable, Equatable {
  package init() {}

  package var detail: Float = 0.8
  package var roughnessBias: Float = 0.025
  package var surfaceSaturation: Float = 0.96
  package var sunStrength: Float = 1
  package var ambientStrength: Float = 1.08
  package var skySaturation: Float = 0.98
  package var saturation: Float = 1
  package var contrast: Float = 0.98
  package var warmth: Float = 0.015
  package var bloom: Float = 0.25
  package static let ranges: [String: ClosedRange<Float>] = [
    "detail": 0...1.5, "roughnessBias": (-0.25)...0.25, "surfaceSaturation": 0...1.5,
    "sunStrength": 0.25...2, "ambientStrength": 0.25...2, "skySaturation": 0...1.5,
    "saturation": 0...1.5, "contrast": 0.75...1.25, "warmth": (-0.25)...0.25, "bloom": 0...0.6,
  ]
  package var values: [String: Float] {
    [
      "detail": detail, "roughnessBias": roughnessBias, "surfaceSaturation": surfaceSaturation,
      "sunStrength": sunStrength, "ambientStrength": ambientStrength,
      "skySaturation": skySaturation, "saturation": saturation, "contrast": contrast,
      "warmth": warmth, "bloom": bloom,
    ]
  }
  package mutating func set(_ edits: [String: Float]) throws {
    var candidate = values
    for (key, value) in edits {
      guard let range = Self.ranges[key], value.isFinite, range.contains(value) else {
        throw RuntimeError.message("Invalid scene-look value: \(key)")
      }
      candidate[key] = value
    }
    self = try JSONDecoder().decode(Self.self, from: JSONEncoder().encode(candidate))
  }
  package func validate() throws {
    var copy = self
    try copy.set(values)
  }
  package var surface: SIMD4<Float> { SIMD4(detail, roughnessBias, surfaceSaturation, 0) }
  package var light: SIMD4<Float> { SIMD4(sunStrength, ambientStrength, skySaturation, 0) }
  package var grade: SIMD4<Float> { SIMD4(saturation, contrast, warmth, bloom) }
}
