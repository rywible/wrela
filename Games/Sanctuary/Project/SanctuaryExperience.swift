import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

final class SanctuaryExperience: GameExperience {
  private enum ClimateDayState: String, Hashable {
    case morning, noon, golden, sunset, afterglow, night
  }
  private enum ClimateWeatherState: String, Hashable {
    case clear, overcast, rain
  }
  private struct ClimatePresentationKey: Equatable {
    var day: ClimateDayState
    var weather: ClimateWeatherState
    var seed: UInt32
  }
  let graphics: MetalRenderer
  var world: GardenWorld
  var batches: [GPUBatch]
  private var stagedStreamUpdate: SanctuaryStreamUpdate?
  private var stagedStreamBatches: [GPUBatch] = []
  private var lastStreamingUploadFrame = -1
  private var markedStreamGeneration: UInt64?
  private var streamingUploadBytesLastFrame = 0
  private var streamingUploadBatchesLastFrame = 0
  private var streamingOversizedBatchBytes = 0
  private var streamingUploadMillisecondsLastFrame: Double = 0
  private var streamingUploadDetailsLastFrame: [[String: Any]] = []
  private var streamingMaximumBatchUploadMilliseconds: Double = 0
  private var streamingSlowestBatchUpload: [String: Any] = [:]
  var pod: GPUBatch
  var sim: SanctuaryWorld
  var simulation: any GameSimulation { sim }
  var surfaceInfluences: SurfaceInfluenceSnapshot { sim.surfaceInfluenceSnapshot }
  var podScale: Float {
    get { sim.podScale }
    set { sim.podScale = newValue }
  }
  var expedition: ExpeditionController? { sim.controller }
  var expeditionPresentation: ExpeditionPresentation?
  var livingPresentation: LivingWorldPresentation?
  private let placementSession = SanctuaryPlacementSession()
  private var placementPresentation: SanctuaryPlacementPresentation?
  private let natureSession = SanctuaryNatureSession()
  private var naturePresentation: SanctuaryNaturePreviewPresentation?
  /// Presentation support follows published terrain/water, including ecology pools
  /// that share no garden revision. This is transient cache identity only.
  struct NaturePresentationSupportKey: Equatable {
    let garden: SanctuaryGardenSurfaceSource
    let ecologicalWater: [EcologyWaterFact]
  }
  private var naturePublishedSupport: NaturePresentationSupportKey?
  private var naturePresentationSupportRevision: UInt64 = 0
  /// Typed renderer handoff for the transient brush; a nature preview never
  /// becomes a render item or save without the native session's confirmation.
  var currentNaturePreview: SanctuaryNatureSession.Preview? { natureSession.currentPreview }
  var lastNatureCommittedOutcome: SanctuaryNatureSession.CommittedOutcome? {
    natureSession.lastCommittedOutcome
  }
  private struct PlacementAimKey: Equatable {
    var position: V3
    var yaw: Float
    var pitch: Float
    var supportRevision: UInt64
    var constructionRevision: UInt64
    var draft: SanctuaryPlacementIntent.Draft?
    var fixedSupport: Bool
    var reach: Float
  }
  private var placementAimKey: PlacementAimKey?
  private var cachedPlacementAim: SanctuaryPlacementSession.Aim?
  private var groundTracePresentation: GroundTracePresentation?
  private struct GroundSupportKey: Equatable {
    var garden: HabitatGarden?
    var construction: PersonalConstruction?
    var water: [EcologyWaterFact]
  }
  private var groundSupportKey: GroundSupportKey?
  private var groundSupportRevision: UInt64 = 0
  var visibilityDistance: Float { 32_000 }
  var camera: PlayerCamera {
    get { sim.camera }
    set { sim.camera = newValue }
  }
  // Experimental opt-in remains off until the bounded native model study passes.
  let interpreterEnabled = ProcessInfo.processInfo.environment["SANCTUARY_ENABLE_INTERPRETER"] == "1"
  lazy var creatureInterpreter = CreatureInterpreter(modelUse: interpreterEnabled ? .whenAvailable : .disabled)
  var interpretationTask: Task<Void, Never>?
  var interpretationGeneration: UInt64 = 0
  var interpretationDiagnostic = "authored fallback"
  let creatureAudio = SanctuaryCreatureAudio()
  private var applyingClimate = false
  private var climateAutomatic = true
  private var climateKey: ClimatePresentationKey?
  private var climateSample: SanctuaryClimate.Sample?
  private var observedClimateElapsed: Double?
  var lighting = GameLighting() {
    didSet {
      guard !applyingClimate, Self.climateLightingChanged(from: oldValue, to: lighting) else { return }
      climateAutomatic = false
    }
  }
  var scene: String { "garden" }
  var position: V3 {
    get { camera.position }
    set { camera.position = newValue }
  }
  var yaw: Float {
    get { camera.yaw }
    set { camera.yaw = newValue }
  }
  var pitch: Float {
    get { camera.pitch }
    set { camera.pitch = newValue }
  }
  var forward: V3 { camera.forward }
  var keys = Set<UInt16>()
  var specimen: AnimatedAsset?
  var specimenTime: Float = 0
  var specimenPlacement = matrix_identity_float4x4

