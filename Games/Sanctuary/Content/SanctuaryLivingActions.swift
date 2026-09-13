import FieldCore
import Foundation
import SimulationCore
import simd

extension SanctuaryWorld {
  public static let livingControls: [(id: String, title: String)] = [
    ("flowers", "Nature · Grow flowers"), ("grove", "Nature · Grow a grove"),
    ("reeds", "Nature · Grow reeds"), ("water", "Nature · Invite shallow water"),
    ("raise", "Nature · Raise earth"), ("lower", "Nature · Lower earth"),
    ("smooth", "Nature · Level earth"), ("undoPlanting", "Nature · Undo last change"),
    ("build-cabin", "Build · Cabin"), ("build-path", "Build · Path"),
    ("build-bridge", "Build · Bridge"), ("build-deck", "Build · Observation deck"),
    ("build-bench", "Build · Bench"), ("build-lantern", "Build · Lantern"),
    ("build-fence", "Build · Fence"), ("undoBuilding", "Build · Undo last placement"),
    ("ride", "Companion · Ride together"), ("fly", "Companion · Fly together"),
    ("help-boulder", "Companion · Move this boulder"), ("undoBoulder", "Companion · Undo boulder move"),
    ("dismount", "Companion · Dismount"), ("higher", "Flight · Rise"),
    ("lowerFlight", "Flight · Descend"), ("journal", "Field journal"),
    ("brush-larger", "Tools · Larger brush"), ("brush-smaller", "Tools · Smaller brush"),
    ("spell-stronger", "Tools · Stronger earth shaping"), ("spell-gentler", "Tools · Gentler earth shaping"),
    ("reach-farther", "Tools · Place farther away"), ("reach-nearer", "Tools · Place nearer"),
    ("building-turn", "Tools · Turn construction 45°"),
    ("building-larger", "Tools · Larger construction"), ("building-smaller", "Tools · Smaller construction"),
    ("building-select", "Build · Select nearest construction"),
    ("building-move", "Build · Move selected construction"),
    ("building-resize", "Build · Resize selected to tool size"),
    ("building-remove", "Build · Remove selected construction"),
    ("building-finish", "Build · Finish editing")
  ]

