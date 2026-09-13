import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

struct GardenWorld {
  static let podMetres: Float = 0.035
  static var podUnitScale: Float { podMetres / (podShape.bounds.max.y - podShape.bounds.min.y) }
  let terrain: Terrain
  var batches: [SceneBatch] = []
  var layout: SanctuaryLayout
  var solids: [Solid] { layout.solids }
  var collisionGrid: SpatialBoundsGrid { layout.collisionGrid }
  var studioMesh: Mesh
  var treeSource = AssetSource.defaults("tree")
  var studioShape: Shape
  let seed: UInt64 = 82317
  private(set) var streamedKeys: Set<SanctuaryTerrainChunkKey> = []
  private var streamedBatchNames: Set<String> = []
  private var streamedBatchesByKey: [SanctuaryTerrainChunkKey: [SceneBatch]] = [:]
  private var streamedGardenSurface = SanctuaryGardenSurfaceSource(garden: nil)
  private var streamedBoulderCoverage: Set<SanctuaryTerrainChunkKey> = []
  private var streamedBoulderRevision: UInt64?
  var biomeTemplates: BiomePresentationTemplates!
  private var distantTerrain: BiomeWorldPresentation.DistantTerrainCache!
  private let streamWorker = SanctuaryStreamWorker()
  private var streamRequestKey: SanctuaryStreamRequestKey?
  private(set) var streamGeneration: UInt64 = 0
  private var settledStreamGeneration: UInt64?
  private var forcedStreamGeneration: UInt64?
  private var collisionPreparationMilliseconds: Double = 0
  private var uploadPreparationMilliseconds: Double = 0
  private var preparedUploadBatchCount = 0
  private var preparedUploadTemplateHits = 0
  private var preparedUploadMetadataBytes = 0
  private var regeneratedTerrainChunks = 0
  private var terrainDirtyCauses: [String] = []
  private var rebuiltCabinTerrain = false
  private var rebuiltDistantTerrain = false
  private var vegetationResidency: SanctuaryBiomeVegetationResidency
  private var vegetationRecords: [SanctuaryTerrainChunkKey: [SanctuaryVegetationRecord]] = [:]
  private var vegetationPlacements: [SanctuaryTerrainChunkKey: [SanctuaryVegetationPlacement]] = [:]
  private var vegetationBatchNames: Set<String> = []
  private var displayedVegetationRevision: UInt64 = 0
  private var resolvedGarden: HabitatGarden?
  private var resolvedConstruction: PersonalConstruction?
  private(set) var publishedGarden: HabitatGarden?
  private(set) var publishedBoulders: BoulderArrangements?
  private(set) var publishedConstruction: PersonalConstruction?
  private var vegetationExcludedCount = 0
  private var vegetationResolvedCount = 0
  private var cabinGrassDescriptorsBefore = 0
  private var cabinGrassDescriptors = 0
  private var cabinGrassClumps = 0
  private var cabinFlowerStems = 0
  private var canonicalCabinGrass: [GrassBlade] = []
  private var canonicalCabinFlowers = Mesh()
  private var cabinFlowerSupports: [SIMD4<Float>] = []
  private var cabinGroundCoverMask: SanctuaryGroundCoverMask?
  private var cabinVisibleGrassIndices: [Int] = []
  private var cabinVisibleFlowerIndices: [Int] = []
  private var cabinGroundCoverFilterMilliseconds: Double = 0
  private var cabinGroundCoverBatchesRebuilt = 0

  var bounds: SanctuaryMapBounds { SanctuaryGeography.bounds }

