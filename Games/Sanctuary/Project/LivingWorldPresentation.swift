import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import SimulationCore
import simd

/// Renderer-facing representations for the production ecology and player-authored world.
///
/// Every mesh is compiled and uploaded in `init`. Simulation updates only select existing
/// batches and change their instance transforms, so a wildlife decision or garden edit never
/// starts a field compilation on the frame thread.
final class LivingWorldPresentation {
  enum CreatureRole { case body, head, eyes, expressive, leg(Int), ear(Float) }

  private struct CreaturePart {
    var batch: GPUBatch
    var role: CreatureRole
    var pivot: V3
    var castsShadow: Bool
  }

  struct PartDesign {
    var name: String
    var shape: Shape
    var color: V3
    var material: Int
    var roughness: Float
    var metallic: Float = 0
    var role: CreatureRole = .body
    var pivot: V3 = .zero
    var castsShadow = true
    var cloudRaySurface = false
    var proceduralMesh: Mesh? = nil
    var parent: String? = nil
    var resolution: Int? = nil
  }

  struct ActorPlacement {
    var position: V3
    var yaw: Float
    var attached: Bool
  }

  private let frostlingSource: AssetSource
  private let frostlingBatches: [GPUBatch]
  private let creatureParts: [String: [CreaturePart]]
  private let creatureJoints: [WildlifeSpecies: [PartJoint]]
  private let treeBatches: [GPUBatch]
  private let reed: GPUBatch
  private let flower: GPUBatch
  private let shallowWater: GPUBatch
  private let treeEnvelope: GardenPlantEnvelope
  private let reedEnvelope: GardenPlantEnvelope
  private let flowerEnvelope: GardenPlantEnvelope
  private var gardenConstructionPlacements: [PersonalConstruction.Placement]?
  private var gardenConstructionMask = SanctuaryGroundCoverMask(garden: nil, construction: nil)
  private struct GardenMaskKey: Hashable {
    let rootHeight: SIMD4<Float>
    let radius: Float
  }
  private var gardenMaskDecisions: [GardenMaskKey: Bool] = [:]

  /// Measured once from the same compiled source used for rendering. Bound the complete
  /// growth pose (at most 7 degrees/1.065 scale) and existing kind-8 wind deformation.
  struct GardenPlantEnvelope {
    let bottom: Float
    let height: Float
    let radius: Float
    let windMargin: Float
    init(_ batches: [SceneBatch]) {
      var low: Float = 0, high: Float = 0, radial: Float = 0
      for batch in batches {
        for instance in batch.instances {
          for vertex in batch.mesh.vertices {
            let p = instance.model * vertex.position
            low = min(low, p.y); high = max(high, p.y)
            radial = max(radial, length(SIMD2(p.x, p.z)))
          }
        }
      }
      let tilt: Float = sin(7 * Float.pi / 180)
      let verticalReach = max(abs(low), abs(high)) * 1.065
      bottom = low * 1.065 - radial * tilt - 0.02
      height = high * 1.065 + radial * tilt + 0.02 - bottom
      let relativeHeight: Float = max(high, 0) / 5
      windMargin = 0.35 * Float(2).squareRoot() * relativeHeight * relativeHeight * 0.75
      radius = radial + verticalReach * tilt + 0.02
    }
  }
  private let natureMagic: NatureMagicPresentation
  private let constructionParts: [String: [CreaturePart]]
  private let ecologyParts: [String: [CreaturePart]]
  private let fallbackMotion = CreatureMotion()

  /// Soundstage registrations compile the same procedural source used below. Moonhart and
  /// Cloud Ray also share their articulated pose evaluators with production presentation.
  /// Social decisions remain owned by `WildlifeActor`.
  static var generators: [AssetGenerator] {
    var result = WildlifeSpecies.allCases.filter { $0 != .frostling }.map { species in
      AssetGenerator(
        id: species.rawValue, name: speciesTitle(species), controls: designControls(for: species),
        animation: inspectionAnimation(for: species),
        parts: { parameters in inspectionParts(for: species, parameters: parameters) },
        compile: { source in
          let parts: [PartDesign]
          if species == .sunhare {
            parts = try SanctuarySunhareDesign.validatedParts(source.parameters)
          } else {
            parts = design(for: species, parameters: source.parameters)
          }
          return try parts.map(sceneBatch)
        })
    }
    result += PersonalConstruction.Primitive.allCases.map { primitive in
      AssetGenerator(
        id: "construction-\(primitive.rawValue)",
        name: "Construction · \(primitive.rawValue.capitalized)",
        compile: { _ in try constructionBatches(for: primitive) })
    }
    result += SanctuaryPlacementPresentation.generators
    result += SanctuaryNaturePreviewPresentation.generators
    result += EcologyStructureKind.allCases.map { kind in
      AssetGenerator(
        id: "ecology-\(kind.rawValue)", name: "Ecology · \(structureTitle(kind))",
        compile: { _ in try design(for: kind).map(sceneBatch) })
    }
    result.append(AssetGenerator(id: "garden-flower", name: "Garden Flower",
      compile: { _ in [try sceneBatch(flowerDesign)] }))
    return result
  }

  init(graphics: MetalRenderer) throws {
    frostlingSource = try AssetSource.load("frostling")
    frostlingBatches = try frostlingSource.compile().map(graphics.upload)
    let treeSource = try AssetSource.load("tree")
    let compiledTree = try treeSource.compile()
    treeBatches = compiledTree.map(graphics.upload)
    treeEnvelope = GardenPlantEnvelope(compiledTree)

    func upload(_ design: PartDesign) throws -> CreaturePart {
      let batch = try Self.sceneBatch(design)
      return CreaturePart(
        batch: graphics.upload(batch), role: design.role, pivot: design.pivot,
        castsShadow: design.castsShadow)
    }

    var compiledCreatures: [String: [CreaturePart]] = [:]
    var joints: [WildlifeSpecies: [PartJoint]] = [:]
    for species in WildlifeSpecies.allCases where species != .frostling {
      if Self.hasAuthoredMotion(species) {
        let source = try AssetSource.load(species.rawValue)
        let designs = Dictionary(uniqueKeysWithValues:
          Self.design(for: species, parameters: source.parameters).map { ($0.name, $0) })
        compiledCreatures[species.rawValue] = try source.compile().map { batch in
          let design = designs[batch.name]
          return CreaturePart(batch: graphics.upload(batch), role: design?.role ?? .body,
            pivot: design?.pivot ?? .zero, castsShadow: design?.castsShadow ?? true)
        }
        joints[species] = source.joints
      } else {
        compiledCreatures[species.rawValue] = try Self.design(for: species).map { try upload($0) }
      }
    }
    creatureParts = compiledCreatures
    creatureJoints = joints

    let compiledReed = try Self.sceneBatch(Self.reedDesign)
    let compiledFlower = try Self.sceneBatch(Self.flowerDesign)
    reed = graphics.upload(compiledReed)
    flower = graphics.upload(compiledFlower)
    reedEnvelope = GardenPlantEnvelope([compiledReed])
    flowerEnvelope = GardenPlantEnvelope([compiledFlower])
    shallowWater = try upload(Self.waterDesign).batch
    natureMagic = NatureMagicPresentation(graphics: graphics)

    var compiledConstruction: [String: [CreaturePart]] = [:]
    for primitive in PersonalConstruction.Primitive.allCases {
      let batches = try Self.constructionBatches(for: primitive)
      compiledConstruction[primitive.rawValue] = zip(Self.design(for: primitive), batches).map { design, batch in
        CreaturePart(batch: graphics.upload(batch), role: design.role, pivot: design.pivot,
          castsShadow: design.castsShadow)
      }
    }
    constructionParts = compiledConstruction

    var compiledEcology: [String: [CreaturePart]] = [:]
    for kind in EcologyStructureKind.allCases {
      compiledEcology[kind.rawValue] = try Self.design(for: kind).map { try upload($0) }
    }
    ecologyParts = compiledEcology
  }

  func items(
    population: WildlifePopulation, garden: HabitatGarden,
    construction: PersonalConstruction, terrainHeight: (Float, Float) -> Float,
    waterHeight: (Float, Float) -> Float?,
    creatureElevation: (WildlifeActor) -> SanctuaryCreatureElevation,
    player: PlayerCamera, playSeconds: Double,
    journey: SanctuaryJourney = SanctuaryJourney(), includeHome: Bool = true
  ) -> [RenderItem] {
    var result: [RenderItem] = []
    let composedHeight: (Float, Float) -> Float = { x, z in
      garden.surfaceHeight(
        baseHeight: terrainHeight(x, z), at: HabitatGarden.Location(x: x, z: z))
    }
    appendWildlife(
      population, creatureElevation: creatureElevation, player: player, journey: journey, to: &result)
    appendEcology(
      population.ecology, terrainHeight: composedHeight, player: player, to: &result)
    appendGarden(garden, construction: construction, terrainHeight: terrainHeight, player: player,
      playSeconds: playSeconds, to: &result)
    result += natureMagic.items(garden: garden, playSeconds: playSeconds,
      player: player, waterHeight: waterHeight)
    if includeHome {
      appendConstruction(SanctuaryHome.construction, player: player, to: &result)
    }
    appendConstruction(construction, player: player, to: &result)
    return result
  }

