import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import simd

/// Regional substrate appearance, evaluated only when compiling a terrain representation.
/// These are appearance affinities from existing geography/grade/water facts, not soil physics.
/// Vertex RGB remains real albedo; material 14 only modulates that authored substrate.
enum BiomeGroundAppearance {
  static let materialKind: Float = 14

  struct Sample {
    let albedo: V3
    let bankDampness: Float
    let exposedStone: Float
    let routeWear: Float
  }

  static func biomeAlbedo(_ biome: SanctuaryBiome) -> V3 {
    switch biome {
    case .woodland: return V3(0.35, 0.40, 0.26)
    case .meadow: return V3(0.47, 0.50, 0.31)
    case .creek: return V3(0.44, 0.43, 0.31)
    case .wetland: return V3(0.36, 0.40, 0.29)
    case .lake: return V3(0.46, 0.46, 0.36)
    case .alpine: return V3(0.49, 0.49, 0.45)
    case .desert: return V3(0.64, 0.54, 0.37)
    case .rainforest: return V3(0.33, 0.38, 0.25)
    case .coast: return V3(0.57, 0.53, 0.41)
    case .tidepool: return V3(0.43, 0.45, 0.42)
    case .ocean: return V3(0.36, 0.39, 0.37)
    }
  }

  static func sample(
    at coordinate: SIMD2<Float>, height: Float, normal: V3,
    terrain: Terrain, garden: HabitatGarden? = nil
  ) -> Sample {
    let geography = terrain.geography
    let weights = (try? geography.sample(at: coordinate).weights) ?? [.woodland: 1]
    var albedo = V3.zero
    for biome in SanctuaryBiome.allCases {
      albedo += biomeAlbedo(biome) * (weights[biome] ?? 0)
    }
    let stone = 1 - smoothstep(0.62, 0.94, normal.y)
    albedo = mix(albedo, V3(0.48, 0.48, 0.43), stone * 0.72)
    let wear = 1 - smoothstep(4, 15, geography.distanceToRoute(at: coordinate))
    albedo = mix(albedo, V3(0.51, 0.44, 0.32), wear * 0.58)

    // Pass the existing vertex height. All five signed water fields remain cheap analytic
    // coverage queries; no extra Terrain.height/normal sampling or upwind traversal occurs.
    var coverage = -Float.infinity
    for body in SanctuaryWaterBody.allCases {
      if let field = terrain.waterField(for: body, at: coordinate, bedHeight: height) {
        coverage = max(coverage, field.signedCoverage)
      }
    }
    if let garden, let field = garden.waterField(
      baseHeight: height, at: .init(x: coordinate.x, z: coordinate.y)) {
      coverage = max(coverage, field.signedCoverage)
    }
    let damp = smoothstep(-16, -2, coverage)
    albedo = mix(albedo, V3(0.29, 0.30, 0.25), damp * (0.86 - stone * 0.22))
    return Sample(albedo: albedo, bankDampness: damp, exposedStone: stone, routeWear: wear)
  }

  private static func smoothstep(_ low: Float, _ high: Float, _ value: Float) -> Float {
    let t = min(1, max(0, (value - low) / (high - low)))
    return t * t * (3 - 2 * t)
  }

  private static func mix(_ a: V3, _ b: V3, _ t: Float) -> V3 { a * (1 - t) + b * t }

  static var generators: [AssetGenerator] {
    let sites: [(String, String, SIMD2<Float>)] = [
      ("ground-creek", "Ground · Creek bank", SIMD2(1_650.29, 2_170)),
      ("ground-alpine", "Ground · Alpine earth and stone", SIMD2(-460, 10_690)),
      ("ground-meadow", "Ground · Meadow soil", SIMD2(2_400, -740)),
    ]
    return sites.map { id, title, center in
      AssetGenerator(id: id, name: title, controls: [
        ScalarControl("span", "Patch span · m", 12, 6...24),
        ScalarControl("eastOffset", "East offset · m", 0, -20...20),
        ScalarControl("northOffset", "North offset · m", 0, -20...20),
      ], compile: { source in
        let p = source.parameters
        let coordinate = center + SIMD2(p["eastOffset"] ?? 0, p["northOffset"] ?? 0)
        return [studyPatch(center: coordinate, span: p["span"] ?? 12, title: title)]
      })
    }
  }

  private static func studyPatch(center: SIMD2<Float>, span: Float, title: String) -> SceneBatch {
    let terrain = Terrain()
    let resolution = 32
    var mesh = Mesh()
    for z in 0...resolution {
      for x in 0...resolution {
        let p = center + SIMD2(Float(x) / Float(resolution) - 0.5,
          Float(z) / Float(resolution) - 0.5) * span
        let h = terrain.height(p.x, p.y)
        let e: Float = 0.25
        let dx = terrain.height(p.x + e, p.y) - terrain.height(p.x - e, p.y)
        let dz = terrain.height(p.x, p.y + e) - terrain.height(p.x, p.y - e)
        let normal = normalize(V3(-dx, 2 * e, -dz))
        let substrate = sample(at: p, height: h, normal: normal, terrain: terrain)
        // Keep source coordinates in local vertices, matching regional terrain exactly.
        // Translation centers the study only; material detail uses these retained coordinates.
        mesh.vertices.append(Vertex(V3(p.x, h, p.y), normal, substrate.albedo))
      }
    }
    for z in 0..<resolution {
      for x in 0..<resolution {
        let a = UInt32(z * (resolution + 1) + x), b = a + 1
        let c = a + UInt32(resolution + 1), d = c + 1
        mesh.indices += [a, c, b, b, c, d]
      }
    }
    let origin = V3(center.x, terrain.height(center.x, center.y), center.y)
    return SceneBatch(name: title, mesh: mesh,
      instances: [Instance(position: -origin, kind: materialKind)], doubleSided: true)
  }
}
