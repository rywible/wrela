import Foundation

/// An optional compiled clump volume beneath the individual visible fibres.
/// Dimensions are relative to the groom's authored width; no creature identity
/// or runtime mesh indices are stored in the source.
public struct GroomEnvelope: Codable, Equatable, Sendable {
  public var coverage: Float
  public var taper: Float
  public var flatten: Float
  public var ridge: Float
  /// Nested coverage surfaces provide depth between unresolved strands. Inner
  /// layers are narrower and shorter; all follow the same authored guide.
  public var layers: Int
  /// Smaller independently tapered locks share each guide's binding and flow.
  public var clumps: Int

  public init(coverage: Float = 0.96, taper: Float = 0.42,
    flatten: Float = 0.72, ridge: Float = 0.045, layers: Int = 1, clumps: Int = 1) {
    self.coverage = coverage; self.taper = taper
    self.flatten = flatten; self.ridge = ridge
    self.layers = layers
    self.clumps = clumps
  }

  private enum CodingKeys: String, CodingKey { case coverage, taper, flatten, ridge, layers, clumps }
  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    coverage = try c.decode(Float.self, forKey: .coverage)
    taper = try c.decode(Float.self, forKey: .taper)
    flatten = try c.decode(Float.self, forKey: .flatten)
    ridge = try c.decode(Float.self, forKey: .ridge)
    layers = try c.decodeIfPresent(Int.self, forKey: .layers) ?? 1
    clumps = try c.decodeIfPresent(Int.self, forKey: .clumps) ?? 1
  }

  public func validate(id: String) throws {
    try CraftError.require([coverage,taper,flatten,ridge].allSatisfy(\.isFinite)
      && (0.05...1.5).contains(coverage) && (0.1...3).contains(taper)
      && (0.1...1).contains(flatten) && (0...0.2).contains(ridge) && (1...4).contains(layers) && (1...8).contains(clumps),
      "groom.\(id).envelope", "Use coverage 0.05…1.5, taper 0.1…3, flatten 0.1…1, ridge 0…0.2, layers 1…4 and clumps 1…8")
  }
}