  init(graphics: MetalRenderer) throws {
    self.graphics = graphics
    let treeSource = try AssetSource.load("tree")
    sim = try SanctuaryWorld(root: ProjectContext.dataRoot, parameters: treeSource.parameters)
    world = try GardenWorld(terrain: sim.world.terrain, source: treeSource,
      initialPosition: sim.camera.position, garden: sim.controller.state.garden,
      boulders: sim.controller.state.boulders, construction: sim.controller.state.buildings)
    batches = world.batches.map(graphics.upload)
    sim.enableHostCommittedRegionalCollision(initialLayout: world.layout,
      garden: world.publishedGarden, boulders: world.publishedBoulders)
    pod = graphics.upload(
      SceneBatch(name: "Seed pod", mesh: world.studioMesh, instances: [Instance()]))
    expeditionPresentation = try ExpeditionPresentation(graphics: graphics, terrain: world.terrain)
    synchronizeCreatureMotion()
    livingPresentation = try LivingWorldPresentation(graphics: graphics)
    placementPresentation = try SanctuaryPlacementPresentation(graphics: graphics)
    naturePresentation = SanctuaryNaturePreviewPresentation(graphics: graphics)
    groundTracePresentation = GroundTracePresentation(graphics: graphics)
    refreshClimatePresentation(force: true)
  }
  var podPosition: V3 {
    V3(
      5,
      world.terrain.height(5, 12) - world.studioShape.bounds.min.y * podScale
        * GardenWorld.podUnitScale, 12)
  }

  func move(_ local: V3) { sim.move(local) }
  private func synchronizeCreatureMotion() {
    // A paused load is observable before its first step. Resolve the same authored
    // motion here so opening a save never exposes CreatureMotion's fallback values.
    sim.motion = SanctuaryProject.motion(
      expeditionPresentation?.source.motion ?? MotionParameters())
  }
  func step(_ seconds: Float, running: Bool) {
    synchronizeCreatureMotion()
    sim.advance(seconds, running: running)
    refreshClimatePresentation()
    if specimen != nil { specimenTime += seconds }
    refreshWorldDetail()
  }
  func syncExpeditionPlayer() { sim.syncExpeditionPlayer() }
  func interact() throws -> String {
    if natureSession.isActive { return try confirmNature() }
    if placementSession.isActive { return try confirmPlacement() }
    return try sim.interact()
  }
  func cancelInteraction() -> Bool {
    if natureSession.isActive {
      natureSession.cancel()
      naturePresentation?.reset()
      return true
    }
    if placementSession.isActive {
      placementSession.cancel()
      return true
    }
    return false
  }
  var textInputPrompt: String? { "T · Say something to nearby wildlife…" }
  func submitText(_ text: String) throws -> String {
    do { return try sim.request(text) }
    catch WildlifePopulationError.unrecognizedRequest {
      guard interpreterEnabled else { throw WildlifePopulationError.unrecognizedRequest }
      guard interpretationTask == nil else { return "Give the animal a moment to respond." }
      let ticket = try sim.prepareCreatureInterpretation(text, requestID: UUID().uuidString)
      let generation = interpretationGeneration
      interpretationDiagnostic = "pending"
      let interpreter = creatureInterpreter
      interpretationTask = Task { @MainActor [weak self] in
        let result = await interpreter.interpret(ticket)
        guard let self, !Task.isCancelled, self.interpretationGeneration == generation else { return }
        defer { self.interpretationTask = nil }
        self.interpretationDiagnostic = "\(result.outcome) · \(Int(result.elapsedMilliseconds)) ms"
        guard let proposal = result.proposal else { return }
        do { _ = try self.sim.acceptCreatureInterpretation(proposal, ticket: ticket) }
        catch { self.interpretationDiagnostic = "rejected: \(error.localizedDescription)" }
      }
      return ""
    }
  }
  func cancelInterpretation() {
    interpretationTask?.cancel()
    interpretationTask = nil
    interpretationGeneration &+= 1
  }
  func audioCues() -> [AudioCue] {
    creatureAudio.cues(state: sim.controller.state, camera: camera,
      elevation: { sim.elevation(for: $0) })
  }
  var controls: [GameControl] {
    let legacyConstruction = Set([
      "build-cabin", "build-path", "build-bridge", "build-deck", "build-bench", "build-lantern",
      "build-fence", "building-select", "building-move", "building-resize", "building-remove",
      "building-finish", "building-turn", "building-larger", "building-smaller", "undoBuilding",
    ])
    let legacyNature = Set([
      "flowers", "grove", "reeds", "water", "raise", "lower", "smooth", "undoPlanting",
      "brush-larger", "brush-smaller", "spell-stronger", "spell-gentler",
    ])
    let ordinary = SanctuaryWorld.livingControls.filter {
      !legacyConstruction.contains($0.id) && !legacyNature.contains($0.id)
    }.map {
      GameControl(id: $0.id, title: $0.title, presentation: $0.id == "journal" ? .document : .inline)
    }
    return ordinary + Self.natureControls.map { GameControl(id: $0.0, title: $0.1) }
      + Self.placementControls.map { GameControl(id: $0.0, title: $0.1) }
  }
  func activateControl(_ id: String) throws -> String {
    if id.hasPrefix("nature-") { return try activateNatureControl(id) }
    if id.hasPrefix("placement-") {
      natureSession.reset()
      naturePresentation?.reset()
      return try activatePlacementControl(id)
    }
    let response = try sim.control(id)
    refreshWorldDetail()
    return response
  }

  private static let natureControls: [(String, String)] = [
    ("nature-flowers", "Nature · Grow flowers"), ("nature-grove", "Nature · Grow a grove"),
    ("nature-reeds", "Nature · Grow reeds"), ("nature-water", "Nature · Invite shallow water"),
    ("nature-raise", "Nature · Raise earth"), ("nature-lower", "Nature · Lower earth"),
    ("nature-smooth", "Nature · Level earth"),
    ("nature-radius-larger", "Nature · Larger brush"),
    ("nature-radius-smaller", "Nature · Smaller brush"),
    ("nature-strength-stronger", "Nature · Stronger shaping"),
    ("nature-strength-gentler", "Nature · Gentler shaping"),
    ("nature-confirm", "Nature · Cast brush"), ("nature-cancel", "Nature · Cancel brush"),
    ("nature-undo", "Nature · Undo last change"),
  ]