  public func control(_ id: String) throws -> String {
    try SimulationSemanticInput.validateControl(id)
    guard Self.livingControls.contains(where: { $0.id == id }) else {
      throw SimulationFailure.invalid("Unknown Sanctuary action")
    }
    syncExpeditionPlayer()
    let hasSelection = controller.state.buildings.selectedPlacementID != nil
    if id.hasPrefix("brush-") || id.hasPrefix("spell-") || id.hasPrefix("reach-")
      || ["building-larger", "building-smaller"].contains(id)
      || (!hasSelection && id == "building-turn") {
      try controller.editLiving { state in
        var tools = state.craftTools
        switch id {
        case "brush-larger": tools.brushRadius = min(16, tools.brushRadius + 1)
        case "brush-smaller": tools.brushRadius = max(1, tools.brushRadius - 1)
        case "spell-stronger": tools.strength = min(4, tools.strength + 0.25)
        case "spell-gentler": tools.strength = max(0.25, tools.strength - 0.25)
        case "reach-farther": tools.reach = min(16, tools.reach + 1)
        case "reach-nearer": tools.reach = max(3, tools.reach - 1)
        case "building-turn":
          let angle = tools.buildingRotation + .pi / 4
          tools.buildingRotation = atan2(sin(angle), cos(angle))
        case "building-larger": tools.buildingScale = min(3, tools.buildingScale + 0.25)
        case "building-smaller": tools.buildingScale = max(0.5, tools.buildingScale - 0.25)
        default: break
        }
        try tools.validate()
        state.tools = tools
      }
      let tools = controller.state.craftTools
      return String(format: "Brush %.0f m · Reach %.0f m · Construction %.2f× · Turn %.0f°",
        tools.brushRadius, tools.reach, tools.buildingScale, tools.buildingRotation * 180 / .pi)
    }
    if id == "help-boulder" || id == "undoBoulder" { return try boulderControl(id) }
    let tools = controller.state.craftTools
    let direction = SIMD2(sin(yaw), -cos(yaw))
    let target = SIMD2(position.x, position.z) + direction * tools.reach
    let location = HabitatGarden.Location(x: target.x, z: target.y)
    if id == "building-select" {
      let selected = try controller.editLiving { state -> PersonalConstruction.PlacementID in
        var buildings = state.buildings
        guard let placement = nearestPlayerConstruction(
          in: buildings, player: state.player, forward: direction, reach: tools.reach)
        else { throw PersonalConstructionError.noReachablePlacement }
        _ = try buildings.apply(.select(placement.id), expectedRevision: buildings.revision)
        state.construction = buildings
        return placement.id
      }
      return "Selected construction \(selected). Turn, resize, move, remove, or Finish editing."
    }
    if id == "building-finish" {
      try controller.editLiving { state in
        var buildings = state.buildings
        _ = try buildings.apply(.deselect, expectedRevision: buildings.revision)
        state.construction = buildings
      }
      return "Finished editing construction. Turn and size now set the next placement."
    }
    if ["building-move", "building-resize", "building-remove", "building-turn"].contains(id) {
      let buildings = controller.state.buildings
      guard let selectedID = buildings.selectedPlacementID,
        let selected = buildings.placement(id: selectedID)
      else { throw PersonalConstructionError.noSelectedPlacement }
      let command: PersonalConstruction.Command
      switch id {
      case "building-move":
        command = .update(
          selectedID, at: .init(x: target.x, y: groundHeight(target.x, target.y), z: target.y),
          yawRadians: selected.yawRadians, scale: selected.scale)
      case "building-resize":
        command = .update(
          selectedID, at: selected.location, yawRadians: selected.yawRadians, scale: tools.buildingScale)
      case "building-turn":
        let angle = selected.yawRadians + .pi / 4
        command = .update(
          selectedID, at: selected.location, yawRadians: atan2(sin(angle), cos(angle)),
          scale: selected.scale)
      case "building-remove":
        command = .remove(selectedID)
      default:
        throw SimulationFailure.invalid("Unknown construction edit")
      }
      _ = try commitLegacyPlacement(command, expectedRevision: buildings.revision)
      switch id {
      case "building-move": return "Moved selected construction."
      case "building-resize": return "Resized selected construction."
      case "building-remove": return "Removed selected construction. Undo restores it."
      case "building-turn": return "Turned selected construction 45°."
      default: return ""
      }
    }
    let nature = natureCommand(
      for: id, at: location, tools: tools, terrainTargetHeight: groundHeight(target.x, target.y))
    if let nature {
      try commitNature(nature, expectedGardenRevision: controller.state.garden?.revision ?? 0)
      return ""
    }
    if id.hasPrefix("build-") {
      guard let primitive = PersonalConstruction.Primitive(rawValue: String(id.dropFirst(6))) else {
        throw SimulationFailure.invalid("Unknown construction")
      }
      let angle = atan2(sin(yaw + tools.buildingRotation), cos(yaw + tools.buildingRotation))
      let base = groundHeight(target.x, target.y)
      _ = try commitLegacyPlacement(
        .place(primitive, at: .init(x: target.x, y: base, z: target.y),
          yawRadians: angle, scale: tools.buildingScale),
        expectedRevision: controller.state.buildings.revision)
      return ""
    }
    if id == "undoBuilding" {
      _ = try commitLegacyPlacement(.undo, expectedRevision: controller.state.buildings.revision)
      return ""
    }
    if id == "journal" { return fieldJournalText }
    return try companionControl(id)
  }