  // MARK: - Wildlife

  private func appendWildlife(
    _ population: WildlifePopulation,
    creatureElevation: (WildlifeActor) -> SanctuaryCreatureElevation,
    player: PlayerCamera, journey: SanctuaryJourney, to result: inout [RenderItem]
  ) {
    let playerXZ = SIMD2(player.position.x, player.position.z)
    for actor in population.actors {
      let attached = journey.companionID == actor.id && journey.mode != .walking
      let p = attached ? playerXZ : actor.position
      let range: Float = actor.species == .cloudRay ? 1_200 : 520
      guard distance(p, playerXZ) <= range else { continue }
      if actor.species == .frostling {
        appendFrostling(
          actor, creatureElevation: creatureElevation, player: player, journey: journey, to: &result)
      } else {
        appendProceduralAnimal(
          actor, creatureElevation: creatureElevation, player: player, journey: journey, to: &result)
      }
    }
  }

  /// Frostlings keep the published source, joints, blink, attention and grounded hop cycle used
  /// by the original production encounter. Social semantics add only a root-level acting cue.
  private func appendFrostling(
    _ actor: WildlifeActor, creatureElevation: (WildlifeActor) -> SanctuaryCreatureElevation,
    player: PlayerCamera, journey: SanctuaryJourney, to result: inout [RenderItem]
  ) {
    let simulation = actor.simulation
    let motion = SanctuaryProject.motion(frostlingSource.motion ?? MotionParameters())
    let poses = PartRig.matrices(
      frostlingSource.joints,
      poses: motion.poses(
        time: simulation.time, phase: simulation.phase, alert: simulation.fear,
        gaze: simulation.gaze))
    let placement = Self.placement(
      for: actor, player: player, journey: journey, creatureElevation: creatureElevation)
    let hop = placement.attached
      ? 0
      : simulation.phase.map {
        CreatureMotion.flight($0) * motion.height * actor.morphologyScale
      } ?? 0
    let root =
      transform(
        placement.position + V3(0, hop, 0), V3(repeating: actor.morphologyScale),
        placement.yaw)
      * Self.semanticBodyPose(actor, time: simulation.time)
    for batch in frostlingBatches {
      var instance = batch.sourceInstances[0]
      instance.model = root * (poses[batch.name] ?? matrix_identity_float4x4) * instance.model
      result.append(RenderItem(batch: batch, instance: instance))
    }
  }

  private func appendProceduralAnimal(
    _ actor: WildlifeActor, creatureElevation: (WildlifeActor) -> SanctuaryCreatureElevation,
    player: PlayerCamera,
    journey: SanctuaryJourney, to result: inout [RenderItem]
  ) {
    guard let parts = creatureParts[actor.species.rawValue] else { return }
    let simulation = actor.simulation
    let placement = Self.placement(
      for: actor, player: player, journey: journey, creatureElevation: creatureElevation)
    let motionY = placement.attached
      ? 0
      : Self.motionYOffset(
        for: actor.species, time: simulation.time, phase: simulation.phase,
        morphologyScale: actor.morphologyScale)
    let root =
      transform(
        placement.position + V3(0, motionY, 0), V3(repeating: actor.morphologyScale),
        placement.yaw)
      * Self.semanticBodyPose(actor, time: simulation.time)
    if Self.hasAuthoredMotion(actor.species) {
      let poses: [String: JointPose]
      if placement.attached, journey.mode == .riding, actor.species == .moonhart {
        poses = SanctuaryMountMotion.mountedPoses(signal: actor.mountedTravel,
          actorYaw: placement.yaw, morphologyScale: actor.morphologyScale,
          time: simulation.time, gaze: simulation.gaze, mood: simulation.state)
      } else {
        poses = Self.sharedPoses(for: actor.species, simulation: simulation,
          identifier: actor.id, player: SIMD2(player.position.x, player.position.z),
          working: Self.isWorkingHabitat(actor), time: simulation.time,
          phase: placement.attached ? nil : simulation.phase)
      }
      let matrices = PartRig.matrices(creatureJoints[actor.species] ?? [], poses: poses)
      // Quadruped feet stay planted during idle; expression belongs in its head and ears.
      let authoredRoot = actor.species == .sunhare ? root : transform(
        placement.position + V3(0, motionY, 0), V3(repeating: actor.morphologyScale), placement.yaw)
        * Self.authoredRootPose(for: actor.species, time: simulation.time)
      for part in parts {
        var instance = part.batch.sourceInstances[0]
        instance.model = authoredRoot
          * (part.batch.skinJoints.isEmpty
            ? matrices[part.batch.name] ?? matrix_identity_float4x4 : matrix_identity_float4x4)
          * instance.model
        var item = RenderItem(batch: part.batch, instance: instance, castsShadow: part.castsShadow)
        item.skinPalette = part.batch.skinJoints.map { matrices[$0] ?? matrix_identity_float4x4 }
        result.append(item)
      }
      return
    }
    let head = Self.semanticHeadPose(actor, player: player)
    let blink = fallbackMotion.blink(at: simulation.time + Self.identifierPhase(actor.id))
    let expressive = Self.semanticExpressivePose(actor, time: simulation.time)

    for part in parts {
      var instance = part.batch.sourceInstances[0]
      let rolePose = Self.legacyPartPose(role: part.role, pivot: part.pivot,
        center: part.batch.center, head: head, blink: blink, expressive: expressive)
      instance.model = root * rolePose * instance.model
      result.append(
        RenderItem(batch: part.batch, instance: instance, castsShadow: part.castsShadow))
    }
  }

  /// Shared by production and CPU attachment checks; no separate fixture animation.
  static func legacyPartPose(role: CreatureRole, pivot: V3, center: V3,
    head: simd_float4x4, blink: Float, expressive: simd_float4x4) -> simd_float4x4
  {
    switch role {
    case .body, .leg, .ear: return matrix_identity_float4x4
    case .head: return around(pivot, rotation: head)
    case .eyes:
      return around(pivot, rotation: head)
        * around(center, scale: V3(1, max(0.06, 1 - blink * 0.94), 1))
    case .expressive: return around(pivot, rotation: expressive)
    }
  }

  static func semanticBodyPose(_ actor: WildlifeActor, time: Float) -> simd_float4x4 {
    Self.semanticBodyPose(actor.simulation, identifier: actor.id,
      time: time, working: Self.isWorkingHabitat(actor), gentleRefusal: actor.species == .sunhare)
  }

  private static func semanticBodyPose(_ simulation: CreatureSimulation, identifier: String,
    time: Float, working: Bool, gentleRefusal: Bool = false) -> simd_float4x4
  {
    let state = simulation.state
    let breath = 1 + sin(time * 2.1 + identifierPhase(identifier)) * 0.012
    var pitch: Float = 0
    var roll: Float = 0
    var offset = V3.zero
    if state == "declining" || !simulation.requestAccepted {
      // Sunhare communicates with its head/ear silhouette. Keep its planted
      // body at the support root instead of translating or tipping the feet.
      if !gentleRefusal {
        pitch = 7 * .pi / 180
        offset.z = 0.09
        offset.y = -0.025
      }
    } else if state == "playing" {
      roll = sin(time * 8) * 7 * .pi / 180
      offset.y = abs(sin(time * 8)) * 0.07
    } else if state == "fleeing" || simulation.fear > 0.35 {
      pitch = -9 * .pi / 180
      offset.y = -0.045
    } else if working {
      pitch = sin(time * 3.4) * 10 * .pi / 180
    } else if state == "greeting" {
      roll = sin(time * 3) * 3 * .pi / 180
    }
    if gentleRefusal {
      // Sole-bearing limbs are root children. Torso breathing/crouch belongs to
      // its own joint; the caller supplies only production travel, yaw and hop.
      return matrix_identity_float4x4
    }
    let rotation = simd_float4x4(
      simd_quatf(angle: roll, axis: V3(0, 0, 1))
        * simd_quatf(angle: pitch, axis: V3(1, 0, 0)))
    return Self.translation(offset) * Self.around(V3(0, 0.28, 0), rotation: rotation)
      * Self.scale(V3(1, breath, 1))
  }

  static func semanticHeadPose(_ actor: WildlifeActor, player: PlayerCamera) -> simd_float4x4 {
    let angles = Self.semanticHeadAngles(actor.simulation, identifier: actor.id,
      player: SIMD2(player.position.x, player.position.z), time: actor.simulation.time,
      working: Self.isWorkingHabitat(actor))
    return simd_float4x4(
      simd_quatf(angle: angles.y * .pi / 180, axis: V3(0, 1, 0))
        * simd_quatf(angle: angles.x * .pi / 180, axis: V3(1, 0, 0)))
  }