  init(terrain: Terrain = Terrain(), source: AssetSource? = nil,
    initialPosition: V3 = V3(0, 0, 24), garden: HabitatGarden? = nil,
    boulders: BoulderArrangements? = nil, construction: PersonalConstruction? = nil) throws {
    self.terrain = terrain
    vegetationResidency = SanctuaryBiomeVegetationResidency(
      population: .init(habitat: .init(terrain: terrain)))
    publishedGarden = garden
    publishedBoulders = boulders
    publishedConstruction = construction
    func mesh(_ s: Shape, _ r: Int) throws -> Mesh { try Mesher.compile(s, resolution: r) }
    // A sculptural seed pod exercises smooth composition and subtraction.
    studioShape = Self.podShape
    studioMesh = try mesh(studioShape, 58)
    distantTerrain = BiomeWorldPresentation.DistantTerrainCache(terrain: terrain)
    batches.append(
      SceneBatch(
        name: "Cabin terrain",
        mesh: BiomeWorldPresentation.cabinTerrainMesh(terrain: terrain, garden: garden),
        instances: [Instance(kind: 3)], roughness: 0.95))

    treeSource = try source ?? AssetSource.load("tree")
    let (branches, crown) = treeSource.treeFields()
    let trunkMesh = try mesh(branches, 48)
    let crownMesh = try mesh(crown, 48)
    let rockShape = AssetSource.stoneField
    let rockMesh = try mesh(rockShape, 40)
    biomeTemplates = try BiomePresentationTemplates(
      trunk: trunkMesh, broadCrown: crownMesh, stone: rockMesh)
    layout = SanctuaryLayout(parameters: treeSource.parameters, terrain: terrain)
    func instances(_ placements: [Placement]) -> [Instance] {
      placements.map {
        Instance(position: $0.position, scale: $0.scale, yaw: $0.yaw, tint: $0.color, kind: $0.kind)
      }
    }
    let trunks = instances(layout.trunks)
    let crowns = instances(layout.crowns)
    let rocks = instances(layout.rocks)
    var rng = layout.remainingRandom
    batches += [
      SceneBatch(
        name: "Trunks", mesh: trunkMesh, instances: trunks,
        lodMeshes: [
          MeshProcessing.simplify(trunkMesh, ratio: 0.45, error: 0.012),
          MeshProcessing.simplify(trunkMesh, ratio: 0.18, error: 0.035),
        ]),
      SceneBatch(
        name: "Canopies", mesh: crownMesh, instances: crowns,
        lodMeshes: [
          MeshProcessing.simplify(crownMesh, ratio: 0.35, error: 0.03),
          MeshProcessing.simplify(crownMesh, ratio: 0.12, error: 0.08),
        ]),
      SceneBatch(
        name: "Stones", mesh: rockMesh, instances: rocks,
        lodMeshes: [
          MeshProcessing.simplify(rockMesh, ratio: 0.4, error: 0.012),
          MeshProcessing.simplify(rockMesh, ratio: 0.15, error: 0.04),
        ]),
    ]

    batches.append(
      SceneBatch(
        name: "Leaves",
        mesh: Mesher.leaves(on: crownMesh, count: Int(treeSource.parameters["leaves"] ?? 1800)),
        instances: crowns.map {
          var i = $0
          i.tint.w = 8
          return i
        },
        lodMeshes: [
          Mesher.leaves(
            on: crownMesh, count: Int(treeSource.parameters["leaves"] ?? 1800), flattened: true)
        ]))
    for index in batches.indices
    where ["Trunks", "Canopies", "Leaves"].contains(batches[index].name) {
      batches[index].roughness = treeSource.appearance["roughness"] ?? -1
      batches[index].metallic = treeSource.appearance["metallic"] ?? 0
      for i in batches[index].instances.indices {
        let kind = batches[index].instances[i].tint.w
        batches[index].instances[i].tint *= treeSource.appearance["tint"] ?? 1
        batches[index].instances[i].tint.w = kind
      }
    }

    // A landmark assembled from bounded fields: two leaning uprights and a lintel.
    let arch = SanctuaryLayout.arch
    let ap = layout.gatePosition
    batches.append(
      SceneBatch(
        name: "Gate", mesh: try mesh(arch, 48),
        instances: [Instance(position: ap, tint: V3(0.58, 0.62, 0.53), kind: 5)]))

    var grass: [GrassBlade] = []
    var flowerStemIndices: Set<Int> = []
    var flowers = Mesh()
    for _ in 0..<24000 {
      let x = rng.range(-76, 76)
      let z = rng.range(-108, 54)
      if abs(x - terrain.pathX(z)) < rng.range(2.2, 3.4) { continue }
      let p = V3(x, terrain.height(x, z) - 0.015, z)
      let angle = rng.range(0, 6.28)
      let h = rng.range(0.18, 0.48)
      let w = rng.range(0.018, 0.045)
      let col = V3(rng.range(0.30, 0.49), rng.range(0.49, 0.64), rng.range(0.17, 0.30))
      grass.append(GrassBlade(anchor: p, angle: angle, height: h, width: w, color: col))
      if rng.next() < 0.065 && z > -60 {
        flowerStemIndices.insert(grass.count - 1)
        cabinFlowerSupports.append(SIMD4(p, h + 0.045))
        let fp = p + V3(0, h, 0)
        let fc = rng.next() < 0.6 ? V3(0.92, 0.71, 0.68) : V3(0.83, 0.85, 0.60)
        for j in 0..<5 {
          let a = Float(j) * 1.256
          let b = a + 1.0
          flowers.triangle(
            Vertex(fp, V3(0, 1, 0), fc, wind: 1),
            Vertex(fp + V3(cos(a) * 0.16, 0.045, sin(a) * 0.16), V3(0, 1, 0), fc, wind: 1),
            Vertex(fp + V3(cos(b) * 0.16, 0.045, sin(b) * 0.16), V3(0, 1, 0), fc, wind: 1))
        }
      }
    }
    cabinGrassDescriptorsBefore = grass.count
    let grassCommunity = BiomeVegetationDesign.grassCommunity(grass, flowerStemIndices: flowerStemIndices)
    grass = grassCommunity.blades
    cabinGrassDescriptors = grass.count
    cabinGrassClumps = grassCommunity.clumpCount
    cabinFlowerStems = flowerStemIndices.count
    canonicalCabinGrass = grass
    canonicalCabinFlowers = flowers
    batches += [
      SceneBatch(name: "Meadow", mesh: Mesh(), instances: [Instance(kind: 3)], grass: grass),
      SceneBatch(name: "Wildflowers", mesh: flowers, instances: [Instance(kind: 3)]),
    ]
    for index in batches.indices {
      batches[index].doubleSided = ["Meadow", "Wildflowers", "Leaves"].contains(batches[index].name)
    }

    _ = rebuildCabinGroundCover(garden: garden, construction: construction)
    _ = try buildTerrainStreaming(around: initialPosition, garden: garden, boulders: boulders, force: true)

  }

