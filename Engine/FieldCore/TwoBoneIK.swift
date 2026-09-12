import simd

/// Analytic two-link reach with a pole vector. Unreachable targets are clamped;
/// the residual is exposed so authoring cannot silently call a missed contact valid.
public enum TwoBoneIK {
  public struct Solution: Sendable {
    public var joint: V3
    public var end: V3
    public var residual: Float
  }
  public static func solve(root: V3, target: V3, pole: V3, upper: Float, lower: Float) -> Solution {
    precondition(upper > 0 && lower > 0)
    let delta = target - root
    let distance = length(delta)
    let direction = distance > 1e-6 ? delta / distance : V3(0, -1, 0)
    let d = min(upper + lower - 0.00001, max(abs(upper - lower) + 0.00001, distance))
    let x = (upper * upper - lower * lower + d * d) / (2 * d)
    let y = sqrt(max(0, upper * upper - x * x))
    var bend = pole - root - direction * dot(pole - root, direction)
    if length_squared(bend) < 1e-8 {
      bend = cross(direction, abs(direction.x) < 0.9 ? V3(1, 0, 0) : V3(0, 1, 0))
    }
    let end = root + direction * d
    return Solution(
      joint: root + direction * x + normalize(bend) * y, end: end, residual: length(end - target))
  }
  public static func rotation(from: V3, to: V3) -> V3 {
    let q = simd_quatf(from: normalize(from), to: normalize(to)).vector
    let x = atan2(2 * (q.w * q.x + q.y * q.z), 1 - 2 * (q.x * q.x + q.y * q.y))
    let y = asin(min(1, max(-1, 2 * (q.w * q.y - q.z * q.x))))
    let z = atan2(2 * (q.w * q.z + q.x * q.y), 1 - 2 * (q.y * q.y + q.z * q.z))
    return V3(x, y, z) * (180 / .pi)
  }
}