  public func localWaterHeight(_ x: Float, _ z: Float) -> Float? {
    guard regionalCollisionPublicationPolicy == .hostCommitted else {
      return localWaterHeight(x, z, state: controller.state)
    }
    let point = SIMD2(x, z)
    var height = world.terrain.resolvedWaterField(at: point, garden: presentationGarden)?.surfaceHeight
    for water in controller.state.population.ecology.waterFacts
      where distance(water.center, point) <= water.upstreamPoolRadius {
      let pool = (presentationGarden ?? HabitatGarden()).surfaceHeight(
        baseHeight: world.terrain.height(water.center.x, water.center.y),
        at: .init(x: water.center.x, z: water.center.y)) + water.waterLevelRise
      height = max(height ?? pool, pool)
    }
    return height
  }

  /// Current direct controls and transient brush intents construct identical
  /// saved garden commands. Smooth receives the already-composed support height.
  func natureCommand(
    for id: String, at location: HabitatGarden.Location, tools: SanctuaryToolSettings,
    terrainTargetHeight: Float
  ) -> HabitatGarden.Command? {
    switch id {
    case "flowers": return .plant(.flowers, at: location, radius: tools.brushRadius)
    case "grove": return .plant(.grove, at: location, radius: min(16, tools.brushRadius + 1))
    case "reeds": return .plant(.reeds, at: location, radius: tools.brushRadius)
    case "water": return .plant(.shallowWater, at: location, radius: min(16, tools.brushRadius + 1))
    case "raise": return .sculpt(.raise, at: location, radius: min(16, tools.brushRadius * 2),
      amount: tools.strength, targetHeight: nil)
    case "lower": return .sculpt(.lower, at: location, radius: min(16, tools.brushRadius * 2),
      amount: tools.strength, targetHeight: nil)
    case "smooth": return .sculpt(.smooth, at: location, radius: min(16, tools.brushRadius * 2),
      amount: min(1, tools.strength), targetHeight: terrainTargetHeight)
    case "undoPlanting": return .undo
    default: return nil
    }
  }

  /// Persists one validated garden command. Optional source revisions let a
  /// transient target reject construction or ecology changes before a write.
  @discardableResult
  func commitNature(
    _ command: HabitatGarden.Command, expectedGardenRevision: UInt64,
    expectedConstructionRevision: UInt64? = nil, expectedEcologyRevision: UInt64? = nil
  ) throws -> HabitatGarden.PatchID {
    syncExpeditionPlayer()
    let id = try controller.editLiving { state -> HabitatGarden.PatchID in
      guard (state.garden?.revision ?? 0) == expectedGardenRevision,
        expectedConstructionRevision.map({ state.buildings.revision == $0 }) ?? true,
        expectedEcologyRevision.map({ state.population.ecology.revision == $0 }) ?? true
      else { throw HabitatGardenError.staleRevision }
      let id = try state.applyNature(command, expectedRevision: expectedGardenRevision)
      state.playerElevation = resolvedEyeHeight(state: state)
      return id
    }
    regroundPlayer()
    return id
  }

  /// Restore and live flight use the same saved, source-owned pool geometry.
  func localWaterHeight(_ x: Float, _ z: Float, state: Expedition) -> Float? {
    let point = SIMD2(x, z)
    var height = world.terrain.resolvedWaterField(at: point, garden: state.garden)?.surfaceHeight
    for water in state.population.ecology.waterFacts
      where distance(water.center, point) <= water.upstreamPoolRadius {
      let pool = (state.garden ?? HabitatGarden()).surfaceHeight(
        baseHeight: world.terrain.height(water.center.x, water.center.y),
        at: .init(x: water.center.x, z: water.center.y)) + water.waterLevelRise
      height = max(height ?? pool, pool)
    }
    return height
  }

  public func flightSurfaceHeight(_ x: Float, _ z: Float) -> Float {
    let ground = groundHeight(x, z)
    return max(ground, localWaterHeight(x, z) ?? ground)
  }