  /// Replaces a bounded 3 × 3 detail neighborhood. The returned batch names let the experience
  /// drop obsolete GPU uploads atomically after every replacement batch has uploaded.
  private mutating func buildTerrainStreaming(
    around position: V3, garden: HabitatGarden?, boulders: BoulderArrangements? = nil,
    force: Bool = false
  ) throws -> SanctuaryStreamUpdate? {
    let keys = Set(
      SanctuaryTerrainChunkKey.neighborhood(around: SIMD2(position.x, position.z), radius: 1))
    regeneratedTerrainChunks = 0
    terrainDirtyCauses = []
    rebuiltCabinTerrain = false
    rebuiltDistantTerrain = false
    let gardenSurface = SanctuaryGardenSurfaceSource(garden: garden)
    let surfaceChanged = gardenSurface != streamedGardenSurface
    let boulderRevision = boulders?.revision
    guard force || keys != streamedKeys || surfaceChanged
      || boulderRevision != streamedBoulderRevision
    else { return nil }
    let geography = SanctuaryGeography()
    let plans = keys.sorted { lhs, rhs in
      lhs.z == rhs.z ? lhs.x < rhs.x : lhs.z < rhs.z
    }.map { geography.plan(for: $0) }
    let gardenCoverage = gardenSurface.coveredChunkKeys
    let boulderCoverage: Set<SanctuaryTerrainChunkKey> = boulders?.sourceChunkKeys ?? []
    let leaving = streamedKeys.subtracting(keys)
    let entering = keys.subtracting(streamedKeys)
    let gardenDirty = gardenSurface.affectedChunkKeys(comparedTo: streamedGardenSurface).intersection(keys)
    if keys != streamedKeys { terrainDirtyCauses.append("detail-neighborhood") }
    if gardenSurface.terrainPatches != streamedGardenSurface.terrainPatches { terrainDirtyCauses.append("terrain-sculpt") }
    if gardenSurface.waterPatches != streamedGardenSurface.waterPatches { terrainDirtyCauses.append("shallow-water") }
    if boulderRevision != streamedBoulderRevision { terrainDirtyCauses.append("boulder-placement") }
    if force { terrainDirtyCauses.append("explicit-refresh") }
    let boulderDirty = boulderRevision == streamedBoulderRevision
      ? Set<SanctuaryTerrainChunkKey>()
      : streamedBoulderCoverage.union(boulderCoverage).intersection(keys)
    let dirty = entering.union(gardenDirty).union(boulderDirty).union(
      force ? gardenCoverage.union(boulderCoverage).intersection(keys)
        : Set<SanctuaryTerrainChunkKey>())
    let dirtyPlans = plans.filter { dirty.contains($0.key) }
    var removed = Set(leaving.union(dirty).flatMap { key in
      streamedBatchesByKey[key]?.map(\.name) ?? []
    })
    for key in leaving.union(dirty) { streamedBatchesByKey[key] = nil }
    var replacement = dirtyPlans.flatMap {
      BiomeWorldPresentation.chunkBatches(
        terrain: terrain, plan: $0, garden: garden, templates: biomeTemplates,
        boulders: boulders)
    }
    // The far and detail replacements travel through one upload transaction. No frame keeps
    // the old far surface beneath new detail, or exposes a hole after a streamed step.
    if force || keys != streamedKeys || surfaceChanged {
      rebuiltDistantTerrain = true
      removed.formUnion(["Distant world", "Distant water"])
      replacement.append(SceneBatch(name: "Distant world",
        mesh: distantTerrain.mesh(terrain: terrain, excluding: keys, garden: garden),
        instances: [Instance(kind: BiomeGroundAppearance.materialKind)]))
      let water = distantTerrain.waterMesh(terrain: terrain, excluding: keys)
      if !water.indices.isEmpty {
        replacement.append(SceneBatch(name: "Distant water", mesh: water,
          instances: [Instance(kind: 3)], roughness: 0.32, doubleSided: true, metallic: 0.04))
      }
    }
    let cabinKeys: Set<SanctuaryTerrainChunkKey> = [
      .init(x: -1, z: -1), .init(x: 0, z: -1), .init(x: -1, z: 0), .init(x: 0, z: 0),
    ]
    if gardenDirty.intersection(cabinKeys).isEmpty == false {
      rebuiltCabinTerrain = true
      removed.insert("Cabin terrain")
      replacement.append(
        SceneBatch(
          name: "Cabin terrain",
          mesh: BiomeWorldPresentation.cabinTerrainMesh(terrain: terrain, garden: garden),
          instances: [Instance(kind: 3)], roughness: 0.95))
    }
    for plan in dirtyPlans {
      streamedBatchesByKey[plan.key] = replacement.filter { $0.name.hasSuffix(" \(plan.key.id)") }
    }
    batches.removeAll { removed.contains($0.name) }
    batches += replacement
    streamedKeys = keys
    streamedBatchNames = Set(streamedBatchesByKey.values.flatMap { $0.map(\.name) })
    streamedGardenSurface = gardenSurface
    streamedBoulderCoverage = boulderCoverage
    streamedBoulderRevision = boulderRevision
    let collisionStart = Date.timeIntervalSinceReferenceDate
    layout.setStreamedCollision(plans: plans, garden: garden, boulders: boulders, affectedKeys: dirty)
    collisionPreparationMilliseconds = (Date.timeIntervalSinceReferenceDate - collisionStart) * 1_000
    regeneratedTerrainChunks = dirtyPlans.count
    return SanctuaryStreamUpdate(keys: keys, removedBatchNames: removed, batches: replacement)
  }