  private func activateNatureControl(_ id: String) throws -> String {
    // Native construction and nature drafts never coexist. This only clears
    // transient preview state; an existing placement remains saved unchanged.
    placementSession.reset()
    placementPresentation?.reset()
    let action: SanctuaryNatureIntent.Action?
    switch id {
    case "nature-flowers": action = .plant(.flowers)
    case "nature-grove": action = .plant(.grove)
    case "nature-reeds": action = .plant(.reeds)
    case "nature-water": action = .plant(.shallowWater)
    case "nature-raise": action = .sculpt(.raise)
    case "nature-lower": action = .sculpt(.lower)
    case "nature-smooth": action = .sculpt(.smooth)
    default: action = nil
    }
    if let action {
      refreshNaturePresentationSupportRevision()
      natureSession.begin(
        action, in: sim, publishedSupportRevision: naturePresentationSupportRevision)
      // Aim validity belongs to the live HUD prompt. Returning a durable
      // selection message here avoids leaving a superseded range rejection in
      // the host's separate status line after the player changes aim.
      return "Nature brush selected. Follow the E prompt."
    }
    switch id {
    case "nature-radius-larger": return try adjustNatureTools(with: "brush-larger")
    case "nature-radius-smaller": return try adjustNatureTools(with: "brush-smaller")
    case "nature-strength-stronger": return try adjustNatureTools(with: "spell-stronger")
    case "nature-strength-gentler": return try adjustNatureTools(with: "spell-gentler")
    case "nature-confirm": return try confirmNature()
    case "nature-cancel":
      natureSession.cancel()
      naturePresentation?.reset()
      return "Cancelled the nature brush."
    case "nature-undo":
      natureSession.reset()
      naturePresentation?.reset()
      let response = try sim.control("undoPlanting")
      refreshWorldDetail()
      return response.isEmpty ? "Undid the last nature change." : response
    default: throw RuntimeError.message("Unknown nature brush")
    }
  }

  private func adjustNatureTools(with control: String) throws -> String {
    let response = try sim.control(control)
    refreshNaturePresentationSupportRevision()
    _ = natureSession.refresh(
      in: sim, publishedSupportRevision: naturePresentationSupportRevision, force: true)
    return response
  }

  private func confirmNature() throws -> String {
    refreshNaturePresentationSupportRevision()
    do {
      _ = try natureSession.confirm(
        in: sim, publishedSupportRevision: naturePresentationSupportRevision)
      refreshWorldDetail()
      return "Nature answered the brush. Aim again to continue."
    } catch is SanctuaryNatureIntentError {
      // The HUD immediately owns target feedback and updates after each aim
      // change. A persistence error still escapes and remains visible until a
      // later explicit brush action changes or retries the draft.
      return "Nature brush remains active. Follow the E prompt."
    }
  }

  private static let placementControls: [(String, String)] = [
    ("placement-cabin", "Place · Cabin"), ("placement-path", "Place · Path"),
    ("placement-bridge", "Place · Bridge"), ("placement-deck", "Place · Observation deck"),
    ("placement-bench", "Place · Bench"), ("placement-lantern", "Place · Lantern"),
    ("placement-fence", "Place · Fence"),
    ("placement-select", "Place · Select construction"),
    ("placement-move", "Place · Move selected"), ("placement-turn", "Place · Turn preview 45°"),
    ("placement-larger", "Place · Larger preview"), ("placement-smaller", "Place · Smaller preview"),
    ("placement-remove", "Place · Remove selected"), ("placement-confirm", "Place · Confirm"),
    ("placement-cancel", "Place · Cancel"), ("placement-undo", "Place · Undo last change"),
  ]

  private func activatePlacementControl(_ id: String) throws -> String {
    if id.hasPrefix("placement-") && id != "placement-select" && id != "placement-move"
      && id != "placement-turn" && id != "placement-larger" && id != "placement-smaller"
      && id != "placement-remove" && id != "placement-confirm" && id != "placement-cancel"
      && id != "placement-undo"
    {
      let raw = String(id.dropFirst("placement-".count))
      guard let primitive = PersonalConstruction.Primitive(rawValue: raw) else {
        throw RuntimeError.message("Unknown construction placement")
      }
      placementSession.beginPlace(primitive, in: sim, aim: placementAim())
      return "Aim the \(raw) and press E to confirm."
    }
    switch id {
    case "placement-select":
      try placementSession.select(in: sim)
      return "Selected construction. Turn or resize it, choose Move when ready, then press E to confirm."
    case "placement-move":
      try placementSession.chooseMove()
      return "Aim the selected construction's new location, then press E to confirm."
    case "placement-turn":
      try placementSession.turn45()
      return "Turned the preview 45°. Press E to confirm."
    case "placement-larger":
      try placementSession.changeScale(by: 0.25)
      return "Enlarged the preview. Press E to confirm."
    case "placement-smaller":
      try placementSession.changeScale(by: -0.25)
      return "Reduced the preview. Press E to confirm."
    case "placement-remove":
      _ = try placementSession.remove(in: sim)
      refreshWorldDetail()
      return "Removed selected construction. Undo restores it."
    case "placement-confirm": return try confirmPlacement()
    case "placement-cancel":
      placementSession.cancel()
      return "Cancelled construction placement."
    case "placement-undo":
      _ = try placementSession.undo(in: sim)
      placementPresentation?.reset()
      refreshWorldDetail()
      return "Undid the last construction change."
    default: throw RuntimeError.message("Unknown construction placement")
    }
  }

