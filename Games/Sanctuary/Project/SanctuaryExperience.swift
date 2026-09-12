import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

final class SanctuaryExperience: GameExperience {
  let graphics: MetalRenderer
  var world: GardenWorld
  var batches: [GPUBatch]
  var pod: GPUBatch
  var sim: SanctuaryWorld
  var simulation: any GameSimulation { sim }
  var podScale: Float {
    get { sim.podScale }
    set { sim.podScale = newValue }
  }
  var expedition: ExpeditionController? { sim.controller }
  var expeditionPresentation: ExpeditionPresentation?
  var camera: PlayerCamera {
    get { sim.camera }
    set { sim.camera = newValue }
  }
  var lighting = GameLighting()
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
    world = try GardenWorld()
    batches = world.batches.map(graphics.upload)
    pod = graphics.upload(
      SceneBatch(name: "Seed pod", mesh: world.studioMesh, instances: [Instance()]))
    sim = try SanctuaryWorld(root: ProjectContext.dataRoot, parameters: world.treeSource.parameters)
    expeditionPresentation = try ExpeditionPresentation(graphics: graphics, terrain: world.terrain)
  }
  var podPosition: V3 {
    V3(
      5,
      world.terrain.height(5, 12) - world.studioShape.bounds.min.y * podScale
        * GardenWorld.podUnitScale, 12)
  }

  func move(_ local: V3) { sim.move(local) }
  func step(_ seconds: Float, running: Bool) {
    sim.motion = SanctuaryProject.motion(
      expeditionPresentation?.source.motion ?? MotionParameters())
    sim.advance(seconds, running: running)
    if specimen != nil { specimenTime += seconds }
  }
  func syncExpeditionPlayer() { sim.syncExpeditionPlayer() }
  func interact() throws -> String { try sim.interact() }
  func loadExpeditionSlot(_ name: String) throws {
    try sim.loadExpeditionSlot(name)
    keys.removeAll()
  }
  func items() -> [RenderItem] {
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
      result += expeditionPresentation.items(expedition.state)
    }
    if let specimen {
      result += specimen.items(clip:"procession",time:specimenTime,placement:specimenPlacement,terrain:world.terrain.height)
    }
    return result
  }
  var hud: GameHUD {
    guard let state = expedition?.state else {
      return GameHUD(title: "Sanctuary", objective: "Explore", detail: "", prompt: nil)
    }
    let detail: String
    if state.phase == .settled {
      detail = String(format: "Frost garden · %.0f%% established · 1 resident", state.habitat * 100)
    } else if state.phase == .carrying {
      let d = Expedition.home - state.player
      let direction =
        abs(d.y) > abs(d.x) ? (d.y > 0 ? "south" : "north") : (d.x > 0 ? "east" : "west")
      detail = String(format: "Frostling safe in your care · Home %.0f m %@", length(d), direction)
    } else {
      detail =
        distance(state.player, state.creaturePosition) < 9
        ? String(format: "Frostling · Trust %.0f%% · Move gently", state.trust * 100)
        : "Field journal · \(state.discoveredSigns.count) of 3 frost traces examined"
    }
    return GameHUD(
      title: "The first resident", objective: state.objective, detail: detail, prompt: state.prompt)
  }
  var snapshot: [String: Any] {
    [
      "specimen":specimen?.source.id ?? "", "specimenTime":specimenTime,
      "expedition": expedition?.snapshot ?? [:], "gameTreeParameters": world.treeSource.parameters,
      "groundHeight": world.terrain.height(position.x, position.z), "seed": world.seed,
      "scale": podScale, "seedPodHeightMetres": GardenWorld.podMetres * podScale, "scene": "garden",
      "fullDetailTriangles": batches.reduce(0) { $0 + $1.indexCount / 3 * $1.instanceCount } + pod
        .indexCount / 3,
    ]
  }
  func query(_ p: V3) throws -> [String: Any] {
    let d =
      world.studioShape.value(at: (p - podPosition) / (podScale * GardenWorld.podUnitScale))
      * (podScale * GardenWorld.podUnitScale)
    return [
      "terrainValue": world.terrain.value(at: p), "terrainHeight": world.terrain.height(p.x, p.z),
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
      camera = PlayerCamera(position: V3(0, world.terrain.height(0, 24) + 1.72, 24), pitch: -0.04)
    case "saveExpedition": try save()
    case "expeditionSlot":
      guard let name = c["name"] as? String else {
        throw RuntimeError.message("Slot name required")
      }
      try loadExpeditionSlot(name)
    case "camera":
      position.y = world.terrain.height(position.x, position.z) + 1.72
      syncExpeditionPlayer()
    case "parameters": if let scale = c["scale"] as? NSNumber { podScale = scale.floatValue }
    case "reloadAssets":
      let candidate = try GardenWorld()
      let uploaded = candidate.batches.map(graphics.upload)
      let presentation = try ExpeditionPresentation(graphics: graphics, terrain: candidate.terrain)
      sim.world = candidate.layout
      world = candidate
      batches = uploaded
      expeditionPresentation = presentation
    default: throw RuntimeError.message("Unknown Sanctuary action: \(action)")
    }
  }
}