  /// Polling and scheduling only. A complete candidate remains separate from live collision
  /// and source batches until the experience finishes its bounded main-thread GPU uploads.
  mutating func updateStreaming(
    around position: V3, garden: HabitatGarden?, boulders: BoulderArrangements? = nil,
    construction: PersonalConstruction? = nil, force: Bool = false
  ) throws -> SanctuaryStreamUpdate? {
    guard [position.x, position.y, position.z].allSatisfy(\.isFinite),
      SanctuaryGeography.bounds.contains(SIMD2(position.x, position.z)) else {
      throw RuntimeError.message("World streaming requires a finite in-world position")
    }
    let key = SanctuaryStreamRequestKey(center: .init(containing: SIMD2(position.x, position.z)),
      garden: garden, boulders: boulders, construction: construction)
    if force || key != streamRequestKey {
      streamGeneration &+= 1
      if force { forcedStreamGeneration = streamGeneration }
      streamRequestKey = key
      streamWorker.supersede(with: streamGeneration)
    }
    if let completion = try streamWorker.poll(generation: streamGeneration) {
      if let update = completion.update { return update }
      self = completion.world
    }
    if streamWorker.isIdle && settledStreamGeneration != streamGeneration {
      let request = SanctuaryStreamRequest(generation: streamGeneration, position: position,
        garden: garden, boulders: boulders, construction: construction,
        force: forcedStreamGeneration == streamGeneration)
      streamWorker.enqueue(world: self, request: request)
    }
    return nil
  }

  mutating func commitStreaming(generation: UInt64) -> Bool {
    guard generation == streamGeneration,
      let completed = streamWorker.commit(generation: generation) else { return false }
    self = completed.world
    return true
  }

  mutating func cancelStreaming() {
    streamGeneration &+= 1
    streamRequestKey = nil
    streamWorker.supersede(with: streamGeneration)
  }

