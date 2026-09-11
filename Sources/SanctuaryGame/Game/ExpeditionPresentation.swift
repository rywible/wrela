import FieldCompiler
import FieldCore
import SanctuaryContent
import simd

/// Compiled once. Gameplay changes transforms and visibility, never remeshes per frame.
final class ExpeditionPresentation {
  let creature: [GPUBatch]
  let stone: GPUBatch
  let frost: GPUBatch
  let flower: GPUBatch
  let ground: GPUBatch
  let terrain: Terrain

  init(graphics: MetalRenderer, terrain: Terrain) throws {
    self.terrain = terrain
    creature = try AssetSource.load("frostling").compile().map(graphics.upload)
    func compile(_ name: String, _ shape: Shape, _ color: V3, resolution: Int = 16) throws
      -> GPUBatch
    {
      var batch = SceneBatch(
        name: name, mesh: try Mesher.compile(shape, resolution: resolution),
        instances: [Instance(tint: color, kind: 7)])
      batch.roughness = 0.7
      return graphics.upload(batch)
    }
    stone = try compile(
      "Sanctuary marker", .sphere(1).stretched(V3(0.32, 0.40, 0.25)), V3(0.52, 0.58, 0.52))
    frost = try compile(
      "Frost trace", .sphere(1).stretched(V3(0.10, 0.035, 0.20)), V3(0.70, 0.86, 0.87))
    let stem = Shape.capsule(.zero, V3(0, 0.32, 0), 0.023)
    var petals = stem
    for i in 0..<5 {
      let a = Float(i) * 2 * .pi / 5
      petals = petals.joined(
        .sphere(1).stretched(V3(0.085, 0.035, 0.085)).moved(
          V3(cos(a) * 0.085, 0.31, sin(a) * 0.085)))
    }
    flower = try compile("Ice flower", petals, V3(0.47, 0.73, 0.84))
    // A thin, terrain-conforming patch with feathered color at its perimeter.
    var mesh = Mesh()
    let n = 48
    let home = Expedition.home
    for z in 0...n {
      for x in 0...n {
        let dx = (Float(x) / Float(n) * 2 - 1) * 5
        let dz = (Float(z) / Float(n) * 2 - 1) * 5
        let p = home + SIMD2(dx, dz)
        let sample = terrain.sample(p.x, p.y)
        let amount = max(0, 1 - length(SIMD2(dx, dz)) / 5)
        mesh.vertices.append(
          Vertex(
            V3(p.x, sample.height + 0.045, p.y), sample.normal,
            V3(0.37, 0.54, 0.24) + V3(0.25, 0.22, 0.52) * min(1, amount * 3)))
      }
    }
    for z in 0..<n {
      for x in 0..<n {
        let dx = (Float(x) / Float(n) * 2 - 1) * 5
        let dz = (Float(z) / Float(n) * 2 - 1) * 5
        if length(SIMD2(dx, dz)) > 4.8 { continue }
        let a = UInt32(z * (n + 1) + x)
        let b = a + 1
        let c = a + UInt32(n + 1)
        let d = c + 1
        mesh.indices += [a, c, b, b, c, d]
      }
    }
    ground = graphics.upload(
      SceneBatch(name: "Frost garden", mesh: mesh, instances: [Instance(kind: 7)]))
  }

  func items(_ state: Expedition) -> [RenderItem] {
    var result: [RenderItem] = []
    func place(
      _ batch: GPUBatch, _ p: SIMD2<Float>, height: Float = 0, scale: Float = 1, yaw: Float = 0,
      tint: V3 = V3(repeating: 1), shadow: Bool = true
    ) {
      let original = batch.sourceInstances[0].tint
      let instance = Instance(
        position: V3(p.x, terrain.height(p.x, p.y) + height, p.y), scale: V3(repeating: scale),
        yaw: yaw, tint: V3(original.x, original.y, original.z) * tint, kind: 7)
      result.append(RenderItem(batch: batch, instance: instance, castsShadow: shadow))
    }
    for i in 0..<10 {
      let a = Float(i) * 2 * .pi / 10
      place(stone, Expedition.home + SIMD2(cos(a), sin(a)) * 5.4, height: 0.15, scale: 0.85)
    }
    for (index, p) in Expedition.signs.enumerated() {
      for step in 0..<5 {
        for side: Float in [-1, 1] {
          place(
            frost, p + SIMD2(side * 0.20, Float(step - 2) * 0.38), height: 0.08, scale: 1.4,
            shadow: false)
        }
      }
      // A small ice flower remains a readable sign from standing height.
      place(flower, p, scale: state.discoveredSigns.contains(index) ? 0.7 : 1.15)
    }
    if state.phase == .settled {
      if state.habitat > 0.1 { result.append(RenderItem(batch: ground, castsShadow: false)) }
      var rng = SeededRandom(seed: 813)
      for i in 0..<48 {
        let a = rng.range(0, 2 * .pi)
        let r = sqrt(rng.next()) * 4.6
        let size = rng.range(0.7, 1.7)
        let growth = clamp(state.habitat * 1.5 - Float(i) / 96, 0, 1)
        if growth > 0.01 {
          place(flower, Expedition.home + SIMD2(cos(a), sin(a)) * r, scale: growth * size)
        }
      }
    }
    if state.phase != .carrying {
      let t = Float(state.age.truncatingRemainder(dividingBy: 4096))
      let p = state.creaturePosition
      let facing =
        state.trust > 0.2 && state.phase == .searching
        ? atan2(state.player.x - p.x, -(state.player.y - p.y)) : 0.35 * sin(t * 0.18)
      for (index, batch) in creature.enumerated() {
        var offset: Float = 0
        if (4...7).contains(index) {
          offset = max(0, sin(t * 2.4 + Float(index % 2) * Float.pi)) * 0.035
        } else if index == 0 {
          offset = sin(t * 1.8) * 0.012
        } else if [2, 3, 11].contains(index) {
          offset = sin(t * 1.1) * 0.012
        }
        place(batch, p, height: offset, yaw: facing)
      }
    }
    return result
  }
}
