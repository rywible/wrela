import Foundation

/// The quiet starting cabin is authored world content. Player construction stays
/// independent and may be placed anywhere permitted by the same collision rules.
public enum SanctuaryHome {
  public static let construction: PersonalConstruction = {
    var result = PersonalConstruction()
    let terrain = Terrain()
    // Door faces local -Z, toward the garden and the first animal greeting.
    try! result.apply(.place(.cabin,
      at: .init(x: 0, y: terrain.height(0, 26), z: 26), yawRadians: 0, scale: 1),
      expectedRevision: result.revision)
    return result
  }()
}
