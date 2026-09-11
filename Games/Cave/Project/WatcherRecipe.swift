import FieldCore
import FieldEngine
import simd

enum WatcherRecipe {
  static let controls: [ScalarControl] = [
    .init("height", "Body height", 1, 0.8...1.3), .init("reach", "Arm reach", 1, 0.7...1.4),
    .init("mask", "Mask width", 1, 0.75...1.3),
  ]
  static func parts(_ p: [String: Float]) -> [AssetPart] {
    let h = p["height"] ?? 1
    let r = p["reach"] ?? 1
    let m = p["mask"] ?? 1
    var parts = [AssetPart]()
    func add(_ id: String, _ parent: String?, _ pivot: V3, _ field: FieldExpression, _ tint: V3) {
      parts.append(
        AssetPart(
          name: id, joint: PartJoint(id: id, parent: parent, pivot: pivot), field: field,
          color: [tint.x, tint.y, tint.z], material: 7, roughness: 0.8))
    }
    let dark = V3(0.12, 0.14, 0.15)
    let bone = V3(0.65, 0.62, 0.51)
    add(
      "torso", nil, V3(0, 0.9 * h, 0),
      .capsule(V3(0, 0.7 * h, 0.12), V3(0, 1.55 * h, 0), 0.22).blended(
        with: .ellipsoid(V3(0, 1.45 * h, 0), V3(0.44, 0.18, 0.21)), radius: 0.08), dark)
    add(
      "mask", "torso", V3(0, 1.6 * h, 0),
      .ellipsoid(V3(0, 1.9 * h, -0.08), V3(0.18 * m, 0.32, 0.14)), bone)
    let sockets = FieldExpression.ellipsoid(
      V3(-0.075 * m, 1.98 * h, -0.208), V3(0.035, 0.07, 0.018)
    ).joined(with: .ellipsoid(V3(0.075 * m, 1.98 * h, -0.208), V3(0.035, 0.07, 0.018)))
    add("sockets", "mask", V3(0, 1.9 * h, 0), sockets, V3(0.012, 0.012, 0.014))
    for side: Float in [-1, 1] {
      let suffix = side < 0 ? "left" : "right"
      let shoulder = V3(side * 0.34, 1.45 * h, 0)
      let hand = V3(side * (0.52 + 0.13 * r), 0.38 * h, -0.18 * r)
      add(
        "arm-" + suffix, "torso", shoulder,
        .capsule(shoulder, V3(side * 0.57, 0.95 * h, 0), 0.075).blended(
          with: .capsule(V3(side * 0.57, 0.95 * h, 0), hand, 0.047), radius: 0.035), dark)
      for finger in 0..<3 {
        let x = Float(finger - 1) * 0.036
        add(
          "finger-\(suffix)-\(finger)", "arm-" + suffix, hand,
          .capsule(hand + V3(x, 0, 0), hand + V3(x * 1.3, -0.22 * r, -0.07), 0.018), bone * 0.55)
      }
      add(
        "leg-" + suffix, nil, V3(side * 0.14, 0.8 * h, 0),
        .capsule(V3(side * 0.14, 0.78 * h, 0), V3(side * 0.24, 0.1, 0), 0.09).blended(
          with: .ellipsoid(V3(side * 0.24, 0.075, -0.12), V3(0.1, 0.075, 0.22)), radius: 0.04), dark
      )
    }
    return parts
  }
}