  var streamingDiagnostics: [String: Any] {
    var result = streamWorker.diagnostics
    result["generation"] = streamGeneration
    result["terrainResidentChunks"] = streamedKeys.count
    result["cabinGrassRecipe"] = BiomeVegetationDesign.grassRecipeIdentity
    result["regionalMeadowGrassRecipe"] = BiomeVegetationDesign.regionalMeadowGrassRecipeIdentity
    let regionalGrassCount = batches.filter { $0.name.hasPrefix("Regional groundcover ") }
      .reduce(0) { $0 + $1.grass.count }
    result["regionalGrassDescriptors"] = regionalGrassCount
    result["regionalGrassSourceBytes"] = regionalGrassCount * MemoryLayout<GrassBlade>.stride
    result["regionalGrassTriangles"] = regionalGrassCount * 3
    result["cabinGrassDescriptorsBefore"] = cabinGrassDescriptorsBefore
    result["cabinGrassDescriptors"] = cabinGrassDescriptors
    result["cabinGrassClumps"] = cabinGrassClumps
    result["cabinFlowerStems"] = cabinFlowerStems
    result["cabinGrassSourceBytes"] = cabinGrassDescriptors * MemoryLayout<GrassBlade>.stride
    result["cabinGrassGeneratedVertices"] = cabinGrassDescriptors * 5
    result["cabinGrassTriangles"] = cabinGrassDescriptors * 3
    result["cabinGrassVisibleDescriptors"] = cabinVisibleGrassIndices.count
    result["cabinFlowersVisible"] = cabinVisibleFlowerIndices.count
    result["cabinGroundCoverFilterMilliseconds"] = cabinGroundCoverFilterMilliseconds
    result["cabinGroundCoverBatchesRebuilt"] = cabinGroundCoverBatchesRebuilt
    result["cabinGroundCoverCanonicalSourceBytes"] = canonicalCabinGrass.count * MemoryLayout<GrassBlade>.stride
      + canonicalCabinFlowers.vertices.count * MemoryLayout<Vertex>.stride
      + canonicalCabinFlowers.indices.count * MemoryLayout<UInt32>.stride
    result["vegetationCachedChunks"] = vegetationResidency.cachedChunkCount
    result["vegetationRecordCacheChunks"] = vegetationRecords.count
    result["vegetationResidentChunks"] = vegetationResidency.snapshot.chunks.count
    result["vegetationMaximumCachedChunks"] = vegetationResidency.configuration.maximumCachedChunks
    result["vegetationMaximumResidentChunks"] = vegetationResidency.configuration.maximumResidentChunks
    result["vegetationResolvedRecords"] = vegetationResolvedCount
    result["vegetationExcludedRecords"] = vegetationExcludedCount
    result["vegetationComplete"] = vegetationResidency.snapshot.isComplete
    result["vegetationSnapshotRevision"] = displayedVegetationRevision
    result["settled"] = settledStreamGeneration == streamGeneration
    result["terrainChunksGeneratedLastPublication"] = regeneratedTerrainChunks
    result["terrainDirtyCauses"] = terrainDirtyCauses
    result["cabinTerrainRebuilt"] = rebuiltCabinTerrain
    result["distantTerrainRebuilt"] = rebuiltDistantTerrain
    result["collisionPreparationMilliseconds"] = collisionPreparationMilliseconds
    result["uploadPreparationMilliseconds"] = uploadPreparationMilliseconds
    result["preparedUploadBatchCount"] = preparedUploadBatchCount
    result["preparedUploadTemplateHits"] = preparedUploadTemplateHits
    result["preparedUploadMetadataBytes"] = preparedUploadMetadataBytes
    result["preparedVegetationTemplateBytes"] = [biomeTemplates.vegetationFull,
      biomeTemplates.vegetationMiddle, biomeTemplates.vegetationFar].reduce(0) { count, source in
        count + source.values.reduce(0) { total, batches in
          total + batches.reduce(0) { $0 + ($1.preparedUpload?.byteCount ?? 0) }
        }
      }
    result["preparedVegetationTemplateCount"] = [biomeTemplates.vegetationFull,
      biomeTemplates.vegetationMiddle, biomeTemplates.vegetationFar].reduce(0) { count, source in
        count + source.values.reduce(0) { $0 + $1.filter { $0.preparedUpload != nil }.count }
      }
    result["collisionSolidCount"] = layout.solids.count
    var residentSourceBytes: Int = 0
    for batch in batches {
      for mesh in [batch.mesh] + batch.lodMeshes {
        residentSourceBytes += mesh.vertices.count * MemoryLayout<Vertex>.stride
        residentSourceBytes += mesh.indices.count * MemoryLayout<UInt32>.stride
      }
      residentSourceBytes += batch.instances.count * MemoryLayout<Instance>.stride
      residentSourceBytes += batch.grass.count * MemoryLayout<GrassBlade>.stride
    }
    result["residentSourceBytes"] = residentSourceBytes
    return result
  }