  private func confirmPlacement() throws -> String {
    _ = try placementSession.confirm(in: sim, aim: placementAim())
    refreshWorldDetail()
    return "Placed construction."
  }
  func refreshWorldDetail() {
    do {
      let state = sim.controller.state
      if let update = try world.updateStreaming(around: position, garden: state.garden,
        boulders: state.boulders, construction: state.buildings) {
        stagedStreamUpdate = update
        stagedStreamBatches.removeAll(keepingCapacity: true)
      }
      if markedStreamGeneration != world.streamGeneration {
        sim.markRegionalPublicationPending(generation: world.streamGeneration,
          affectedKeys: world.pendingSupportKeys(garden: state.garden,
            boulders: state.boulders, construction: state.buildings))
        markedStreamGeneration = world.streamGeneration
      }
      if stagedStreamUpdate?.generation != world.streamGeneration {
        stagedStreamUpdate = nil
        stagedStreamBatches.removeAll(keepingCapacity: true)
      }
      guard lastStreamingUploadFrame != graphics.frame else { return }
      lastStreamingUploadFrame = graphics.frame
      streamingUploadBytesLastFrame = 0
      streamingUploadBatchesLastFrame = 0
      streamingUploadMillisecondsLastFrame = 0
      streamingUploadDetailsLastFrame.removeAll(keepingCapacity: true)
      guard let update = stagedStreamUpdate else { return }
      let byteBudget = 8 * 1_024 * 1_024
      while stagedStreamBatches.count < update.batches.count, streamingUploadBatchesLastFrame < 2 {
        let batch = update.batches[stagedStreamBatches.count]
        let bytes = Self.streamSourceBytes(batch)
        if streamingUploadBatchesLastFrame > 0 && streamingUploadBytesLastFrame + bytes > byteBudget { break }
        // A single indivisible source batch may exceed the byte target. Report it rather
        // than silently calling this a hard byte cap or blocking publication forever.
        if bytes > byteBudget { streamingOversizedBatchBytes = max(streamingOversizedBatchBytes, bytes) }
        let uploadStart = Date.timeIntervalSinceReferenceDate
        stagedStreamBatches.append(graphics.upload(batch))
        let uploadMilliseconds = (Date.timeIntervalSinceReferenceDate - uploadStart) * 1_000
        streamingUploadMillisecondsLastFrame += uploadMilliseconds
        let uploadDetail: [String: Any] = ["name": batch.name, "sourceBytes": bytes,
          "milliseconds": uploadMilliseconds, "preparedCPUData": batch.preparedUpload != nil,
          "generation": update.generation]
        streamingUploadDetailsLastFrame.append(uploadDetail)
        if uploadMilliseconds > streamingMaximumBatchUploadMilliseconds {
          streamingMaximumBatchUploadMilliseconds = uploadMilliseconds
          streamingSlowestBatchUpload = uploadDetail
        }
        streamingUploadBatchesLastFrame += 1
        streamingUploadBytesLastFrame += bytes
      }
      guard stagedStreamBatches.count == update.batches.count,
        world.commitStreaming(generation: update.generation) else { return }
      batches.removeAll { update.removedBatchNames.contains($0.name) }
      batches += stagedStreamBatches
      _ = sim.publishRegionalState(generation: update.generation, layout: world.layout,
        garden: world.publishedGarden, boulders: world.publishedBoulders)
      groundSupportKey = nil
      stagedStreamUpdate = nil
      stagedStreamBatches.removeAll(keepingCapacity: true)
    } catch { sim.controller.message = "World detail: \(error.localizedDescription)" }
  }

  private static func streamSourceBytes(_ batch: SceneBatch) -> Int {
    let meshes = [batch.mesh] + batch.lodMeshes
    return meshes.reduce(0) { $0 + $1.vertices.count * MemoryLayout<Vertex>.stride + $1.indices.count * 4 }
      + batch.instances.count * MemoryLayout<Instance>.stride
      + batch.skinWeights.count * MemoryLayout<SkinWeight>.stride
      + batch.grass.count * MemoryLayout<GrassBlade>.stride
  }

  /// Cache the production targeting result by the camera, edited support, and
  /// transient draft. The ray query is bounded but never repeated per overlay.
  private func placementAim() -> SanctuaryPlacementSession.Aim {
    let state = sim.controller.state
    refreshGroundSupportRevision(state)
    let tools = state.craftTools
    let draft = placementSession.intent.draft
    let key = PlacementAimKey(
      position: camera.position, yaw: camera.yaw, pitch: camera.pitch,
      supportRevision: groundSupportRevision, constructionRevision: state.buildings.revision,
      draft: draft, fixedSupport: placementSession.requiresFixedSupportTarget, reach: tools.reach)
    if key == placementAimKey, let cachedPlacementAim { return cachedPlacementAim }

    let target: SanctuaryTargetingResult
    if placementSession.requiresFixedSupportTarget, let draft {
      let support = V3(draft.location.x, draft.location.y, draft.location.z)
      target = sim.target(
        origin: camera.position, direction: support - camera.position, maxReach: tools.reach,
        ignoringPlacementID: draft.selectedPlacementID)
    } else {
      target = sim.cameraTarget(maxReach: tools.reach, ignoringPlacementID: draft?.selectedPlacementID)
    }
    let point = target.support?.point ?? target.point
    let aim = SanctuaryPlacementSession.Aim(
      location: .init(x: point.x, y: point.y, z: point.z),
      targetIsOutOfReach: target.obstruction == .rangeLimit,
      targetIsObstructed: target.obstruction != .none || target.support == nil,
      distance: target.distance, supportRevision: groundSupportRevision)
    placementAimKey = key
    cachedPlacementAim = aim
    return aim
  }