  private static func semanticHeadAngles(_ simulation: CreatureSimulation, identifier: String,
    player: SIMD2<Float>, time: Float, working: Bool, gentleRefusal: Bool = false) -> V3
  {
    var gaze = simulation.gaze
    if simulation.state == "watching", abs(gaze) < 0.5 {
      let delta = player - simulation.position
      if length_squared(delta) > 0.0001 {
        let desired = atan2(-delta.x, -delta.y) - simulation.yaw
        gaze = clamp(atan2(sin(desired), cos(desired)) * 180 / .pi, -30, 30)
      }
    }
    if !gentleRefusal && (simulation.state == "declining" || !simulation.requestAccepted) {
      gaze = gaze >= 0 ? -28 : 28
    }
    let nod: Float
    if working {
      nod = 13 + sin(time * 3.4) * 9
    } else {
      switch simulation.state {
      case "greeting": nod = -7
      case "playing": nod = sin(time * 7) * 9
      case "fleeing": nod = -8
      default: nod = sin(time * 0.8 + identifierPhase(identifier)) * 2.5
      }
    }
    let resting = V3(nod, gaze, 0)
    guard gentleRefusal, let elapsed = SanctuaryWildlifePerformance.refusalElapsed(simulation) else {
      return resting
    }
    // A saved refusal starts a short acknowledgement, two gentle shakes and a
    // settle. The separate ears inherit this head turn and soften from the
    // same saved event envelope. Head tilt exposes the gesture from behind.
    let envelope = CreatureMotion.smooth(elapsed / 0.28)
      * (1 - CreatureMotion.smooth((elapsed - 2.1) / 0.75))
    let shakeEnvelope = CreatureMotion.smooth((elapsed - 0.35) / 0.15)
      * (1 - CreatureMotion.smooth((elapsed - 1.65) / 0.3))
    let shake = sin((elapsed - 0.35) * (3 * .pi)) * shakeEnvelope
    let speaker = simulation.movementAnchor ?? player
    let delta = speaker - simulation.position
    let desired = atan2(-delta.x, -delta.y) - simulation.yaw
    let acknowledgment = clamp(atan2(sin(desired), cos(desired)) * 180 / .pi, -60, 60)
    let gesture = V3(16, acknowledgment + shake * 18, 10 + shake * 5)
    return resting + (gesture - resting) * envelope
  }

  static func semanticExpressivePose(_ actor: WildlifeActor, time: Float) -> simd_float4x4 {
    Self.semanticExpressivePose(species: actor.species, state: actor.simulation.state,
      identifier: actor.id, time: time)
  }

  private static func expressiveAmplitude(_ state: String) -> Float {
    switch state {
    case "playing": return 18
    case "greeting", "following": return 9
    case "declining", "fleeing": return -6
    default: return 3
    }
  }

  private static func semanticExpressivePose(species: WildlifeSpecies, state: String,
    identifier: String, time: Float) -> simd_float4x4
  {
    let amplitude: Float
    amplitude = expressiveAmplitude(state)
    let axis: V3 = [.canopyGlider, .cloudRay].contains(species)
      ? V3(0, 0, 1) : V3(0, 1, 0)
    return simd_float4x4(
      simd_quatf(
        angle: sin(time * 5 + identifierPhase(identifier)) * amplitude * .pi / 180,
        axis: axis))
  }

  // MARK: - Persistent ecology

  private func appendEcology(
    _ ecology: EcologyStructures, terrainHeight: (Float, Float) -> Float,
    player: PlayerCamera, to result: inout [RenderItem]
  ) {
    let playerXZ = SIMD2(player.position.x, player.position.z)
    for structure in ecology.structures {
      guard distance(structure.position, playerXZ) <= structure.radius + 320,
        let parts = ecologyParts[structure.kind.rawValue]
      else { continue }
      let completionScale: Float
      switch structure.state {
      case .building: completionScale = 0.25 + structure.progress * 0.75
      case .rebuilding: completionScale = 0.42 + structure.progress * 0.58
      case .active, .displaced: completionScale = 1
      }
      let displaced = structure.state == .displaced
      let p = structure.position
      let horizontalScale = structure.radius / Self.sourceRadius(for: structure.kind)
      let root = transform(
        V3(p.x, terrainHeight(p.x, p.y) + (displaced ? -0.04 : 0), p.y),
        V3(horizontalScale * completionScale, completionScale, horizontalScale * completionScale),
        structure.yaw)
        * simd_float4x4(
          simd_quatf(
            angle: displaced ? 13 * .pi / 180 : 0,
            axis: V3(0, 0, 1)))
      for part in parts {
        var instance = part.batch.sourceInstances[0]
        instance.model = root * instance.model
        if displaced {
          instance.tint.x *= 0.72
          instance.tint.y *= 0.72
          instance.tint.z *= 0.72
        }
        result.append(
          RenderItem(batch: part.batch, instance: instance, castsShadow: part.castsShadow))
      }
    }
    for water in ecology.waterFacts
    where distance(water.center, playerXZ) <= water.upstreamPoolRadius + 320 {
      let p = water.center
      var instance = shallowWater.sourceInstances[0]
      instance.model = transform(
        V3(p.x, terrainHeight(p.x, p.y) + water.waterLevelRise + 0.012, p.y),
        V3(water.upstreamPoolRadius, 1, water.upstreamPoolRadius), 0) * instance.model
      result.append(RenderItem(batch: shallowWater, instance: instance, castsShadow: false))
    }
  }

  // MARK: - Garden

  private func appendGarden(
    _ garden: HabitatGarden, construction: PersonalConstruction,
    terrainHeight: (Float, Float) -> Float, player: PlayerCamera,
    playSeconds: Double, to result: inout [RenderItem]
  ) {
    let playerXZ = SIMD2(player.position.x, player.position.z)
    func surfaceHeight(_ x: Float, _ z: Float) -> Float {
      garden.surfaceHeight(
        baseHeight: terrainHeight(x, z), at: HabitatGarden.Location(x: x, z: z))
    }
    // This is the same published construction passed to appendConstruction, never a
    // preview or live edit waiting on publication. Compare actual placements, not revisions.
    if gardenConstructionPlacements != construction.placements {
      gardenConstructionPlacements = construction.placements
      gardenConstructionMask = SanctuaryGroundCoverMask(garden: nil, construction: construction)
      gardenMaskDecisions.removeAll(keepingCapacity: true)
    }
    var currentDecisions: [GardenMaskKey: Bool] = [:]
    func obstructed(_ root: V3, _ scale: Float, _ envelope: GardenPlantEnvelope) -> Bool {
      guard !gardenConstructionMask.isEmpty else { return false }
      let bottom = root + V3(0, envelope.bottom * scale, 0)
      let key = GardenMaskKey(rootHeight: SIMD4(bottom, envelope.height * scale),
        radius: envelope.radius * scale + envelope.windMargin)
      let hidden = gardenMaskDecisions[key] ?? gardenConstructionMask.excludes(
        root: bottom, height: key.rootHeight.w, radius: key.radius)
      currentDecisions[key] = hidden
      return hidden
    }
    defer { gardenMaskDecisions = currentDecisions }
    let visiblePatches = garden.patches
      .filter { distance(SIMD2($0.center.x, $0.center.z), playerXZ) <= $0.radius + 180 }
      .sorted {
        distance_squared(SIMD2($0.center.x, $0.center.z), playerXZ)
          < distance_squared(SIMD2($1.center.x, $1.center.z), playerXZ)
      }
    // Bound nearby decorative instances. The authored patch and its ecological query remain
    // complete even when distant decoration is outside this presentation window.
    var decorativeBudget = 560
    for patch in visiblePatches {
      guard decorativeBudget > 0 else { break }
      var rng = SeededRandom(seed: patch.id &+ 0x9E37_79B9)
      let event = NatureMagicPresentation.event(for: patch, playSeconds: playSeconds)
      switch patch.planting {
      case .shallowWater:
        // Streamed water triangles use the production resolved shoreline and local height.
        // Ecology's separate authored dam pool retains its existing presentation.
        continue
      case .grove:
        let count = min(decorativeBudget, min(9, max(3, Int(patch.radius * 0.75))))
        for index in 0..<count {
          let sample = Self.sample(in: patch, rng: &rng, innerFraction: 0.18)
          let scale = rng.range(0.32, 0.72)
          let yaw = rng.range(-.pi, .pi)
          let root = V3(sample.x, surfaceHeight(sample.x, sample.y), sample.y)
          guard !obstructed(root, scale, treeEnvelope) else { continue }
          let placement = transform(root, V3(repeating: scale), yaw)
            * NatureMagicPresentation.growthTransform(event: event, elementIndex: index)
          for batch in treeBatches {
            var instance = batch.sourceInstances[0]
            instance.model = placement * instance.model
            result.append(RenderItem(batch: batch, instance: instance))
          }
        }
        decorativeBudget -= count
      case .reeds:
        let count = min(decorativeBudget, min(28, max(6, Int(patch.radius * patch.radius * 0.9))))
        for index in 0..<count {
          let sample = Self.sample(in: patch, rng: &rng)
          let scale = rng.range(0.65, 1.3)
          let yaw = rng.range(-.pi, .pi)
          let root = V3(sample.x, surfaceHeight(sample.x, sample.y), sample.y)
          guard !obstructed(root, scale, reedEnvelope) else { continue }
          var instance = reed.sourceInstances[0]
          instance.model = transform(root, V3(repeating: scale), yaw)
            * NatureMagicPresentation.growthTransform(event: event, elementIndex: index) * instance.model
          result.append(RenderItem(batch: reed, instance: instance))
        }
        decorativeBudget -= count
      case .flowers:
        let count = min(decorativeBudget, min(36, max(7, Int(patch.radius * patch.radius * 1.5))))
        for index in 0..<count {
          let sample = Self.sample(in: patch, rng: &rng)
          let scale = rng.range(0.7, 1.35)
          let yaw = rng.range(-.pi, .pi)
          let root = V3(sample.x, surfaceHeight(sample.x, sample.y), sample.y)
          guard !obstructed(root, scale, flowerEnvelope) else { continue }
          var instance = flower.sourceInstances[0]
          instance.model = transform(root, V3(repeating: scale), yaw)
            * NatureMagicPresentation.growthTransform(event: event, elementIndex: index) * instance.model
          result.append(RenderItem(batch: flower, instance: instance))
        }
        decorativeBudget -= count
      }
    }
    // Terrain sculpting has no duplicate overlay mesh here. `surfaceHeight` uses the same
    // garden query as the streamed terrain rebuild, collision, wildlife and every prop.
  }

