import FieldCore
import Foundation
import simd

public struct Terrain: HeightField {
  public init() {}
  public var semantics: FieldSemantics { .implicit }
  public func pathX(_ z: Float) -> Float { sin(z * 0.048) * 9 + sin(z * 0.105) * 2 }
  public func height(_ x: Float, _ z: Float) -> Float {
    let base =
      0.8 * sin(x * 0.055 + z * 0.019) + 1.25 * sin(z * 0.045) + 0.42 * cos(x * 0.13 - z * 0.08)
    let flank = pow(abs(x) / 75, 2) * 15
    let hill = 8 * exp(-((x + 34) * (x + 34) / 700 + (z + 45) * (z + 45) / 1400))
    let ridge =
      17 * exp(-((z + 122) * (z + 122) / 450)) * pow(0.55 + 0.45 * sin(x * 0.066 + 0.7), 2)
    return base + flank + hill + ridge
  }
  public func value(at p: V3) -> Float { p.y - height(p.x, p.z) }
  public func sample(_ x: Float, _ z: Float) -> (height: Float, normal: V3) {
    let phase = x * 0.055 + z * 0.019
    let phase2 = x * 0.13 - z * 0.08
    let hx = x + 34
    let hz = z + 45
    let rz = z + 122
    let H = exp(-(hx * hx / 700 + hz * hz / 1400))
    let R = exp(-(rz * rz / 450))
    let wave = 0.55 + 0.45 * sin(x * 0.066 + 0.7)
    let base = 0.8 * sin(phase) + 1.25 * sin(z * 0.045) + 0.42 * cos(phase2)
    let height = base + pow(abs(x) / 75, 2) * 15 + 8 * H + 17 * R * pow(wave, 2)
    let dx =
      0.044 * cos(phase) - 0.0546 * sin(phase2) + 2 * x / 375 - 16 * hx * H / 700 + 17 * 0.9 * 0.066
      * R * wave * cos(x * 0.066 + 0.7)
    let dz =
      0.0152 * cos(phase) + 0.05625 * cos(z * 0.045) + 0.0336 * sin(phase2) - 16 * hz * H / 1400
      - 34 * rz * R * wave * wave / 450
    return (height, normalize(V3(-dx, 1, -dz)))
  }
  public func normal(_ x: Float, _ z: Float) -> V3 { sample(x, z).normal }

}