  /// Finds only player-created construction that is in front of the player. The
  /// edge distance includes a capped footprint allowance, so a large bridge is
  /// still practical to select without becoming a remote-selection tool.
  func nearestPlayerConstruction(
    in construction: PersonalConstruction, player: SIMD2<Float>, forward: SIMD2<Float>, reach: Float
  ) -> PersonalConstruction.Placement? {
    var nearest: PersonalConstruction.Placement?
    var nearestEdgeDistance = Float.greatestFiniteMagnitude
    for placement in construction.placements {
      let delta = SIMD2<Float>(placement.location.x - player.x, placement.location.z - player.y)
      let centerDistance: Float = length(delta)
      let extents: SIMD3<Float> = placement.primitive.halfExtents
      let footprintRadius: Float = min(
        4, sqrt(extents.x * extents.x + extents.z * extents.z) * placement.scale)
      let edgeDistance: Float = max(0, centerDistance - footprintRadius)
      guard dot(delta, forward) >= 0, edgeDistance <= reach else { continue }
      let winsDistance = edgeDistance < nearestEdgeDistance
      let winsTie: Bool
      if let nearest { winsTie = edgeDistance == nearestEdgeDistance && placement.id < nearest.id }
      else { winsTie = edgeDistance == nearestEdgeDistance }
      if winsDistance || winsTie {
        nearest = placement
        nearestEdgeDistance = edgeDistance
      }
    }
    return nearest
  }

  func ecologyBlocks(from origin: SIMD2<Float>, to target: SIMD2<Float>) -> Bool {
    controller.state.population.ecology.collisionFacts.contains { fact in
      let oldDistance = distance(origin, fact.center), newDistance = distance(target, fact.center)
      return newDistance < fact.radius + 0.28 && newDistance <= oldDistance
    }
  }

  public func regroundForPresentation() { regroundPlayer() }

  /// Computes the eye against a candidate state before it is persisted. This
  /// keeps edit/undo saves and the next displayed frame on the same surface.
  func resolvedEyeHeight(state: Expedition) -> Float {
    let point = state.player
    let travel = state.travel
    let eye: Float = travel.mode == .flying ? travel.flightHeight : travel.mode == .riding ? 2.5 : 1.72
    var ground = (state.garden ?? HabitatGarden()).surfaceHeight(
      baseHeight: world.terrain.height(point.x, point.y), at: .init(x: point.x, z: point.y))
    if travel.mode == .flying {
      ground = max(ground, localWaterHeight(point.x, point.y, state: state) ?? ground)
    } else {
      ground = standingTerrainHeight(at: point, state: state)
      let feet = (state.playerElevation ?? position.y) - eye
      for fact in state.buildings.collisionFacts + SanctuaryHome.construction.collisionFacts
        where fact.kind == .walkable && fact.top <= feet + 0.5 {
        if fact.contains(.init(x: point.x, y: fact.top, z: point.y)) { ground = max(ground, fact.top) }
      }
    }
    return ground + eye
  }

  func regroundPlayer() {
    if regionalCollisionPublicationPolicy == .hostCommitted {
      position.y = publishedEyeHeight()
      return
    }
    syncExpeditionPlayer()
    position.y = resolvedEyeHeight(state: controller.state)
    syncExpeditionPlayer()
  }

  /// Shared source-derived oriented construction collision. Walkable pieces supply
  /// a top surface only when reachable from the player's current feet.
  var constructionFacts: [PersonalConstruction.CollisionFact] {
    controller.state.buildings.collisionFacts + SanctuaryHome.construction.collisionFacts
  }

  func constructionGround(at p: V3, terrain: Float) -> Float {
    let feet = position.y - (controller.state.travel.mode == .riding ? 2.5 : 1.72)
    var ground = terrain
    for fact in constructionFacts where fact.kind == .walkable {
      let probe = PersonalConstruction.Location(x: p.x, y: fact.top, z: p.z)
      if fact.contains(probe), fact.top <= feet + 0.5 { ground = max(ground, fact.top) }
    }
    return ground
  }

  func constructionBlocks(_ p: V3, ground: Float) -> Bool {
    constructionFacts.contains { fact in
      guard fact.kind == .solid else { return false }
      return fact.intersects(
        at: .init(x: p.x, y: ground, z: p.z), from: ground + 0.1, through: ground + 1.55)
    }
  }
}