  private func refreshGroundSupportRevision(_ state: Expedition) {
    let supportKey = GroundSupportKey(garden: state.garden, construction: state.construction,
      water: state.population.ecology.waterFacts)
    if supportKey != groundSupportKey {
      groundSupportKey = supportKey
      groundSupportRevision &+= 1
      placementAimKey = nil
      cachedPlacementAim = nil
    }
  }

  /// The host can keep published terrain support behind the saved garden while
  /// streaming. Preview deformation follows that published source and only
  /// refreshes once its source is actually published. Restored branches can
  /// share a revision while containing different terrain or water edits.
  private func refreshNaturePresentationSupportRevision() {
    let source = NaturePresentationSupportKey(
      garden: SanctuaryGardenSurfaceSource(garden: sim.presentationGarden),
      ecologicalWater: sim.controller.state.population.ecology.waterFacts)
    if source != naturePublishedSupport {
      naturePublishedSupport = source
      naturePresentationSupportRevision &+= 1
    }
  }
  func loadExpeditionSlot(_ name: String) throws {
    cancelInterpretation()
    try sim.loadExpeditionSlot(name)
    synchronizeCreatureMotion()
    natureSession.reset()
    naturePresentation?.reset()
    naturePublishedSupport = nil
    naturePresentationSupportRevision = 0
    placementSession.reset()
    placementPresentation?.reset()
    placementAimKey = nil
    cachedPlacementAim = nil
    groundTracePresentation?.reset()
    groundSupportKey = nil
    keys.removeAll()
    refreshClimatePresentation(force: true)
  }
  func items() -> [RenderItem] {
    // Simulation harness restores bypass the experience. Refresh here so a
    // paused render observes the restored saved play-time climate.
    refreshClimatePresentation()
    refreshWorldDetail()
    let state = sim.controller.state
    refreshGroundSupportRevision(state)
    refreshNaturePresentationSupportRevision()
    let placementPreview = placementSession.isActive
      ? placementSession.preview(in: sim, aim: placementAim()) : nil
    _ = natureSession.refresh(in: sim, publishedSupportRevision: naturePresentationSupportRevision)
    var result = batches.map {
      RenderItem(batch: $0, castsShadow: !["Meadow", "Wildflowers", "Leaves"].contains($0.name))
    }
    result.append(
      RenderItem(
        batch: pod,
        instance: Instance(
          position: podPosition, scale: V3(repeating: podScale * GardenWorld.podUnitScale),
          tint: V3(0.48, 0.27, 0.13), kind: 6)))
    if let expedition, let expeditionPresentation {
      if sim.legacyInteractions { result += expeditionPresentation.items(expedition.state) }
      else if let livingPresentation {
        result += livingPresentation.items(population: expedition.state.population,
          garden: sim.presentationGarden ?? HabitatGarden(),
          construction: world.publishedConstruction ?? PersonalConstruction(),
          terrainHeight: world.terrain.height, waterHeight: sim.localWaterHeight,
          creatureElevation: { sim.elevation(for: $0) },
          player: camera, playSeconds: expedition.state.age, journey: expedition.state.travel)
      }
    }
    if let specimen {
      result += specimen.items(clip:"procession",time:specimenTime,placement:specimenPlacement,terrain:world.terrain.height)
    }
    if let placementPresentation {
      let garden = state.garden ?? HabitatGarden()
      result += placementPresentation.items(
        draft: placementPreview?.placement, valid: placementPreview?.isValid ?? false,
        supportRevision: groundSupportRevision,
        terrainHeight: { x, z in
          garden.surfaceHeight(baseHeight: self.world.terrain.height(x, z),
            at: .init(x: x, z: z))
        })
    }
    if let naturePresentation {
      result += naturePresentation.items(
        preview: currentNaturePreview, supportRevision: naturePresentationSupportRevision,
        supportHeight: sim.flightSurfaceHeight)
    }
    if let groundTracePresentation {
      result += groundTracePresentation.items(snapshot: surfaceInfluences,
        playerPosition: camera.position, supportRevision: groundSupportRevision) { event, x, z in
          guard let soil = self.sim.groundTraceSupport(event: event, x: x, z: z) else { return nil }
          return .init(height: soil.height, strength: soil.strength)
        }
    }
    return result
  }
  var hud: GameHUD {
    guard let state = expedition?.state else {
      return GameHUD(title: "Sanctuary", objective: "Explore", detail: "", prompt: nil)
    }
    let site = SanctuaryGeography.landmarks.min {
      distance($0.coordinate, state.player) < distance($1.coordinate, state.player)
    }
    let place = site.map { distance($0.coordinate, state.player) < 180 ? $0.name : "The open landscape" }
      ?? "Sanctuary"
    if natureSession.isActive {
      refreshNaturePresentationSupportRevision()
      let preview = natureSession.refresh(
        in: sim, publishedSupportRevision: naturePresentationSupportRevision)
      let status = preview?.valid == true ? "ready" : "adjust aim"
      let brush = preview.map { String(format: "%.0f m brush", $0.radius) } ?? "brush"
      return GameHUD(
        title: "Nature · \(natureSession.actionTitle)", objective: "\(status) · E casts",
        detail: "\(brush) · Choose another brush or Cancel to finish",
        prompt: SanctuaryNatureSession.feedback(for: preview?.rejection))
    }
    if placementSession.isActive, let draft = placementSession.intent.draft {
      let preview = placementSession.currentPreview
      let status = preview?.isValid == true ? "ready" : "adjust aim"
      let reason = preview?.rejection.map { SanctuaryPlacementIntentError.rejected($0).localizedDescription }
      return GameHUD(
        title: "Place · \(draft.primitive.rawValue.capitalized)", objective: "\(status) · E confirms",
        detail: "Move, turn, or resize the preview · Cancel leaves your saved construction unchanged",
        prompt: reason ?? "E confirm · Cancel placement")
    }
    let mode = state.travel.mode
    let detail = "WASD \(mode == .walking ? "walk" : mode == .riding ? "ride" : "fly") · Shift faster · T reach out · E observe"
    return GameHUD(title: place, objective: "", detail: detail,
      prompt: mode != .walking ? "Dismount" : sim.nearbyAnimal != nil ? "Greet nearby wildlife" : "Look for traces")
  }
  var snapshot: [String: Any] {
    let climate = climateSample ?? SanctuaryClimate.sample(
      elapsedPlaySeconds: sim.controller.state.age, seed: sim.controller.state.creatureState.seed)
    let climateSnapshot: [String: Any] = [
      "mode": climateAutomatic ? "automatic" : "manual",
      "dayState": climateKey?.day.rawValue ?? "unapplied",
      "weatherState": climateKey?.weather.rawValue ?? "unapplied",
      "timeOfDay": climate.timeOfDay,
      "cloud": climate.cloud,
      "rain": climate.rain,
      "wind": climate.wind,
      "outdoorAmbientFloor": [
        lighting.outdoorAmbientFloor.x, lighting.outdoorAmbientFloor.y,
        lighting.outdoorAmbientFloor.z,
      ],
      "cachePolicy": "coarse named states; live candidate cache steps, explicit paused rebuilds",
    ]
    let decisions: [String] = sim.controller.state.decisionJournal.decisions.compactMap {
      try? SimulationCoding.encode($0).base64EncodedString()
    }
    let expeditionSnapshot: [String: Any] = expedition?.snapshot ?? [:]
    var result: [String: Any] = [:]
    result["interpreterEnabled"] = interpreterEnabled
    result["interpreterStatus"] = interpretationDiagnostic
    result["recordedCreatureDecisions"] = decisions
    result["specimen"] = specimen?.source.id ?? ""
    result["specimenTime"] = specimenTime
    result["livingWorld"] = sim.livingObservations
    result["groundTraces"] = [
      "rendered": groundTracePresentation?.renderedTraceCount ?? 0,
      "cached": groundTracePresentation?.cachedEventCount ?? 0,
      "supportQueriesLastFrame": groundTracePresentation?.supportQueriesLastFrame ?? 0,
    ]
    let placementPreview = placementSession.currentPreview
    var placementDiagnostics: [String: Any] = [:]
    placementDiagnostics["active"] = placementSession.isActive
    placementDiagnostics["primitive"] = placementPreview?.placement.primitive.rawValue ?? ""
    placementDiagnostics["selectedPlacementID"] = placementPreview?.placement.selectedPlacementID ?? 0
    placementDiagnostics["valid"] = placementPreview?.isValid ?? false
    if let rejection = placementPreview?.rejection {
      placementDiagnostics["rejection"] = String(describing: rejection)
    } else {
      placementDiagnostics["rejection"] = ""
    }
    placementDiagnostics["supportRevision"] = groundSupportRevision
    placementDiagnostics["supportQueriesLastFrame"] = placementPresentation?.supportQueriesLastFrame ?? 0
    result["placementPreview"] = placementDiagnostics
    let naturePreview = natureSession.currentPreview
    var natureDiagnostics: [String: Any] = [:]
    natureDiagnostics["active"] = natureSession.isActive
    natureDiagnostics["valid"] = naturePreview?.valid ?? false
    natureDiagnostics["radius"] = naturePreview?.radius ?? 0
    if let point = naturePreview?.targetPoint {
      natureDiagnostics["targetPoint"] = [point.x, point.y, point.z]
    } else {
      natureDiagnostics["targetPoint"] = []
    }
    natureDiagnostics["feedback"] = SanctuaryNatureSession.feedback(for: naturePreview?.rejection)
    natureDiagnostics["supportRevision"] = naturePresentationSupportRevision
    natureDiagnostics["supportQueriesLastFrame"] = naturePresentation?.supportQueriesLastFrame ?? 0
    if let outcome = natureSession.lastCommittedOutcome {
      natureDiagnostics["sourceEventID"] = outcome.sourceEventID
      natureDiagnostics["playSeconds"] = outcome.playSeconds
    }
    result["naturePreview"] = natureDiagnostics
    result["natureMagicEvents"] = NatureMagicPresentation.activeEvents(
      garden: sim.controller.state.garden ?? HabitatGarden(),
      playSeconds: sim.controller.state.age).map(\.diagnostics)
    result["streamedDetailChunkIDs"] = world.streamedKeys.sorted {
      $0.z == $1.z ? $0.x < $1.x : $0.z < $1.z
    }.map(\.id)
    var streamDiagnostics = world.streamingDiagnostics
    streamDiagnostics["uploadBatchLimitPerFrame"] = 2
    streamDiagnostics["uploadByteTargetPerFrame"] = 8 * 1_024 * 1_024
    streamDiagnostics["uploadBatchesLastFrame"] = streamingUploadBatchesLastFrame
    streamDiagnostics["sourceUploadBytesLastFrame"] = streamingUploadBytesLastFrame
    streamDiagnostics["uploadMillisecondsLastFrame"] = streamingUploadMillisecondsLastFrame
    streamDiagnostics["uploadDetailsLastFrame"] = streamingUploadDetailsLastFrame
    streamDiagnostics["maximumBatchUploadMilliseconds"] = streamingMaximumBatchUploadMilliseconds
    streamDiagnostics["slowestBatchUpload"] = streamingSlowestBatchUpload
    streamDiagnostics["largestOversizedSourceBatchBytes"] = streamingOversizedBatchBytes
    streamDiagnostics["pendingUploadBatches"] = (stagedStreamUpdate?.batches.count ?? 0) - stagedStreamBatches.count
    result["worldStreaming"] = streamDiagnostics
    result["distantBatchNames"] = world.batches.map(\.name).filter { $0.hasPrefix("Distant ") }.sorted()
    result["worldBatchLODSubmissions"] = batches
      .filter { $0.name.hasPrefix("Vegetation tier") }
      .flatMap { batch -> [[String: Any]] in
        let levels = [batch] + batch.lods
        return levels.indices.map { level in
          let instanceCount = batch.selection.counts[level]
          let trianglesPerInstance = levels[level].indexCount / 3
          return [
            "batchName": batch.name,
            "level": level,
            "instanceCount": instanceCount,
            "trianglesPerInstance": trianglesPerInstance,
            "submittedTriangles": instanceCount * trianglesPerInstance,
            "geometricErrorMetres": levels[level].error,
          ]
        }
      }
    result["expedition"] = expeditionSnapshot
    result["gameTreeParameters"] = world.treeSource.parameters
    result["groundHeight"] = sim.groundHeight(position.x, position.z)
    result["seed"] = world.seed
    result["scale"] = podScale
    result["seedPodHeightMetres"] = GardenWorld.podMetres * podScale
    result["scene"] = "garden"
    result["climate"] = climateSnapshot
    result["outdoorAmbientFloor"] = [
      lighting.outdoorAmbientFloor.x, lighting.outdoorAmbientFloor.y,
      lighting.outdoorAmbientFloor.z,
    ]
    if let animal = sim.nearbyAnimal {
      let elevation = sim.elevation(for: animal)
      result["nearbyAnimalElevation"] = [
        "id": animal.id,
        "supportY": elevation.supportY,
        "rootY": elevation.rootY,
        "focusY": elevation.focusY,
      ]
    }
    result["fullDetailTriangles"] =
      batches.reduce(0) { $0 + $1.indexCount / 3 * $1.instanceCount } + pod.indexCount / 3
    return result
  }
  func query(_ p: V3) throws -> [String: Any] {
    let d =
      world.studioShape.value(at: (p - podPosition) / (podScale * GardenWorld.podUnitScale))
      * (podScale * GardenWorld.podUnitScale)
    return [
      "terrainValue": p.y - sim.groundHeight(p.x, p.z), "terrainHeight": sim.groundHeight(p.x, p.z),
      "terrainSemantics": "implicit", "podDistanceBound": d,
      "podPosition": [podPosition.x, podPosition.y, podPosition.z],
    ]
  }
  func save() throws {
    syncExpeditionPlayer()
    try expedition?.save()
  }
  func command(_ action: String, _ c: [String: Any]) throws {
    switch action {
    case "natureControl":
      guard let id = c["controlID"] as? String else { throw RuntimeError.message("Nature control required") }
      _ = try activateControl(id)
    case "addressCreature":
      guard let text = c["text"] as? String else {
        throw RuntimeError.message("Creature interaction text required")
      }
      _ = try submitText(text)
    case "specimen":
      guard let id=c["value"] as? String else {throw RuntimeError.message("Specimen id required")}
      if id == "none" {specimen=nil;return}
      let candidate=try AnimatedAsset(source:AssetSource.load(id),graphics:graphics)
      guard candidate.source.animation?.clips.contains("procession") == true else {throw RuntimeError.message("This specimen has no procession clip")}
      let x=position.x+forward.x*9, z=position.z+forward.z*9
      specimenPlacement=transform(V3(x,world.terrain.height(x,z),z),V3(repeating:0.7),atan2(forward.x,forward.z))
      specimen=candidate; specimenTime=0
    case "scene":
      guard c["value"] as? String == "garden" else {
        throw RuntimeError.message("Use Soundstage for authoring")
      }
    case "reset":
      cancelInterpretation()
      natureSession.reset()
      naturePresentation?.reset()
      naturePublishedSupport = nil
      naturePresentationSupportRevision = 0
      placementSession.reset()
      placementPresentation?.reset()
      placementAimKey = nil
      cachedPlacementAim = nil
      camera = PlayerCamera(position: V3(0, world.terrain.height(0, 24) + 1.72, 24), pitch: -0.04)
      refreshClimatePresentation(force: true)
    case "climateAuto":
      guard let enabled = c["value"] as? Bool else {
        throw RuntimeError.message("Climate automation requires a boolean value")
      }
      climateAutomatic = enabled
      climateKey = nil
      if enabled { refreshClimatePresentation(force: true) }
    case "saveExpedition": try save()
    case "expeditionSlot":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Slot name required")
      }
      try loadExpeditionSlot(name)
    case "camera":
      guard SanctuaryGeography.bounds.contains(SIMD2(position.x, position.z)) else {
        throw RuntimeError.message("Camera is outside Sanctuary's finite world")
      }
      sim.regroundForPresentation()
      syncExpeditionPlayer()
    case "parameters": if let scale = c["scale"] as? NSNumber { podScale = scale.floatValue }
    case "reloadAssets":
      let candidate = try GardenWorld(terrain: sim.world.terrain,
        initialPosition: position, garden: sim.controller.state.garden,
        boulders: sim.controller.state.boulders, construction: sim.controller.state.buildings)
      let uploaded = candidate.batches.map(graphics.upload)
      let presentation = try ExpeditionPresentation(graphics: graphics, terrain: candidate.terrain)
      world.cancelStreaming()
      markedStreamGeneration = nil
      stagedStreamUpdate = nil
      stagedStreamBatches.removeAll(keepingCapacity: true)
      sim.enableHostCommittedRegionalCollision(initialLayout: candidate.layout,
        garden: candidate.publishedGarden, boulders: candidate.publishedBoulders)
      world = candidate
      batches = uploaded
      expeditionPresentation = presentation
    default: throw RuntimeError.message("Unknown Sanctuary action: \(action)")
    }
  }

  private static func climateLightingChanged(from old: GameLighting, to new: GameLighting) -> Bool {
    old.preset != new.preset || old.wetness != new.wetness
      || old.sky.altitude != new.sky.altitude || old.sky.azimuth != new.sky.azimuth
      || old.sky.coverage != new.sky.coverage || old.sky.density != new.sky.density
      || old.sky.haze != new.sky.haze || old.sky.cloudSeed != new.sky.cloudSeed
      || old.outdoorAmbientFloor != new.outdoorAmbientFloor
  }

  private func refreshClimatePresentation(force: Bool = false) {
    let state = sim.controller.state
    let sample = SanctuaryClimate.sample(
      elapsedPlaySeconds: state.age, seed: state.creatureState.seed)
    climateSample = sample
    if let prior = observedClimateElapsed,
      state.age + 1 < prior || state.age > prior + 2
    {
      // A saved-state restore can arrive through GameSimulation without
      // calling this experience. Reapply its coarse presentation on render.
      climateKey = nil
    }
    observedClimateElapsed = state.age
    guard climateAutomatic else { return }

    let key = ClimatePresentationKey(
      day: climateDayState(sample.timeOfDay), weather: climateWeatherState(sample),
      seed: state.creatureState.seed)
    if force || key != climateKey {
      applyingClimate = true
      lighting = climateLighting(for: sample, key: key)
      applyingClimate = false
      climateKey = key
    } else if lighting.wetness != sample.rain
      || lighting.outdoorAmbientFloor != climateOutdoorAmbientFloor(sample.timeOfDay)
    {
      // Wetness and the deterministic night floor are outside the atmosphere
      // cache key, so their slow updates do not restart a cloud/sky candidate.
      applyingClimate = true
      lighting.wetness = sample.rain
      lighting.outdoorAmbientFloor = climateOutdoorAmbientFloor(sample.timeOfDay)
      applyingClimate = false
    }
  }

  private func climateDayState(_ time: Float) -> ClimateDayState {
    switch time {
    case 0.20..<0.27, 0.80..<0.88: return .afterglow
    case 0.27..<0.47: return .morning
    case 0.47..<0.61: return .noon
    case 0.61..<0.70: return .golden
    case 0.70..<0.80: return .sunset
    default: return .night
    }
  }

  private func climateWeatherState(_ sample: SanctuaryClimate.Sample) -> ClimateWeatherState {
    if sample.rain >= 0.18 { return .rain }
    if sample.cloud >= 0.58 { return .overcast }
    return .clear
  }

  private func climateLighting(
    for sample: SanctuaryClimate.Sample, key: ClimatePresentationKey
  ) -> GameLighting {
    let preset: String
    switch key.weather {
    case .rain: preset = "rain"
    case .overcast: preset = "overcast"
    case .clear:
      switch key.day {
      case .morning: preset = "morning"
      case .noon: preset = "noon"
      case .golden: preset = "golden"
      case .sunset: preset = "sunset"
      case .afterglow, .night: preset = "afterglow"
      }
    }
    var result = lighting
    var sky = SkySettings.preset(preset)
    sky.altitude = [
      .morning: Float(36), .noon: 55, .golden: 10, .sunset: 2.5, .afterglow: -1.2, .night: -12,
    ][key.day]!
    switch key.weather {
    case .clear:
      sky.coverage = 0.42
      sky.density = 0.8
      sky.haze = key.day == .night ? 0.55 : 0.7
    case .overcast:
      sky.coverage = 0.85
      sky.density = 1.7
      sky.haze = 1.2
    case .rain:
      sky.coverage = 0.95
      sky.density = 2.4
      sky.haze = 1.5
    }
    sky.cloudSeed = Float(key.seed % 251) / 25 + 1
    result.preset = preset
    result.sky = sky
    result.wetness = sample.rain
    result.outdoorAmbientFloor = climateOutdoorAmbientFloor(sample.timeOfDay)
    return result
  }

  /// Keeps the authored night floor continuous across the visible sunset and
  /// dawn spans. This is a deterministic readability aid, not moon transport,
  /// lunar shadows, or a thermodynamic claim.
  private func climateOutdoorAmbientFloor(_ time: Float) -> V3 {
    let factor: Float
    switch time {
    case 0.76..<0.88:
      factor = climateSmoothstep(0.76, 0.88, time)
    case 0.88...1, 0..<0.16:
      factor = 1
    case 0.16..<0.27:
      factor = 1 - climateSmoothstep(0.16, 0.27, time)
    default:
      factor = 0
    }
    return V3(0.0063, 0.0098, 0.014) * factor
  }

  private func climateSmoothstep(_ low: Float, _ high: Float, _ value: Float) -> Float {
    let t = clamp((value - low) / (high - low), 0, 1)
    return t * t * (3 - 2 * t)
  }
}