  fileprivate mutating func prepareStreamWork(_ request: SanctuaryStreamRequest) throws -> SanctuaryStreamUpdate? {
    let residencyRequest = try vegetationResidency.beginRequest(
      around: SIMD2(request.position.x, request.position.z))
    // One canonical chunk per serial worker turn. Cache-only progress deliberately remains
    // unsettled, so polling schedules the rest of the tier rather than stranding eight jobs.
    if let job = vegetationResidency.nextJobs(for: residencyRequest.token, budget: .oneChunk).first {
      let compiled = try vegetationResidency.compile(job)
      vegetationRecords[job.key] = compiled.exact
      _ = vegetationResidency.publish(compiled)
    }
    let snapshot = vegetationResidency.snapshot
    let retained = Set(residencyRequest.desiredChunks.map(\.key)).union(snapshot.chunks.map(\.key))
    vegetationRecords = vegetationRecords.filter { retained.contains($0.key) }
    vegetationPlacements = vegetationPlacements.filter { retained.contains($0.key) }
    // A complete near tier is the minimum replacement. Keep the previous terrain and its
    // vegetation/collision visible while missing incoming chunks are being generated.
    guard snapshot.requestToken == residencyRequest.token else { return nil }

    let requestedSurface = SanctuaryGardenSurfaceSource(garden: request.garden)
    let resolvedSurface = SanctuaryGardenSurfaceSource(garden: resolvedGarden)
    let gardenChanged = requestedSurface != resolvedSurface
    let gardenPublicationChanged = publishedGarden != request.garden
    let constructionChanged = resolvedConstruction != request.construction
    if gardenChanged || constructionChanged {
      var affected = Set<SanctuaryTerrainChunkKey>()
      if gardenChanged {
        affected.formUnion(requestedSurface.affectedChunkKeys(comparedTo: resolvedSurface))
      }
      if constructionChanged {
        affected.formUnion(constructionChunks(resolvedConstruction))
        affected.formUnion(constructionChunks(request.construction))
      }
      for key in affected { vegetationPlacements[key] = nil }
    }
    collisionPreparationMilliseconds = 0
    let supportBranchChanged = publishedBoulders != request.boulders
      && publishedBoulders?.revision == request.boulders?.revision
    var update = try buildTerrainStreaming(around: request.position, garden: request.garden,
      boulders: request.boulders, force: request.force || supportBranchChanged)
    forcedStreamGeneration = nil
    let changed = snapshot.revision != displayedVegetationRevision || gardenChanged || constructionChanged
    if changed {
      let start = Date.timeIntervalSinceReferenceDate
      let mask = SanctuaryVegetationLocalEditMask(garden: request.garden, construction: request.construction)
      let facts = request.construction?.collisionFacts ?? []
      let trunk = treeSource.treeFields().0
      var byTier: [SanctuaryVegetationResidencyTier: [SanctuaryVegetationPlacement]] = [:]
      var collision: [SanctuaryTerrainChunkKey: [Solid]] = [:]
      vegetationExcludedCount = 0
      vegetationResolvedCount = 0
      for chunk in snapshot.chunks {
        let records = vegetationRecords[chunk.key] ?? []
        if vegetationPlacements[chunk.key] == nil {
          vegetationPlacements[chunk.key] = records.compactMap {
            SanctuaryVegetationPlacement.resolve($0, terrain: terrain, garden: request.garden,
              decision: mask.decision(for: $0), constructionFacts: facts, treeTrunk: trunk)
          }
        }
        let placements = vegetationPlacements[chunk.key] ?? []
        vegetationExcludedCount += records.count - placements.count
        vegetationResolvedCount += placements.count
        byTier[chunk.tier, default: []] += placements
        if chunk.tier == .detailed { collision[chunk.key] = placements.compactMap(\.solid) }
      }
      let replacement = BiomeWorldPresentation.vegetationBatches(placements: byTier, templates: biomeTemplates)
      let removed = vegetationBatchNames
      batches.removeAll { removed.contains($0.name) }
      batches += replacement
      vegetationBatchNames = Set(replacement.map(\.name))
      layout.setVegetationCollision(collision)
      collisionPreparationMilliseconds += (Date.timeIntervalSinceReferenceDate - start) * 1_000
      update = SanctuaryStreamUpdate(keys: streamedKeys,
        removedBatchNames: (update?.removedBatchNames ?? []).union(removed),
        batches: (update?.batches ?? []) + replacement)
      displayedVegetationRevision = snapshot.revision
      resolvedConstruction = request.construction
    }
    if let cover = rebuildCabinGroundCover(garden: request.garden, construction: request.construction) {
      update = SanctuaryStreamUpdate(keys: streamedKeys,
        removedBatchNames: (update?.removedBatchNames ?? []).union(cover.removedBatchNames),
        batches: (update?.batches ?? []) + cover.batches)
    }
    // Even a decoration-only edit is a host publication. An empty replacement commits the
    // exact captured garden to LivingWorldPresentation without uploading unchanged meshes.
    if update == nil && gardenPublicationChanged {
      update = SanctuaryStreamUpdate(keys: streamedKeys, removedBatchNames: [], batches: [])
    }
    resolvedGarden = request.garden
    publishedGarden = request.garden
    publishedBoulders = request.boulders
    publishedConstruction = request.construction
    if snapshot.isComplete { settledStreamGeneration = request.generation }
    update?.generation = request.generation
    uploadPreparationMilliseconds = 0
    preparedUploadBatchCount = 0
    preparedUploadTemplateHits = 0
    preparedUploadMetadataBytes = 0
    if var preparedUpdate = update {
      let preparationStart = Date.timeIntervalSinceReferenceDate
      for index in preparedUpdate.batches.indices {
        if preparedUpdate.batches[index].preparedUpload != nil { preparedUploadTemplateHits += 1 }
        preparedUpdate.batches[index].prepareUpload()
        if let prepared = preparedUpdate.batches[index].preparedUpload {
          preparedUploadBatchCount += 1
          preparedUploadMetadataBytes += prepared.byteCount
        }
      }
      update = preparedUpdate
      uploadPreparationMilliseconds = (Date.timeIntervalSinceReferenceDate - preparationStart) * 1_000
    }
    return update
  }