  // MARK: - Construction

  /// Production, Soundstage and placement contours consume the same compiled source.
  /// The preview derives its sparse representation once; it never recompiles a draft.
  private static let constructionSourceCache: Result<[PersonalConstruction.Primitive: [SceneBatch]], Error> = Result {
    try Dictionary(uniqueKeysWithValues: PersonalConstruction.Primitive.allCases.map { primitive in
      (primitive, try design(for: primitive).map(sceneBatch))
    })
  }

  static func constructionBatches(for primitive: PersonalConstruction.Primitive) throws -> [SceneBatch] {
    try constructionSourceCache.get()[primitive] ?? []
  }

  private func appendConstruction(
    _ construction: PersonalConstruction, player: PlayerCamera,
    to result: inout [RenderItem]
  ) {
    let playerXZ = SIMD2(player.position.x, player.position.z)
    for placement in construction.placements {
      let p = SIMD2(placement.location.x, placement.location.z)
      guard distance(p, playerXZ) <= 360,
        let parts = constructionParts[placement.primitive.rawValue]
      else { continue }
      let root = transform(
        V3(placement.location.x, placement.location.y, placement.location.z),
        V3(repeating: placement.scale), placement.yawRadians)
      for part in parts {
        var instance = part.batch.sourceInstances[0]
        instance.model = root * instance.model
        result.append(
          RenderItem(batch: part.batch, instance: instance, castsShadow: part.castsShadow))
      }
    }
  }

  // MARK: - Source recipes

  /// Recipes emphasize silhouette and broad color masses. Each is registered for real-renderer
  /// inspection; species outside the reviewed repair remain provisional silhouette studies.
  private static func designControls(for species: WildlifeSpecies) -> [ScalarControl] {
    switch species {
    case .sunhare: return SanctuarySunhareDesign.controls
    case .canopyGlider: return SanctuaryCanopyGliderDesign.controls
    case .moonhart, .cloudRay: return SanctuaryMountDesign.controls(for: species)
    default: return []
    }
  }

  static func design(for species: WildlifeSpecies,
    parameters: [String: Float] = [:]) -> [PartDesign] {
    func part(
      _ suffix: String, _ shape: Shape, _ color: V3, material: Int = 9,
      roughness: Float = 0.84, role: CreatureRole = .body, pivot: V3 = .zero,
      shadow: Bool = true
    ) -> PartDesign {
      PartDesign(
        name: "Wildlife \(species.rawValue) \(suffix)", shape: shape, color: color,
        material: material, roughness: roughness, role: role, pivot: pivot,
        castsShadow: shadow)
    }
    func eyes(_ y: Float, _ z: Float, spacing: Float, radius: Float, pivot: V3) -> PartDesign {
      let shape = ellipsoid(V3(-spacing, y, z), V3(radius, radius * 1.2, radius * 0.7))
        .joined(ellipsoid(V3(spacing, y, z), V3(radius, radius * 1.2, radius * 0.7)))
      return part(
        "eyes", shape, V3(0.025, 0.035, 0.038), material: 7, roughness: 0.12,
        role: .eyes, pivot: pivot)
    }

    switch species {
    case .frostling:
      return []
    case .sunhare:
      return SanctuarySunhareDesign.parts(parameters)
    case .brookweaver:
      let headPivot = V3(0, 0.42, -0.40)
      return [
        part(
          "body",
          ellipsoid(V3(0, 0.31, 0.08), V3(0.34, 0.27, 0.60))
            .joined(ellipsoid(V3(-0.23, 0.11, -0.28), V3(0.13, 0.08, 0.20)))
            .joined(ellipsoid(V3(0.23, 0.11, -0.28), V3(0.13, 0.08, 0.20))),
          V3(0.37, 0.29, 0.20)),
        part(
          "head", ellipsoid(V3(0, 0.43, -0.49), V3(0.27, 0.22, 0.28)),
          V3(0.48, 0.36, 0.23), role: .head, pivot: headPivot),
        part(
          "paddle tail", ellipsoid(V3(0, 0.20, 0.82), V3(0.23, 0.07, 0.42)),
          V3(0.25, 0.20, 0.16), material: 7, roughness: 0.78, role: .expressive,
          pivot: V3(0, 0.23, 0.49)),
        eyes(0.48, -0.73, spacing: 0.15, radius: 0.027, pivot: headPivot),
      ]
    case .reedwalker:
      let headPivot = V3(0, 1.18, -0.12)
      let body = ellipsoid(V3(0, 0.91, 0.10), V3(0.29, 0.37, 0.43))
        .joined(.capsule(V3(-0.13, 0.06, 0.08), V3(-0.11, 0.72, 0.10), 0.038))
        .joined(.capsule(V3(0.13, 0.06, 0.08), V3(0.11, 0.72, 0.10), 0.038))
        .joined(.capsule(V3(0, 0.99, -0.05), V3(0, 1.38, -0.14), 0.095))
      let head = ellipsoid(V3(0, 1.43, -0.17), V3(0.17, 0.18, 0.18))
        .joined(.capsule(V3(0, 1.43, -0.30), V3(0, 1.38, -0.76), 0.055))
      return [
        part("body and stilt legs", body, V3(0.42, 0.57, 0.42), material: 7, roughness: 0.9),
        part(
          "crested head", head, V3(0.65, 0.72, 0.47), material: 7, roughness: 0.82,
          role: .head, pivot: headPivot),
        part(
          "wing fan", ellipsoid(V3(0, 0.96, 0.29), V3(0.42, 0.08, 0.32)),
          V3(0.28, 0.45, 0.39), material: 7, roughness: 0.9, role: .expressive,
          pivot: V3(0, 0.96, 0.05)),
        eyes(1.48, -0.33, spacing: 0.105, radius: 0.024, pivot: headPivot),
      ]
    case .moonhart:
      return SanctuaryMountDesign.parts(for: species, parameters: parameters)
    case .cloudstepper:
      let headPivot = V3(0, 1.10, -0.39)
      let body = ellipsoid(V3(0, 0.84, 0.08), V3(0.45, 0.48, 0.66))
        .joined(.capsule(V3(-0.29, 0.08, -0.34), V3(-0.27, 0.68, -0.32), 0.09))
        .joined(.capsule(V3(0.29, 0.08, -0.34), V3(0.27, 0.68, -0.32), 0.09))
        .joined(.capsule(V3(-0.29, 0.08, 0.39), V3(-0.27, 0.67, 0.38), 0.09))
        .joined(.capsule(V3(0.29, 0.08, 0.39), V3(0.27, 0.67, 0.38), 0.09))
      let horns = Shape.torus(0.22, 0.045).moved(V3(-0.14, 1.40, -0.48))
        .joined(.torus(0.22, 0.045).moved(V3(0.14, 1.40, -0.48)))
      return [
        part("wool body", body, V3(0.77, 0.76, 0.67)),
        part(
          "head", ellipsoid(V3(0, 1.23, -0.56), V3(0.28, 0.30, 0.32)),
          V3(0.46, 0.43, 0.37), role: .head, pivot: headPivot),
        part(
          "ring horns", horns, V3(0.35, 0.32, 0.27), material: 7, roughness: 0.7,
          role: .head, pivot: headPivot),
        eyes(1.28, -0.84, spacing: 0.16, radius: 0.031, pivot: headPivot),
      ]
    case .dunefox:
      let headPivot = V3(0, 0.55, -0.36)
      return [
        part(
          "body",
          ellipsoid(V3(0, 0.40, 0.12), V3(0.28, 0.29, 0.52))
            .joined(ellipsoid(V3(-0.19, 0.10, -0.26), V3(0.10, 0.10, 0.18)))
            .joined(ellipsoid(V3(0.19, 0.10, -0.26), V3(0.10, 0.10, 0.18))),
          V3(0.80, 0.60, 0.35)),
        part(
          "wide-eared head",
          ellipsoid(V3(0, 0.67, -0.45), V3(0.23, 0.24, 0.27))
            .joined(ellipsoid(V3(-0.20, 0.94, -0.39), V3(0.14, 0.30, 0.055)))
            .joined(ellipsoid(V3(0.20, 0.94, -0.39), V3(0.14, 0.30, 0.055))),
          V3(0.91, 0.69, 0.41), role: .head, pivot: headPivot),
        part(
          "brush tail",
          .capsule(V3(0, 0.41, 0.48), V3(0.22, 0.33, 1.08), 0.16),
          V3(0.90, 0.72, 0.47), role: .expressive, pivot: V3(0, 0.41, 0.45)),
        eyes(0.72, -0.69, spacing: 0.13, radius: 0.030, pivot: headPivot),
      ]
    case .canopyGlider:
      return SanctuaryCanopyGliderDesign.parts(parameters)
    case .saltback:
      let headPivot = V3(0, 0.35, -0.53)
      return [
        part(
          "feet",
          ellipsoid(V3(-0.28, 0.10, -0.29), V3(0.16, 0.09, 0.19))
            .joined(ellipsoid(V3(0.28, 0.10, -0.29), V3(0.16, 0.09, 0.19)))
            .joined(ellipsoid(V3(-0.28, 0.10, 0.30), V3(0.16, 0.09, 0.19)))
            .joined(ellipsoid(V3(0.28, 0.10, 0.30), V3(0.16, 0.09, 0.19))),
          V3(0.35, 0.37, 0.32), material: 7, roughness: 0.92),
        part(
          "salt shell", ellipsoid(V3(0, 0.35, 0.05), V3(0.52, 0.31, 0.68)),
          V3(0.57, 0.60, 0.57), material: 5, roughness: 0.72),
        part(
          "head", ellipsoid(V3(0, 0.36, -0.66), V3(0.22, 0.18, 0.27)),
          V3(0.43, 0.46, 0.41), material: 7, roughness: 0.9, role: .head,
          pivot: headPivot),
        eyes(0.41, -0.88, spacing: 0.12, radius: 0.027, pivot: headPivot),
      ]
    case .tidepooler:
      let headPivot = V3(0, 0.22, -0.12)
      let legs = (-1...1).reduce(Shape.sphere(0.001)) { shape, index in
        let z = Float(index) * 0.20
        return shape
          .joined(.capsule(V3(-0.17, 0.17, z), V3(-0.52, 0.05, z * 1.3), 0.035))
          .joined(.capsule(V3(0.17, 0.17, z), V3(0.52, 0.05, z * 1.3), 0.035))
      }
      return [
        part(
          "body and legs", ellipsoid(V3(0, 0.22, 0), V3(0.34, 0.18, 0.28)).joined(legs),
          V3(0.67, 0.29, 0.20), material: 7, roughness: 0.74),
        part(
          "raised claws",
          .capsule(V3(-0.24, 0.23, -0.18), V3(-0.48, 0.39, -0.34), 0.075)
            .joined(.capsule(V3(0.24, 0.23, -0.18), V3(0.48, 0.39, -0.34), 0.075))
            .joined(ellipsoid(V3(-0.53, 0.43, -0.39), V3(0.15, 0.12, 0.14)))
            .joined(ellipsoid(V3(0.53, 0.43, -0.39), V3(0.15, 0.12, 0.14))),
          V3(0.85, 0.40, 0.24), material: 7, roughness: 0.68, role: .expressive,
          pivot: headPivot),
        eyes(0.39, -0.22, spacing: 0.16, radius: 0.032, pivot: headPivot),
      ]
    case .cloudRay:
      return SanctuaryMountDesign.parts(for: species, parameters: parameters)
    }
  }

