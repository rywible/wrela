import FieldCore
import Foundation
import simd

public struct Solid {
  public var shape: Shape
  public var position: V3
  public var scale: Float
  public var name: String
  public init(shape: Shape, position: V3, scale: Float, name: String) {
    self.shape = shape
    self.position = position
    self.scale = scale
    self.name = name
  }
  public func value(_ p: V3) -> Float { shape.value(at: (p - position) / scale) * scale }
}
public struct Placement {
  public var position: V3
  public var scale: V3
  public var yaw: Float
  public var color: V3
  public var kind: Float
  public init(position: V3, scale: V3, yaw: Float = 0, tint: V3, kind: Float) {
    self.position = position
    self.scale = scale
    self.yaw = yaw
    self.color = tint
    self.kind = kind
  }
}
/// Shared CPU layout: rendering and collision consume the same placement sequence.
public struct SanctuaryLayout {
  public static let podMetres: Float = 0.035
  public static var podUnitScale: Float {
    podMetres / (podShape.bounds.max.y - podShape.bounds.min.y)
  }
  public let terrain = Terrain()
  public var trunks: [Placement] = [], crowns: [Placement] = [], rocks: [Placement] = [],
    solids: [Solid] = []
  public var collisionGrid = SpatialBoundsGrid(bounds: [])
  public var remainingRandom: SeededRandom
  public let seed: UInt64 = 82317
  public static var arch: Shape {
    Shape.capsule(V3(-3, 0, 0), V3(-2, 8, 0), 0.85).blended(
      .capsule(V3(3, 0, 0), V3(2, 8, 0), 0.85), radius: 0.5
    ).blended(.capsule(V3(-2, 8, 0), V3(2, 8, 0), 1), radius: 0.75)
  }
  public var gatePosition: V3 {
    V3(terrain.pathX(-67), terrain.height(terrain.pathX(-67), -67), -67)
  }
  public init(parameters: [String: Float] = [:]) {
    remainingRandom = SeededRandom(seed: 82317)
    let trunk = Self.treeFields(parameters).0
    let rockShape = Self.stoneField

    func encounterClearing(_ x: Float, _ z: Float) -> Bool {
      let p = SIMD2(x, z)
      return distance(p, Expedition.den) < 6 || distance(p, Expedition.home) < 7
        || Expedition.signs.contains { distance(p, $0) < 2 }
    }
    var rng = SeededRandom(seed: seed)
    for _ in 0..<260 {
      let x = rng.range(-105, 105)
      let z = rng.range(-125, 65)
      if encounterClearing(x, z) { continue }
      let pathDistance = abs(x - terrain.pathX(z))
      if pathDistance < 5 || (abs(x) < 13 && z > 9 && z < 33) { continue }
      let s = rng.range(1.1, 2.3)
      let p = V3(x, terrain.height(x, z) - 0.15, z)
      let yaw = rng.range(0, 6.28)
      trunks.append(
        Placement(
          position: p, scale: V3(repeating: s), yaw: yaw, tint: V3(0.31, 0.25, 0.18), kind: 4))
      let gold = rng.next()
      let color =
        gold < 0.22
        ? V3(0.73, 0.70, 0.32)
        : V3(rng.range(0.33, 0.48), rng.range(0.57, 0.70), rng.range(0.27, 0.39))
      crowns.append(Placement(position: p, scale: V3(repeating: s), yaw: yaw, tint: color, kind: 2))
      solids.append(Solid(shape: trunk, position: p, scale: s, name: "Tree"))
    }
    for _ in 0..<95 {
      let x = rng.range(-75, 75)
      let z = rng.range(-105, 50)
      if abs(x - terrain.pathX(z)) < 3.4 || encounterClearing(x, z) { continue }
      let s = rng.range(0.5, 2.3)
      let p = V3(x, terrain.height(x, z), z)
      rocks.append(
        Placement(position: p, scale: V3(repeating: s), tint: V3(0.53, 0.57, 0.51), kind: 5))
      solids.append(Solid(shape: rockShape, position: p, scale: s, name: "Stone"))
    }

    remainingRandom = rng
    solids.append(Solid(shape: Self.arch, position: gatePosition, scale: 1, name: "The old gate"))
    collisionGrid = SpatialBoundsGrid(
      bounds: solids.map {
        let b = $0.shape.bounds
        return Bounds(b.min * $0.scale + $0.position, b.max * $0.scale + $0.position).expanded(
          0.28 / $0.shape.exteriorDistanceScale)
      })
  }
  public static func treeFields(_ parameters: [String: Float] = [:]) -> (Shape, Shape) {
    let h = (parameters["height"] ?? 5.6) / 5.6
    let w = (parameters["width"] ?? 6) / 6
    let r = parameters["trunk"] ?? 0.24
    let spread = parameters["spread"] ?? 1
    let trunk = Shape.capsule(V3(0, 0, 0), V3(0.12, 3.6, 0.1), r)
      .joined(.capsule(V3(0, 2.1, 0), V3(1.2 * spread, 3.4, 0.1), r * 0.54))
      .joined(.capsule(V3(0, 2.4, 0), V3(-spread, 3.65, 0.2), r * 0.58))
    let crown = Shape.sphere(1).stretched(V3(2.05, 0.85, 1.65)).moved(V3(0, 4.2, 0))
      .blended(Shape.sphere(1).stretched(V3(1.5, 0.7, 1.35)).moved(V3(1.4, 3.9, 0.2)), radius: 0.3)
      .blended(
        Shape.sphere(1).stretched(V3(1.65, 0.7, 1.4)).moved(V3(-1.3, 4.3, 0.1)), radius: 0.25
      )
      .blended(
        Shape.sphere(1).stretched(V3(1.45, 0.7, 1.25)).moved(V3(-0.25, 4.95, 0.1)), radius: 0.25
      )
      .blended(
        Shape.sphere(1).stretched(V3(1.3, 0.65, 1.2)).moved(V3(0.1, 4.0, -1.15)), radius: 0.3)
    return (trunk.stretched(V3(w, h, w)), crown.stretched(V3(w, h, w)))
  }
  public static var stoneField: Shape {
    Shape.sphere(0.9).blended(.sphere(0.65).moved(V3(0.55, -0.08, 0.12)), radius: 0.3).cut(
      .box(V3(2, 1, 2)).moved(V3(0, -1.35, 0)))
  }
  public static var podShape: Shape {
    Shape.sphere(1.15).blended(.sphere(0.72).moved(V3(0, 0.75, 0)), radius: 0.55)
      .blended(.capsule(V3(0, 1.1, 0), V3(0.48, 2.15, 0), 0.18), radius: 0.28)
      .cut(.sphere(0.82).moved(V3(0, 0.24, 0.85)))
  }
}
