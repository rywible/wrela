import simd

/// A sampled height surface. The compiler knows its geometry, not its world recipe.
public protocol HeightField: Sendable {
    func sample(_ x: Float, _ z: Float) -> (height: Float, normal: V3)
}