  private static func design(for primitive: PersonalConstruction.Primitive) -> [PartDesign] {
    func part(
      _ suffix: String, _ shape: Shape, _ color: V3, material: Int = 7,
      roughness: Float = 0.78, shadow: Bool = true
    ) -> PartDesign {
      PartDesign(
        name: "Construction \(primitive.rawValue) \(suffix)", shape: shape, color: color,
        material: material, roughness: roughness, castsShadow: shadow)
    }
    let timber = V3(0.38, 0.25, 0.15)
    let paleWood = V3(0.57, 0.40, 0.23)
    let stone = V3(0.46, 0.49, 0.48)
    switch primitive {
    case .cabin:
      return cabinDesign
    case .path:
      return [part("stone", .box(V3(1.38, 0.055, 0.54)).moved(V3(0, 0.055, 0)), stone, material: 5, roughness: 0.94)]
    case .bridge:
      let profile = SanctuaryWalkableConstructionProfile.bridge
      let deck = Shape.box(profile.platform.halfExtents).moved(profile.platform.center)
      let first = profile.rails[0]
      let rail = profile.rails.dropFirst().reduce(Shape.capsule(first.start, first.end, first.radius)) {
        $0.joined(.capsule($1.start, $1.end, $1.radius))
      }
      return [
        part("deck", deck, paleWood, material: 7, roughness: 0.9),
        part("rails", rail, timber, material: 7, roughness: 0.86),
      ]
    case .deck:
      let profile = SanctuaryWalkableConstructionProfile.deck
      let deck = profile.rails.reduce(Shape.box(profile.platform.halfExtents).moved(profile.platform.center)) {
        $0.joined(.capsule($1.start, $1.end, $1.radius))
      }
      return [part("platform", deck, paleWood, material: 7, roughness: 0.9)]
    case .bench:
      let shape = Shape.box(V3(0.88, 0.075, 0.32)).moved(V3(0, 0.54, 0))
        .joined(.box(V3(0.88, 0.28, 0.06)).moved(V3(0, 0.78, 0.27)))
        .joined(.capsule(V3(-0.68, 0.04, 0), V3(-0.68, 0.51, 0), 0.055))
        .joined(.capsule(V3(0.68, 0.04, 0), V3(0.68, 0.51, 0), 0.055))
      return [part("seat", shape, paleWood, material: 7, roughness: 0.88)]
    case .lantern:
      let post = Shape.capsule(V3(0, 0, 0), V3(0, 1.82, 0), 0.055)
        .joined(.capsule(V3(0, 1.78, 0), V3(0.18, 1.78, 0), 0.035))
      return [
        part("post", post, timber, material: 4, roughness: 0.82),
        part(
          "lamp", ellipsoid(V3(0.18, 1.57, 0), V3(0.14, 0.20, 0.14)),
          V3(0.96, 0.63, 0.22), roughness: 0.18, shadow: false),
      ]
    case .fence:
      let shape = Shape.capsule(V3(-1.18, 0, 0), V3(-1.18, 1.48, 0), 0.07)
        .joined(.capsule(V3(1.18, 0, 0), V3(1.18, 1.48, 0), 0.07))
        .joined(.capsule(V3(-1.18, 0.55, 0), V3(1.18, 0.55, 0), 0.055))
        .joined(.capsule(V3(-1.18, 1.18, 0), V3(1.18, 1.18, 0), 0.055))
      return [part("rails", shape, paleWood, material: 4, roughness: 0.9)]
    }
  }

