import FieldCore
import FieldEngine
import Foundation

/// An authored anatomy recipe. The document stores intent/proportions; this
/// recipe expands them into named fields and attachment frames for the compiler.
/// A different species gets its own recipe, rather than an unbounded universal rig.
enum FrostlingRecipe {
  struct Control {
    var key: String
    var title: String
    var initial: Float
    var range: ClosedRange<Float>
  }
  static let controls: [Control] = [
    .init(key: "headSize", title: "Head size", initial: 1, range: 0.9...1.2),
    .init(key: "cheeks", title: "Cheek fullness", initial: 1, range: 0.9...1.15),
    .init(key: "bodyRoundness", title: "Body roundness", initial: 1, range: 0.9...1.15),
    .init(key: "bodyLength", title: "Body length", initial: 1, range: 0.85...1.15),
    .init(key: "earLength", title: "Ear length", initial: 1, range: 0.75...1.15),
    .init(key: "earWidth", title: "Ear width", initial: 1, range: 0.8...1.2),
    .init(key: "earSplay", title: "Ear splay · degrees", initial: 9, range: 0...20),
    .init(key: "eyeSize", title: "Eye size", initial: 1, range: 0.8...1.2),
    .init(key: "eyeSpacing", title: "Eye spacing", initial: 1, range: 0.85...1.1),
    .init(key: "pawSize", title: "Paw size", initial: 1, range: 0.85...1.15),
    .init(key: "tailSize", title: "Tail size", initial: 1, range: 0.75...1.2),
  ]
  static var ranges: [String: ClosedRange<Float>] {
    Dictionary(uniqueKeysWithValues: controls.map { ($0.key, $0.range) })
  }
  static var defaults: [String: Float] {
    Dictionary(uniqueKeysWithValues: controls.map { ($0.key, $0.initial) })
  }
  static func parts(_ parameters: [String: Float]) -> [AssetPart] {
    let values = defaults.merging(parameters) { _, new in new }
    func value(_ key: String) -> Float { values[key]! }
    let head = value("headSize")
    let cheeks = value("cheeks")
    let roundness = value("bodyRoundness")
    let bodyLength = value("bodyLength")
    let earLength = value("earLength")
    let earWidth = value("earWidth")
    let eye = value("eyeSize")
    let spacing = value("eyeSpacing")
    let paw = value("pawSize")
    let headFrame = V3(0, 0.78, -0.34)
    func headPoint(_ p: V3) -> V3 { headFrame + (p - headFrame) * head }
    let fur = V3(0.69, 0.78, 0.79)
    let face = V3(0.79, 0.84, 0.82)
    var result: [AssetPart] = []
    func part(
      _ id: String, _ name: String, parent: String? = nil, pivot: V3 = .zero,
      color: V3 = V3(0.69, 0.78, 0.79), roughness: Float = 0.88, material: Int = 9,
      field: FieldExpression
    ) {
      result.append(
        AssetPart(
          name: name, joint: PartJoint(id: id, parent: parent, pivot: pivot), field: field,
          color: [color.x, color.y, color.z], material: material, roughness: roughness, metallic: 0)
      )
    }
    // Low, rounded silhouette with a continuous shoulder and belly; no exposed stick legs.
    part(
      "body", "Rounded body", pivot: V3(0, 0.43, 0.15), color: fur,
      field: .ellipsoid(V3(0, 0.43, 0.15), V3(0.33 * roundness, 0.35, 0.45 * bodyLength))
        .blended(with: .ellipsoid(V3(0, 0.52, -0.12), V3(0.265, 0.30, 0.29)), radius: 0.12))
    part(
      "head", "Head and cheeks", parent: "body", pivot: headPoint(V3(0, 0.70, -0.23)), color: face,
      field: .ellipsoid(headFrame, V3(0.265 * cheeks, 0.255, 0.26) * head)
        .blended(
          with: .ellipsoid(headPoint(V3(0, 0.69, -0.51)), V3(0.175 * cheeks, 0.115, 0.145) * head),
          radius: 0.075 * head))
    for (side, sign) in [("left", Float(-1)), ("right", Float(1))] {
      let base = headPoint(V3(sign * 0.14, 0.95, -0.28))
      let center = base + V3(0, 0.19 * earLength, 0)
      part(
        "ear-" + side, side.capitalized + " ear", parent: "head", pivot: base, color: fur,
        field: .ellipsoid(center, V3(0.087 * earWidth, 0.29 * earLength, 0.060) * head))
      result[result.count - 1].joint!.rotation.z = -sign * value("earSplay")
      part(
        "lining-" + side, side.capitalized + " ear lining", parent: "ear-" + side, pivot: base,
        color: V3(0.50, 0.64, 0.68),
        field: .ellipsoid(
          center + V3(0, 0.015, -0.055) * head,
          V3(0.046 * earWidth, 0.215 * earLength, 0.012) * head))
      part(
        "front-" + side, side.capitalized + " forepaw", pivot: V3(sign * 0.22, 0.10, -0.27),
        color: V3(0.67, 0.76, 0.78),
        field: .ellipsoid(V3(sign * 0.22, 0.10 * paw, -0.27), V3(0.115, 0.10, 0.175) * paw)
          .blended(
            with: .ellipsoid(V3(sign * 0.22, 0.21, -0.21), V3(0.083, 0.14, 0.09)), radius: 0.04))
      part(
        "hind-" + side, side.capitalized + " hind paw", pivot: V3(sign * 0.25, 0.10, 0.37),
        color: V3(0.63, 0.74, 0.77),
        field: .ellipsoid(V3(sign * 0.25, 0.10 * paw, 0.37), V3(0.135, 0.10, 0.225) * paw))
    }
    part(
      "tail", "Soft tail", parent: "body", pivot: V3(0, 0.48, 0.56), color: V3(0.83, 0.86, 0.82),
      field: .ellipsoid(V3(0, 0.49, 0.68), V3(0.18, 0.19, 0.20) * value("tailSize")))
    // Seat eyes on the head surface. Spacing changes recompute depth rather than
    // leaving black beads floating in front of the face.
    let eyeX: Float = 0.185 * spacing
    let eyeY: Float = 0.825
    let rx: Float = 0.265 * cheeks
    let eyeZ =
      headFrame.z - 0.26
      * sqrt(max(0.05, 1 - pow(eyeX / rx, 2) - pow((eyeY - headFrame.y) / 0.255, 2)))
    let eyePivot = headPoint(V3(0, eyeY, eyeZ))
    let eyeShape = FieldExpression.ellipsoid(
      headPoint(V3(-eyeX, eyeY, eyeZ)), V3(0.035, 0.047, 0.025) * eye * head
    )
    .joined(with: .ellipsoid(headPoint(V3(eyeX, eyeY, eyeZ)), V3(0.035, 0.047, 0.025) * eye * head))
    part(
      "eyes", "Blinking eyes", parent: "head", pivot: eyePivot, color: V3(0.035, 0.055, 0.065),
      roughness: 0.12, material: 7, field: eyeShape)
    part(
      "nose", "Little nose", parent: "head", pivot: headPoint(V3(0, 0.713, -0.649)),
      color: V3(0.19, 0.30, 0.34), roughness: 0.38, material: 7,
      field: .ellipsoid(headPoint(V3(0, 0.713, -0.649)), V3(0.037, 0.024, 0.02) * head))
    return result
  }
}