  /// Filters immutable cabin source only when saved support/occupancy facts change. The
  /// result joins the existing generation-checked upload transaction; no per-frame rebuild.
  private mutating func rebuildCabinGroundCover(
    garden: HabitatGarden?, construction: PersonalConstruction?
  ) -> SanctuaryStreamUpdate? {
    cabinGroundCoverFilterMilliseconds = 0
    cabinGroundCoverBatchesRebuilt = 0
    let mask = SanctuaryGroundCoverMask(garden: garden, construction: construction)
    guard mask != cabinGroundCoverMask else { return nil }
    let start = Date.timeIntervalSinceReferenceDate
    let hadMask = cabinGroundCoverMask != nil
    let grassIndices = canonicalCabinGrass.indices.filter { index in
      let blade = canonicalCabinGrass[index]
      let root = V3(blade.anchorAngle.x, blade.anchorAngle.y, blade.anchorAngle.z)
      return !mask.excludes(root: root, height: blade.size.x,
        radius: blade.size.x * 0.32 + blade.size.y + 0.15)
    }
    let flowerIndices = cabinFlowerSupports.indices.filter { index in
      let source = cabinFlowerSupports[index]
      return !mask.excludes(root: V3(source.x, source.y, source.z), height: source.w, radius: 0.4)
    }
    cabinGroundCoverMask = mask
    defer { cabinGroundCoverFilterMilliseconds = (Date.timeIntervalSinceReferenceDate - start) * 1_000 }
    let grassChanged = !hadMask || grassIndices != cabinVisibleGrassIndices
    let flowersChanged = !hadMask || flowerIndices != cabinVisibleFlowerIndices
    guard grassChanged || flowersChanged else { return nil }
    cabinVisibleGrassIndices = grassIndices
    cabinVisibleFlowerIndices = flowerIndices
    var replacement: [SceneBatch] = []
    if grassChanged && !grassIndices.isEmpty {
      let grass = grassIndices.count == canonicalCabinGrass.count
        ? canonicalCabinGrass : grassIndices.map { canonicalCabinGrass[$0] }
      replacement.append(SceneBatch(name: "Meadow", mesh: Mesh(), instances: [Instance(kind: 3)],
        grass: grass, doubleSided: true))
    }
    if flowersChanged && !flowerIndices.isEmpty {
      var flowers = canonicalCabinFlowers
      if flowerIndices.count != cabinFlowerSupports.count {
        flowers = Mesh()
        for index in flowerIndices {
          // Each original flower owns five independent triangles in canonical source order.
          for triangle in 0..<5 {
            let start = index * 15 + triangle * 3
            let indices = canonicalCabinFlowers.indices
            let vertices = canonicalCabinFlowers.vertices
            flowers.triangle(vertices[Int(indices[start])], vertices[Int(indices[start + 1])],
              vertices[Int(indices[start + 2])])
          }
        }
      }
      replacement.append(SceneBatch(name: "Wildflowers", mesh: flowers,
        instances: [Instance(kind: 3)], doubleSided: true))
    }
    var removed: Set<String> = []
    if grassChanged { removed.insert("Meadow") }
    if flowersChanged { removed.insert("Wildflowers") }
    batches.removeAll { removed.contains($0.name) }
    batches += replacement
    cabinGroundCoverBatchesRebuilt = replacement.count
    return SanctuaryStreamUpdate(keys: streamedKeys, removedBatchNames: removed, batches: replacement)
  }

  private func constructionChunks(_ construction: PersonalConstruction?) -> Set<SanctuaryTerrainChunkKey> {
    var result: Set<SanctuaryTerrainChunkKey> = []
    for placement in construction?.placements ?? [] {
      let extent = placement.primitive.halfExtents * placement.scale
      let radius = length(SIMD2(extent.x, extent.z)) + 32
      let lower = SanctuaryTerrainChunkKey(containing: SIMD2(placement.location.x - radius, placement.location.z - radius))
      let upper = SanctuaryTerrainChunkKey(containing: SIMD2(placement.location.x + radius, placement.location.z + radius))
      for z in lower.z...upper.z { for x in lower.x...upper.x {
        let key = SanctuaryTerrainChunkKey(x: x, z: z)
        if key.intersectsWorld { result.insert(key) }
      } }
    }
    return result
  }