  private static let cabinDesign: [PartDesign] = {
    let wood = V3(0.49, 0.39, 0.28)
    let trim = V3(0.31, 0.25, 0.20)
    let floorColor = V3(0.45, 0.35, 0.25)
    var walls: [Mesh] = []
    var wallFields: [Shape] = []
    func wallBox(_ center: V3, _ half: V3, _ color: V3) {
      walls.append(SanctuaryProceduralCraft.box(center, half, color: color))
      wallFields.append(Shape.box(half).moved(center))
    }
    // Structural planes retain the exact old wall and doorway dimensions.
    wallBox(V3(0, 1.36, 2.25), V3(2.75, 1.18, 0.12), wood)
    wallBox(V3(-2.63, 1.36, 0), V3(0.12, 1.18, 2.15), wood)
    wallBox(V3(2.63, 1.36, 0), V3(0.12, 1.18, 2.15), wood)
    wallBox(V3(-1.81, 1.36, -2.25), V3(0.82, 1.18, 0.12), wood)
    wallBox(V3(1.81, 1.36, -2.25), V3(0.82, 1.18, 0.12), wood)
    wallBox(V3(0, 2.25, -2.25), V3(1.00, 0.29, 0.12), wood)
    let gable: [SIMD2<Float>] = [
      SIMD2(-2.63, 2.50), SIMD2(2.63, 2.50), SIMD2(2.63, 2.60),
      SIMD2(0, 3.35), SIMD2(-2.63, 2.60),
    ]
    walls.append(SanctuaryProceduralCraft.prism(gable, near: -2.37, far: -2.13, color: wood))
    walls.append(SanctuaryProceduralCraft.prism(gable, near: 2.13, far: 2.37, color: wood))
    // Broad horizontal siding has restrained alternating values and narrow backed seams.
    for index in 0..<11 {
      let y: Float = 0.24 + (Float(index) + 0.5) * (2.30 / 11)
      let halfY: Float = 2.30 / 22 - 0.003
      let shade: Float = index % 3 == 0 ? 0.94 : (index % 3 == 1 ? 1.02 : 0.99)
      let color = wood * shade
      walls.append(SanctuaryProceduralCraft.box(V3(0, y, 2.373), V3(2.63, halfY, 0.008), color: color))
      for side: Float in [-1, 1] {
        walls.append(SanctuaryProceduralCraft.box(V3(side * 2.753, y, 0), V3(0.008, halfY, 2.15), color: color))
        walls.append(SanctuaryProceduralCraft.box(V3(side * 1.81, y, -2.373), V3(0.82, halfY, 0.008), color: color))
      }
      if y - halfY >= 1.96 {
        walls.append(SanctuaryProceduralCraft.box(V3(0, y, -2.373), V3(1.0, halfY, 0.008), color: color))
      }
    }
    let wallField = wallFields.dropFirst().reduce(wallFields[0]) { $0.joined($1) }
      .joined(.box(V3(2.63, 0.425, 0.12)).moved(V3(0, 2.925, -2.25)))
      .joined(.box(V3(2.63, 0.425, 0.12)).moved(V3(0, 2.925, 2.25)))

    var floor = [SanctuaryProceduralCraft.box(V3(0, 0.115, 0), V3(2.75, 0.115, 2.35), color: floorColor * 0.8)]
    for index in 0..<15 {
      let width: Float = 5.50 / 15
      let x: Float = -2.75 + (Float(index) + 0.5) * width
      let shade: Float = index % 3 == 0 ? 0.96 : (index % 3 == 1 ? 1.03 : 1)
      floor.append(SanctuaryProceduralCraft.box(V3(x, 0.235, 0), V3(width * 0.5 - 0.002, 0.005, 2.35), color: floorColor * shade))
    }

    let roofColor = V3(0.25, 0.27, 0.25)
    let roofLeft: [SIMD2<Float>] = [
      SIMD2(-2.95, 2.49), SIMD2(0, 3.335), SIMD2(0, 3.585), SIMD2(-2.95, 2.74),
    ]
    let roofRight: [SIMD2<Float>] = [
      SIMD2(0, 3.335), SIMD2(2.95, 2.49), SIMD2(2.95, 2.74), SIMD2(0, 3.585),
    ]
    let roof = ParametricMesh.joined([
      SanctuaryProceduralCraft.prism(roofLeft, near: -2.48, far: 2.48, color: roofColor),
      SanctuaryProceduralCraft.prism(roofRight, near: -2.48, far: 2.48, color: roofColor * 1.035),
    ])
    var framing: [Mesh] = []
    var panes: [Mesh] = []
    for side: Float in [-1, 1] {
      // Door trim sits on the wall side of the original opening, never narrowing it.
      framing.append(SanctuaryProceduralCraft.box(V3(side * 1.04, 1.10, -2.401), V3(0.05, 0.86, 0.032), color: trim))
      for z: Float in [-2.32, 2.32] {
        framing.append(SanctuaryProceduralCraft.box(V3(side * 2.64, 1.39, z), V3(0.075, 1.15, 0.075), color: trim))
      }
      let x: Float = side * 1.81
      let windowWood = V3(0.67, 0.60, 0.47)
      framing.append(SanctuaryProceduralCraft.box(V3(x, 1.00, -2.405), V3(0.48, 0.045, 0.055), color: windowWood))
      framing.append(SanctuaryProceduralCraft.box(V3(x, 1.82, -2.405), V3(0.48, 0.045, 0.04), color: windowWood))
      for edge: Float in [-1, 1] {
        framing.append(SanctuaryProceduralCraft.box(V3(x + edge * 0.435, 1.41, -2.405), V3(0.045, 0.37, 0.04), color: windowWood))
      }
      framing.append(SanctuaryProceduralCraft.box(V3(x, 1.41, -2.405), V3(0.024, 0.37, 0.04), color: windowWood))
      framing.append(SanctuaryProceduralCraft.box(V3(x, 1.41, -2.405), V3(0.39, 0.024, 0.04), color: windowWood))
      panes.append(SanctuaryProceduralCraft.box(V3(x, 1.41, -2.391), V3(0.40, 0.37, 0.012), color: V3(0.31, 0.40, 0.40)))
    }
    framing.append(SanctuaryProceduralCraft.box(V3(0, 2.005, -2.401), V3(1.09, 0.045, 0.032), color: trim))
    func piece(_ suffix: String, shape: Shape, mesh: Mesh, roughness: Float = 0.87, shadow: Bool = true) -> PartDesign {
      PartDesign(name: "Construction cabin \(suffix)", shape: shape,
        color: V3(repeating: 1), material: 7, roughness: roughness, castsShadow: shadow,
        proceduralMesh: mesh)
    }
    return [
      piece("timber walls", shape: wallField, mesh: ParametricMesh.joined(walls)),
      piece("plank floor", shape: .box(V3(2.75, 0.12, 2.35)).moved(V3(0, 0.12, 0)), mesh: ParametricMesh.joined(floor)),
      piece("roof", shape: .box(V3(2.95, 0.548, 2.48)).moved(V3(0, 3.0375, 0)), mesh: roof, roughness: 0.94),
      piece("joinery", shape: .box(V3(2.715, 1.27, 2.48)).moved(V3(0, 1.27, 0)), mesh: ParametricMesh.joined(framing)),
      piece("window panes", shape: .box(V3(2.22, 0.37, 0.012)).moved(V3(0, 1.41, -2.391)), mesh: ParametricMesh.joined(panes), roughness: 0.28, shadow: false),
    ]
  }()

  private static func design(for kind: EcologyStructureKind) -> [PartDesign] {
    func part(
      _ suffix: String, _ shape: Shape, _ color: V3, material: Int = 7,
      roughness: Float = 0.86, shadow: Bool = true
    ) -> PartDesign {
      PartDesign(
        name: "Ecology \(kind.rawValue) \(suffix)", shape: shape, color: color,
        material: material, roughness: roughness, castsShadow: shadow)
    }
    switch kind {
    case .dam:
      let logs = Shape.capsule(V3(-1, 0.18, -0.24), V3(1, 0.18, -0.24), 0.12)
        .joined(.capsule(V3(-0.92, 0.39, -0.05), V3(0.94, 0.39, -0.05), 0.11))
        .joined(.capsule(V3(-0.84, 0.60, 0.14), V3(0.86, 0.60, 0.14), 0.10))
        .joined(.capsule(V3(-0.78, 0.15, 0.30), V3(0.72, 0.57, -0.20), 0.075))
        .joined(.capsule(V3(-0.68, 0.55, -0.18), V3(0.76, 0.17, 0.27), 0.070))
      return [
        part("interlocked logs", logs, V3(0.36, 0.24, 0.15), material: 4, roughness: 0.94),
        part(
          "mud packing", ellipsoid(V3(0, 0.18, 0.08), V3(0.92, 0.20, 0.34)),
          V3(0.28, 0.24, 0.18), roughness: 0.98),
      ]
    case .nest:
      var weave = Shape.torus(0.55, 0.09).moved(V3(0, 0.10, 0))
      for index in 0..<8 {
        let angle = Float(index) * .pi / 8
        let direction = V3(cos(angle), 0, sin(angle))
        weave = weave.joined(
          .capsule(-direction * 0.58 + V3(0, 0.08, 0), direction * 0.58 + V3(0, 0.13, 0), 0.028))
      }
      return [
        part("woven rim", weave, V3(0.51, 0.39, 0.22), material: 4, roughness: 0.96),
        part(
          "soft bowl", ellipsoid(V3(0, 0.055, 0), V3(0.50, 0.055, 0.43)),
          V3(0.62, 0.55, 0.34), material: 8, roughness: 0.98),
      ]
    case .restingBed:
      return [
        part(
          "grass ring",
          Shape.torus(0.58, 0.13).stretched(V3(1, 0.42, 0.78)).moved(V3(0, 0.07, 0)),
          V3(0.54, 0.50, 0.29), material: 8, roughness: 0.99),
        part(
          "pressed center", ellipsoid(V3(0, 0.035, 0), V3(0.50, 0.035, 0.34)),
          V3(0.42, 0.43, 0.25), material: 8, roughness: 1, shadow: false),
      ]
    case .seedCache:
      let caps = ellipsoid(V3(-0.22, 0.25, -0.12), V3(0.13, 0.07, 0.09))
        .joined(ellipsoid(V3(0.02, 0.28, -0.17), V3(0.12, 0.065, 0.08)))
        .joined(ellipsoid(V3(0.24, 0.23, 0.02), V3(0.13, 0.07, 0.09)))
      return [
        part(
          "leaf mound", ellipsoid(V3(0, 0.12, 0), V3(0.66, 0.19, 0.54)),
          V3(0.39, 0.42, 0.22), material: 8, roughness: 0.98),
        part("visible seeds", caps, V3(0.70, 0.46, 0.22), material: 6, roughness: 0.72),
      ]
    }
  }

