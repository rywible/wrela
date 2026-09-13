import Foundation

/// The renderer's projected-error LOD rule, kept pure so source-level presentation tests can
/// exercise the same thresholds as a production frame. `errors[index]` describes LOD
/// `index + 1`; level zero is the unreduced source mesh.
package enum SceneLODSelection {
  package static func selectedLevel<Errors: Collection>(
    currentLOD: Int, errors: Errors, scale: Float, distance: Float, kind: Float,
    top: Float, wind: Float, shadow: Bool, shadowTexelsPerMetre: Float
  ) -> Int where Errors.Element == Float {
    var selected = 0
    for (index, delta) in errors.enumerated() {
      let level = index + 1
      var error = delta * scale
      if kind == 8 {
        // response components are bounded by ±0.35; bilinear
        // 10 m cells have Jacobian norm at most 0.14/m.
        // Bound both changing response and y² bend weight.
        let amplitude = 0.35 * sqrt(2) * abs(wind)
        let slope = 0.14 * abs(wind)
        error +=
          amplitude * 0.03 * delta * (2 * top + delta) + 0.03 * pow(top + delta, 2) * slope
          * scale * delta
      }
      if shadow
        ? error * shadowTexelsPerMetre < 0.8
        : error * 935 / distance < (level == currentLOD ? 0.65 : 0.4)
      {
        selected = level
      }
    }
    return selected
  }
}