  /// Chunks whose saved support differs from the currently visible publication.
  func pendingSupportKeys(garden: HabitatGarden?, boulders: BoulderArrangements?,
    construction: PersonalConstruction?) -> Set<SanctuaryTerrainChunkKey> {
    var result: Set<SanctuaryTerrainChunkKey> = []
    result.formUnion(SanctuaryGardenSurfaceSource(garden: garden).affectedChunkKeys(
      comparedTo: SanctuaryGardenSurfaceSource(garden: publishedGarden)))
    if boulders != publishedBoulders {
      result.formUnion(boulders?.sourceChunkKeys ?? [])
      result.formUnion(publishedBoulders?.sourceChunkKeys ?? [])
    }
    if construction != publishedConstruction {
      result.formUnion(constructionChunks(construction))
      result.formUnion(constructionChunks(publishedConstruction))
    }
    return result
  }

  /// Rebuilds visible terrain/water after a garden edit, even if the camera stayed in one chunk.
  mutating func rebuildTerrain(
    around position: V3, garden: HabitatGarden, boulders: BoulderArrangements? = nil
  ) throws -> SanctuaryStreamUpdate {
    try buildTerrainStreaming(around: position, garden: garden, boulders: boulders, force: true)!
  }

  static var podShape: Shape { SanctuaryLayout.podShape }
}

private typealias SanctuaryStreamRequestKey = SanctuaryRegionalSourceReceipt

fileprivate struct SanctuaryStreamRequest {
  let generation: UInt64
  let position: V3
  let garden: HabitatGarden?
  let boulders: BoulderArrangements?
  let construction: PersonalConstruction?
  let force: Bool
}

private struct SanctuaryStreamCompletion {
  let world: GardenWorld
  let update: SanctuaryStreamUpdate?
}

/// One worker for all world-detail CPU work. At most one task is running/queued and one
/// candidate awaits upload. Superseding replaces intent, never grows a FIFO of old travels.
private final class SanctuaryStreamWorker {
  private let queue = DispatchQueue(label: "sanctuary.world-stream", qos: .userInitiated)
  private let lock = NSLock()
  private var generation: UInt64 = 0
  private var running = false
  private var offered = false
  private var ready: Result<SanctuaryStreamCompletion, Error>?
  private var staleCount = 0
  private var completedCount = 0
  private var lastMilliseconds: Double = 0
  private var maximumMilliseconds: Double = 0

  var isIdle: Bool { lock.lock(); defer { lock.unlock() }; return !running && ready == nil }

  func supersede(with value: UInt64) {
    lock.lock(); defer { lock.unlock() }
    generation = value
    if ready != nil { staleCount += 1 }
    ready = nil; offered = false
  }

  func enqueue(world: GardenWorld, request: SanctuaryStreamRequest) {
    lock.lock()
    guard !running, ready == nil, generation == request.generation else { lock.unlock(); return }
    running = true
    lock.unlock()
    queue.async { [self] in
      let start = Date.timeIntervalSinceReferenceDate
      let result = Result<SanctuaryStreamCompletion, Error> {
        var candidate = world
        let update = try candidate.prepareStreamWork(request)
        return SanctuaryStreamCompletion(world: candidate, update: update)
      }
      let elapsed = (Date.timeIntervalSinceReferenceDate - start) * 1_000
      lock.lock(); defer { lock.unlock() }
      running = false
      lastMilliseconds = elapsed; maximumMilliseconds = max(maximumMilliseconds, elapsed)
      completedCount += 1
      guard generation == request.generation else { staleCount += 1; return }
      ready = result
      offered = false
    }
  }

  func poll(generation value: UInt64) throws -> SanctuaryStreamCompletion? {
    lock.lock(); defer { lock.unlock() }
    guard generation == value, !offered, let ready else { return nil }
    let completion: SanctuaryStreamCompletion
    do { completion = try ready.get() }
    catch { self.ready = nil; throw error }
    if completion.update == nil { self.ready = nil }
    else { offered = true }
    return completion
  }

  func commit(generation value: UInt64) -> SanctuaryStreamCompletion? {
    lock.lock(); defer { lock.unlock() }
    guard generation == value, offered, let completion = try? ready?.get() else { return nil }
    ready = nil; offered = false
    return completion
  }

  var diagnostics: [String: Any] {
    lock.lock(); defer { lock.unlock() }
    return ["maximumQueuedJobs": 1, "queuedOrRunningJobs": running ? 1 : 0,
      "pendingPublication": ready != nil, "staleJobs": staleCount,
      "completedJobs": completedCount, "lastGenerationMilliseconds": lastMilliseconds,
      "maximumGenerationMilliseconds": maximumMilliseconds]
  }
}