  /// Shared production source entry for the isolated study and geometry receipts.
  static func gardenReedBatch() throws -> SceneBatch { try sceneBatch(reedDesign) }

  /// Retained exact pre-promotion source for matched study comparisons.
  static func gardenReedBaselineBatch() throws -> SceneBatch { try sceneBatch(originalReedDesign) }

  private static let reedDesign = PartDesign(
    name: GardenReedStudy.name,
    shape: .box(V3(0.8, 0.8, 0.8)).moved(V3(0, 0.4, 0)),
    color: V3(0.35, 0.48, 0.24), material: 8, roughness: 0.92,
    proceduralMesh: GardenReedStudy.mesh())

  private static let originalReedDesign = PartDesign(
    name: "Garden reed cluster",
    shape: Shape.capsule(V3(-0.12, 0, 0.05), V3(-0.11, 0.82, 0.04), 0.018)
      .joined(.capsule(V3(0.09, 0, -0.08), V3(0.10, 1.02, -0.06), 0.020))
      .joined(.capsule(V3(0.19, 0, 0.12), V3(0.18, 0.69, 0.11), 0.017))
      .joined(.capsule(V3(-0.04, 0, -0.16), V3(-0.02, 0.90, -0.15), 0.016)),
    color: V3(0.35, 0.48, 0.24), material: 8, roughness: 0.92)

  private static let flowerDesign = PartDesign(
    name: "Garden flower cluster",
    shape: .box(V3(0.11, 0.19, 0.11)).moved(V3(0, 0.18, 0)),
    color: V3(repeating: 1), material: 7, roughness: 0.87, castsShadow: false,
    proceduralMesh: SanctuaryProceduralCraft.flower())

  private static let waterDesign = PartDesign(
    name: "Garden shallow water", shape: ellipsoid(.zero, V3(1, 0.025, 1)),
    color: V3(0.28, 0.57, 0.63), material: 7, roughness: 0.16, metallic: 0.05,
    castsShadow: false)

  private static func sceneBatch(_ design: PartDesign) throws -> SceneBatch {
    // Each source is compiled once. A sampled closed surface keeps the ray's thin rim smooth.
    let resolution = design.resolution ?? (design.name == "Wildlife sunhare eyes" ? 64
      : design.name.hasPrefix("Construction") || design.name.hasPrefix("Ecology")
      || design.name == "Wildlife sunhare head"
      || design.name.hasPrefix("Wildlife moonhart") || design.name.hasPrefix("Wildlife cloudRay")
      ? 48 : 24)
    let mesh: Mesh
    if let authored = design.proceduralMesh {
      mesh = authored
    } else if design.cloudRaySurface {
      mesh = cloudRayMesh()
    } else {
      mesh = try Mesher.compile(design.shape, resolution: resolution)
    }
    var batch = SceneBatch(
      name: design.name, mesh: mesh,
      instances: [Instance(tint: design.color, kind: Float(design.material))])
    if design.cloudRaySurface {
      batch.skinJoints = [design.name, "Wildlife cloudRay wing -1.0", "Wildlife cloudRay wing 1.0"]
      batch.skinWeights = mesh.vertices.map { vertex in
        let x = vertex.position.x
        let t = clamp((abs(x) - 0.38) / 1.50, 0, 1)
        return SkinWeight(0, x < 0 ? 1 : 2, blend: t * t * (3 - 2 * t))
      }
    }
    batch.roughness = design.roughness
    batch.metallic = design.metallic
    return batch
  }

  private static func cloudRayMesh() -> Mesh {
    // Longitude/latitude samples follow the rounded edge instead of a coarse cubic grid.
    return ParametricMesh.surface(u: 96, v: 40, position: cloudRayPoint, tint: cloudRayTint)
  }

  private static func cloudRayPoint(_ u: Float, _ v: Float) -> V3 {
    let azimuth: Float = u * (2 * Float.pi)
    let latitude: Float = (0.0001 + v * 0.9998) * Float.pi
    let radial: Float = sin(latitude)
    let x: Float = 2.10 * radial * cos(azimuth)
    let span: Float = abs(x) / 2.10
    let sweep: Float = 0.26 * span * span
    let z: Float = 1.40 * radial * sin(azimuth) + sweep
    let foreheadZ: Float = z + 0.90
    let foreheadExponent: Float = -(x * x / 0.55) - (foreheadZ * foreheadZ / 0.32)
    let forehead: Float = 0.11 * exp(foreheadExponent)
    let thickness: Float = 0.245 * cos(latitude)
    let wingLift: Float = 0.14 * span * span
    let y: Float = thickness + wingLift - 0.025 * z + forehead
    return V3(x, y, z)
  }

  private static func cloudRayTint(_ u: Float, _ v: Float) -> V3 {
    let underside: Float = max(0, -cos(v * Float.pi))
    let tint = V3(0.32, 0.23, 0.13) * (underside * 0.74)
    return V3(repeating: 1) + tint
  }

  static func hasAuthoredMotion(_ species: WildlifeSpecies) -> Bool {
    species == .sunhare || species == .moonhart || species == .cloudRay || species == .canopyGlider
  }

  // Cache recipe metadata: live posing does not recreate fields or meshes.
  private static let motionDesigns: [WildlifeSpecies: [PartDesign]] = [
    .sunhare: design(for: .sunhare),
  ]
  private static func inspectionParts(for species: WildlifeSpecies,
    parameters: [String: Float] = [:]) -> [AssetPart] {
    guard hasAuthoredMotion(species) else { return [] }
    let designs = design(for: species, parameters: parameters)
    let headName = "Wildlife \(species.rawValue) head"
    var result = designs.map { design in
      let parent: String?
      let pivot: V3
      switch design.role {
      case .eyes:
        parent = headName
        pivot = (design.shape.bounds.min + design.shape.bounds.max) * 0.5
      case .ear:
        parent = headName
        pivot = design.pivot
      default:
        parent = nil
        pivot = design.pivot
      }
      let bounds = design.shape.bounds
      // The procedural compiler supplies the real surface; these bounds register semantic parts.
      return AssetPart(name: design.name,
        joint: PartJoint(id: design.name, parent: design.parent ?? parent, pivot: pivot),
        field: .ellipsoid((bounds.min + bounds.max) * 0.5, (bounds.max - bounds.min) * 0.5),
        color: [design.color.x, design.color.y, design.color.z],
        material: design.material, roughness: design.roughness)
    }
    if species == .cloudRay {
      for side: Float in [-1, 1] {
        let name = "Wildlife cloudRay wing \(side)"
        result.append(AssetPart(name: name, joint: PartJoint(id: name, pivot: V3(side * 0.38, 0, 0)),
          field: .ellipsoid(V3(side, 0, 0), V3(0.1, 0.1, 0.1)), color: [1, 1, 1]))
      }
    }
    return result
  }

