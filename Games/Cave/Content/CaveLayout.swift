import FieldCore
import simd

/// A bounded, fully volumetric rock field: the void has floors, walls and ceilings.
public enum CaveLayout {
  public static let entrance = V3(0, 1.72, 4)
  public static let relic = V3(3, 0.7, -42)
  public static func shell(width: Float = 1, ceiling: Float = 1) -> Shape {
    let points: [V3] = [V3(0, 2, 6), V3(0, 2, -12), V3(-5, 2, -23), V3(3, 2, -33), V3(3, 2, -44)]
    var air = Shape.sphere(3.4).moved(points[0])
    for i in 1..<points.count {
      air = air.blended(.capsule(points[i - 1], points[i], 2.5 * width), radius: 0.8)
    }
    air = air.blended(.capsule(V3(0, 2, -12), V3(8, 2, -22), 2.2 * width), radius: 0.7)
      .blended(.capsule(V3(8, 2, -22), V3(3, 2, -33), 2.3 * width), radius: 0.7)
      .blended(.sphere(4.1 * width).stretched(V3(1, ceiling, 1)).moved(V3(3, 2, -42)), radius: 0.8)
      .blended(.capsule(V3(-5, 2, -23), V3(-10, 2, -28), 2.0), radius: 0.6)
    return Shape.box(V3(18, 7, 30)).moved(V3(0, 2, -20)).cut(air)
      .joined(.box(V3(18, 1, 30)).moved(V3(0, -1, -20)))
  }
  public static var outcrops: [Shape] {
    [(Float(0), Float(-5)), (0, -10), (-4, -20), (-4, -25), (2, -33), (3, -40)].flatMap { x, z in
      [
        Shape.sphere(1).stretched(V3(0.5, 1.2, 0.65)).moved(V3(x + 1.8, 3.5, z)),
        Shape.sphere(1).stretched(V3(0.8, 0.65, 0.9)).moved(V3(x - 2.2, 0.35, z - 1)),
      ]
    }
  }
  public static func rock(width: Float = 1, ceiling: Float = 1) -> Shape {
    outcrops.reduce(shell(width: width, ceiling: ceiling)) { $0.joined($1) }
  }
}
