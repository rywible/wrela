import FieldCore

/// Code-first field recipes; JSON stores only the resulting authoring parameters.
extension FieldExpression {
  package static func sphere(_ radius: Float) -> Self { Self(op: "sphere", values: [radius]) }
  package static func ellipsoid(_ center: V3, _ radii: V3) -> Self {
    sphere(1).stretched(radii).moved(center)
  }
  package static func capsule(_ a: V3, _ b: V3, _ radius: Float) -> Self {
    Self(op: "capsule", values: [a.x, a.y, a.z, b.x, b.y, b.z, radius])
  }
  package func moved(_ p: V3) -> Self {
    Self(op: "move", values: [p.x, p.y, p.z], children: [self])
  }
  package func stretched(_ scale: V3) -> Self {
    Self(op: "stretch", values: [scale.x, scale.y, scale.z], children: [self])
  }
  package func joined(with other: Self) -> Self { Self(op: "union", children: [self, other]) }
  package func blended(with other: Self, radius: Float) -> Self {
    Self(op: "blend", values: [radius], children: [self, other])
  }
}