  /// The same semantic channels drive Sunhare's production and studio rig.
  private static func semanticPoses(_ simulation: CreatureSimulation, identifier: String,
    player: SIMD2<Float>, working: Bool, time: Float? = nil, phase: Float?) -> [String: JointPose]
  {
    let time = time ?? simulation.time
    let head = semanticHeadAngles(simulation, identifier: identifier, player: player,
      time: time, working: working, gentleRefusal: true)
    let blink = CreatureMotion().blink(at: time + identifierPhase(identifier))
    // Only the production phase creates lift; support joints are exactly
    // identity during both stance intervals, including anticipatory crouch.
    let p = phase ?? 0
    let airborne = phase != nil && p > 0.22 && p < 0.72
    let air: Float = airborne ? max(0, CreatureMotion.flight(p)) : 0
    let prep: Float = phase != nil && p < 0.22 ? sin(p / 0.22 * .pi) : 0
    let land: Float = phase != nil && p > 0.72 ? sin((p - 0.72) / 0.28 * .pi) : 0
    let breath = sin(time * 2.1 + identifierPhase(identifier)) * 0.003 * (1 - air)
    let torsoScale = V3(1 + prep * 0.025 + land * 0.02,
      1 - prep * 0.055 + air * 0.025 - land * 0.045, 1 + prep * 0.02 + land * 0.015)
    let flightPitch: Float = airborne ? air * cos((p - 0.22) / 0.5 * .pi) * 6 : 0
    let refusal: Float = SanctuaryWildlifePerformance.refusalElapsed(simulation).map { age in
      CreatureMotion.smooth(age / 0.28) * (1 - CreatureMotion.smooth((age - 2.1) / 0.75))
    } ?? 0
    var result: [String: JointPose] = [:]
    for part in motionDesigns[.sunhare] ?? [] {
      switch part.role {
      case .body:
        if part.name == "Wildlife sunhare body" {
          result[part.name] = JointPose(offset: V3(0, breath - prep * 0.045 - land * 0.03, 0),
            rotation: V3(-prep * 7 + flightPitch + land * 6, 0, 0), scale: torsoScale)
        }
      case .head:
        // Preserve the soft skull's proportions while its neck follows the
        // torso's contact compression and the brain continues to aim the face.
        result[part.name] = JointPose(rotation: head, scale: V3(repeating: 1) / torsoScale)
      case .eyes: result[part.name] = JointPose(scale: V3(1, max(0.06, 1 - blink * 0.94), 1))
      case .leg(let id):
        if airborne {
          result[part.name] = JointPose(offset: V3(0, air * 0.055, 0),
            rotation: V3(air * (id < 2 ? -22 : 16), 0, 0))
        }
      case .ear(let side):
        let flick = pow(max(0, sin(time * 1.1 + side * 1.7)), 12)
        result[part.name] = JointPose(rotation: V3(
          prep * 10 - air * 12 + land * 7 + flick * 4 + refusal * 12,
          side * flick * 4, side * refusal * 9))
      case .expressive:
        result[part.name] = JointPose(rotation: V3(0,
          sin(time * 5 + identifierPhase(identifier)) * expressiveAmplitude(simulation.state), 0))
      }
    }
    return result
  }

  static func sharedPoses(for species: WildlifeSpecies,
    simulation: CreatureSimulation, identifier: String, player: SIMD2<Float>,
    working: Bool, time: Float, phase: Float?) -> [String: JointPose]
  {
    switch species {
    case .sunhare:
      return semanticPoses(simulation, identifier: identifier, player: player,
        working: working, time: time, phase: phase)
    case .canopyGlider:
      return SanctuaryCanopyGliderDesign.poses(time: time, gaze: simulation.gaze, mood: simulation.state)
    case .moonhart, .cloudRay:
      return SanctuaryMountMotion.poses(for: species, time: time, phase: phase,
        gaze: simulation.gaze, mood: simulation.state, parameters: [:])
    default: return [:]
    }
  }

  static func authoredRootPose(for species: WildlifeSpecies, time: Float) -> simd_float4x4 {
    guard species == .cloudRay else { return matrix_identity_float4x4 }
    return simd_float4x4(simd_quatf(angle: sin(time * 0.36) * 0.027, axis: V3(0, 0, 1)))
  }

  private static func speciesTitle(_ species: WildlifeSpecies) -> String {
    switch species {
    case .frostling: return "Frostling"
    case .sunhare: return "Sunhare"
    case .brookweaver: return "Brookweaver"
    case .reedwalker: return "Reedwalker"
    case .moonhart: return "Moonhart"
    case .cloudstepper: return "Cloudstepper"
    case .dunefox: return "Dunefox"
    case .canopyGlider: return "Canopy Glider"
    case .saltback: return "Saltback"
    case .tidepooler: return "Tidepooler"
    case .cloudRay: return "Cloud Ray"
    }
  }

  private static func structureTitle(_ kind: EcologyStructureKind) -> String {
    switch kind {
    case .dam: return "Interlocked Dam"
    case .nest: return "Woven Nest"
    case .restingBed: return "Resting Bed"
    case .seedCache: return "Seed Cache"
    }
  }

  private static func sourceRadius(for kind: EcologyStructureKind) -> Float {
    switch kind {
    case .dam: return 1.12
    case .nest: return 0.64
    case .restingBed: return 0.71
    case .seedCache: return 0.66
    }
  }

  private static func isWorkingHabitat(_ actor: WildlifeActor) -> Bool {
    actor.simulation.activeRequest == nil && actor.simulation.fear < 0.35
      && actor.relocationTarget == nil && !actor.habitatWork.completed
      && actor.habitatWork.progress > 0 && actor.simulation.state == "resting"
  }

  private static func ellipsoid(_ center: V3, _ radius: V3) -> Shape {
    Shape.sphere(1).stretched(radius).moved(center)
  }

  private static func sample(
    in patch: HabitatGarden.Patch, rng: inout SeededRandom, innerFraction: Float = 0
  ) -> SIMD2<Float> {
    let angle = rng.range(0, 2 * .pi)
    let normalizedRadius = innerFraction + sqrt(rng.next()) * (1 - innerFraction)
    let radius = normalizedRadius * patch.radius * 0.88
    return SIMD2(
      patch.center.x + cos(angle) * radius, patch.center.z + sin(angle) * radius)
  }

  private static func motionHeight(for species: WildlifeSpecies) -> Float {
    switch species {
    case .moonhart, .cloudstepper: return 0.25
    case .reedwalker: return 0.18
    case .cloudRay: return 0.45
    case .tidepooler: return 0.06
    default: return 0.13
    }
  }

  private static func motionYOffset(
    for species: WildlifeSpecies, time: Float, phase: Float?, morphologyScale: Float = 1
  ) -> Float {
    // World elevation owns ground/water support and species flight clearance. This shared
    // animation offset contains only motion so gameplay and studio cannot add that twice.
    let hop = (species == .cloudRay ? nil : phase).map {
      CreatureMotion.flight($0) * motionHeight(for: species) * morphologyScale
    } ?? 0
    return hop + hoverBob(for: species, time: time)
  }

  /// These studies use the same pose functions and production hop phase as live wildlife.
  private static func inspectionAnimation(for species: WildlifeSpecies) -> AnimationDefinition? {
    guard species == .sunhare || hasAuthoredMotion(species) else { return nil }
    let flying = species == .cloudRay || species == .canopyGlider
    return SanctuaryWildlifePerformance.make(identifier: "\(species.rawValue)-001", flying: flying,
      poses: { frame in
        sharedPoses(for: species, simulation: frame.brain, identifier: frame.identifier,
          player: frame.player, working: false, time: frame.time, phase: frame.phase)
      }, root: { frame in
        // The studio plane is at the source bounds; leave room for the entire downstroke.
        let clearance: Float = flying ? 0.60 : 0
        let y = motionYOffset(for: species, time: frame.time, phase: frame.phase) + clearance
        let acting = species == .sunhare
          ? semanticBodyPose(frame.brain, identifier: frame.identifier, time: frame.time,
            working: false, gentleRefusal: true)
          : authoredRootPose(for: species, time: frame.time)
        return transform(V3(frame.position.x, y, frame.position.y), V3(repeating: 1), frame.yaw) * acting
      })
  }

  static func placement(
    for actor: WildlifeActor, player: PlayerCamera, journey: SanctuaryJourney,
    creatureElevation: (WildlifeActor) -> SanctuaryCreatureElevation
  ) -> ActorPlacement {
    guard journey.companionID == actor.id, journey.mode != .walking else {
      let p = actor.position
      return ActorPlacement(
        position: V3(p.x, creatureElevation(actor).rootY, p.y), yaw: actor.yaw, attached: false)
    }
    let recipe = SanctuaryMountAttachment.recipe(for: actor.species, mode: journey.mode)
    return ActorPlacement(
      position: recipe.rootPosition(player: player, morphologyScale: actor.morphologyScale),
      // Camera yaw increases toward +X; the right-handed render transform takes
      // local -Z toward -X. Convert at attachment, as the animal brain already does.
      yaw: -player.yaw, attached: true)
  }

  private static func hoverBob(for species: WildlifeSpecies, time: Float) -> Float {
    switch species {
    case .canopyGlider: return sin(time * 1.9) * 0.09
    case .cloudRay: return sin(time * 0.72) * 0.28
    default: return 0
    }
  }

  static func identifierPhase(_ id: String) -> Float {
    var value: UInt32 = 2_166_136_261
    for byte in id.utf8 { value = (value ^ UInt32(byte)) &* 16_777_619 }
    return Float(value % 10_000) / 10_000 * 2 * .pi
  }

  private static func translation(_ p: V3) -> simd_float4x4 {
    var result = matrix_identity_float4x4
    result.columns.3 = SIMD4(p, 1)
    return result
  }

  private static func scale(_ value: V3) -> simd_float4x4 {
    simd_float4x4(
      diagonal: SIMD4(value.x, value.y, value.z, 1))
  }

  private static func around(
    _ pivot: V3, rotation: simd_float4x4 = matrix_identity_float4x4,
    scale: V3 = V3(repeating: 1)
  ) -> simd_float4x4 {
    translation(pivot) * rotation * Self.scale(scale) * translation(-pivot)
  }
}
