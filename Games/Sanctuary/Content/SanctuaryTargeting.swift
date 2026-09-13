import FieldCore
import Foundation
import simd

public struct SanctuaryTargetSupport: Equatable, Sendable {
  public let point: V3
  public let normal: V3
}

public struct SanctuaryTargetingResult: Equatable, Sendable {
  public enum HitKind: String, Sendable { case ground, walkableConstruction, construction, solid, none }
  public enum Obstruction: String, Sendable {
    case none, solid, rangeLimit, outsideWorld, invalidInput, queryBudget
  }
  /// The nearest ray contact, or a finite ray endpoint for an invalid preview.
  public let point: V3
  /// Only composed ground and a walkable construction's upward-facing top support placement.
  public let support: SanctuaryTargetSupport?
  public let hitKind: HitKind
  public let obstruction: Obstruction
  public let distance: Float
  public let maxReach: Float
  public let solidName: String?
  public let placementID: PersonalConstruction.PlacementID?
}

extension SanctuaryWorld {
  /// Pure query of current production sources. Callers may cache by camera/support revisions.
  /// Water does not hide the bed; intent validation decides whether a bridge/deck is suitable.
  public func cameraTarget(maxReach: Float,
    ignoringPlacementID: PersonalConstruction.PlacementID? = nil
  ) -> SanctuaryTargetingResult {
    target(origin: camera.position, direction: camera.forward, maxReach: maxReach,
      ignoringPlacementID: ignoringPlacementID)
  }

  /// Direction may have any finite nonzero length; reach and returned distance are metres.
  /// Ignoring a selected placement excludes only user construction, never the authored home.
  public func target(origin rawOrigin: V3, direction rawDirection: V3, maxReach: Float,
    ignoringPlacementID: PersonalConstruction.PlacementID? = nil
  ) -> SanctuaryTargetingResult {
    typealias Result = SanctuaryTargetingResult
    let origin = TargetRay.finite(rawOrigin) ? rawOrigin : .zero
    func result(_ t: Float, _ kind: Result.HitKind, _ obstruction: Result.Obstruction,
      support: SanctuaryTargetSupport? = nil, name: String? = nil,
      placementID: PersonalConstruction.PlacementID? = nil, direction: V3 = .zero) -> Result {
      Result(point: origin + direction * t, support: support, hitKind: kind,
        obstruction: obstruction, distance: t,
        maxReach: maxReach.isFinite ? max(0, maxReach) : 0,
        solidName: name, placementID: placementID)
    }
    guard TargetRay.finite(rawOrigin), TargetRay.finite(rawDirection),
      maxReach.isFinite, (3...16).contains(maxReach),
      SanctuaryGeography.bounds.contains(SIMD2(origin.x, origin.z)) else {
      return result(0, .none, .invalidInput)
    }
    let scale = max(abs(rawDirection.x), max(abs(rawDirection.y), abs(rawDirection.z)))
    guard scale > 0 else { return result(0, .none, .invalidInput) }
    let scaledDirection = rawDirection / scale
    let direction = scaledDirection / length(scaledDirection)
    let ray = TargetRay(origin: origin, direction: direction)
    let bounds = SanctuaryGeography.bounds
    let worldRange = ray.box(Bounds(V3(bounds.minimum.x, -Float.greatestFiniteMagnitude, bounds.minimum.y),
      V3(bounds.maximum.x, Float.greatestFiniteMagnitude, bounds.maximum.y)), limit: maxReach)
    let end = worldRange?.exit ?? 0
    var best = result(end, .none, end < maxReach ? .outsideWorld : .rangeLimit, direction: direction)
    var budget = 4096

    // Exact OBB slabs catch even thin fences and preserve doorway gaps between source facts.
    let facts = controller.state.buildings.collisionFacts.filter {
      $0.placementID != ignoringPlacementID
    } + SanctuaryHome.construction.collisionFacts
    for fact in facts {
      let c = cos(fact.yawRadians), s = sin(fact.yawRadians)
      func local(_ p: V3) -> V3 { V3(p.x * c - p.z * s, p.y, p.x * s + p.z * c) }
      let center = V3(fact.center.x, fact.center.y, fact.center.z)
      let localRay = TargetRay(origin: local(origin - center), direction: local(direction))
      let box = Bounds(V3(-fact.halfExtents.x, fact.bottom - center.y, -fact.halfExtents.z),
        V3(fact.halfExtents.x, fact.top - center.y, fact.halfExtents.z))
      guard let hit = localRay.box(box, limit: best.distance), hit.entry <= best.distance else { continue }
      let p = origin + direction * hit.entry
      let top = fact.kind == .walkable && hit.normal.y > 0.5
      best = result(hit.entry, top ? .walkableConstruction : .construction, top ? .none : .solid,
        support: top ? SanctuaryTargetSupport(point: p, normal: V3(0, 1, 0)) : nil,
        placementID: fact.placementID, direction: direction)
    }

    // Grid candidates seed the list; a bounded complete bounds scan also includes cells touched
    // only at a ray corner. SpatialBoundsGrid has no segment traversal contract to rely on.
    let solids = world.solids + [Solid(shape: SanctuaryLayout.podShape, position: podPosition,
      scale: podScale * SanctuaryLayout.podUnitScale, name: "Seed pod")]
    var candidates = Set(world.collisionGrid.candidates(at: origin))
    candidates.formUnion(world.collisionGrid.candidates(at: origin + direction * end))
    for (index, solid) in solids.enumerated() {
      let b = solid.shape.bounds
      let worldBounds = Bounds(b.min * solid.scale + solid.position, b.max * solid.scale + solid.position)
      if ray.box(worldBounds, limit: best.distance) != nil { candidates.insert(index) }
    }
    for index in candidates.sorted() where solids.indices.contains(index) {
      let solid = solids[index]
      let b = solid.shape.bounds
      guard solid.scale.isFinite, solid.scale > 0,
        let span = ray.box(Bounds(b.min * solid.scale + solid.position,
          b.max * solid.scale + solid.position), limit: best.distance) else { continue }
      let contact = ray.firstContact(from: span.entry, to: span.exit, budget: &budget,
        value: { solid.value($0) }, lower: { a, b in
          let lo = (simd_min(a, b) - solid.position) / solid.scale
          let hi = (simd_max(a, b) - solid.position) / solid.scale
          return solid.shape.valueInterval(in: Bounds(lo, hi)).lower * solid.scale
        })
      if let contact, contact.distance <= best.distance {
        best = result(contact.distance, .solid, contact.confirmed ? .solid : .queryBudget,
          name: solid.name, direction: direction)
      }
    }

    let terrain = world.terrain
    let edits = controller.state.garden?.orderedTerrainPatches ?? []
    let contact = ray.firstContact(from: 0, to: best.distance, budget: &budget,
      value: { p in p.y - groundHeight(p.x, p.z) }, lower: { a, b in
        let lo = simd_min(a, b), hi = simd_max(a, b), middle = (a + b) * 0.5
        let certificate = terrain.patchCertificate(x: lo.x...hi.x, z: lo.z...hi.z)
        let h = terrain.height(middle.x, middle.z)
        let variation = certificate.dx * (hi.x - lo.x) * 0.5
          + certificate.dz * (hi.z - lo.z) * 0.5 + 0.001
        let base = TargetInterval(max(certificate.height.lowerBound, h - variation),
          min(certificate.height.upperBound, h + variation))
        var height = base
        for patch in edits {
          let center = SIMD2(patch.center.x, patch.center.z)
          let lower = SIMD2(lo.x, lo.z) - center, upper = SIMD2(hi.x, hi.z) - center
          let near = simd_max(simd_max(lower, -upper), .zero)
          let far = simd_max(abs(lower), abs(upper))
          func influence(_ squaredDistance: Float) -> Float {
            let n = max(0, 1 - squaredDistance / (patch.radius * patch.radius))
            return n * n * (3 - 2 * n)
          }
          let influenceRange = TargetInterval(influence(length_squared(far)), influence(length_squared(near)))
          let amount = influenceRange * TargetInterval(patch.amount, patch.amount)
          switch patch.operation {
          case .raise: height = height + amount
          case .lower: height = height - amount
          case .smooth:
            if let target = patch.targetHeight {
              height = height + (TargetInterval(target, target) - height) * amount
            }
          }
          let cap = TargetInterval(HabitatGarden.maximumTerrainOffset, HabitatGarden.maximumTerrainOffset)
          height = height.clamped(lower: base - cap, upper: base + cap)
        }
        return lo.y - height.high - 0.001
      })
    if let contact, contact.distance <= best.distance {
      if contact.confirmed {
        var p = origin + direction * contact.distance
        let inside = p.y < groundHeight(p.x, p.z) - TargetRay.tolerance
        p.y = groundHeight(p.x, p.z)
        let e: Float = 0.02
        let normal = normalize(V3(groundHeight(p.x - e, p.z) - groundHeight(p.x + e, p.z),
          2 * e, groundHeight(p.x, p.z - e) - groundHeight(p.x, p.z + e)))
        best = result(contact.distance, .ground, inside ? .solid : .none,
          support: inside ? nil : SanctuaryTargetSupport(point: p, normal: normal), direction: direction)
      } else {
        best = result(contact.distance, .ground, .queryBudget, direction: direction)
      }
    }
    return best
  }
}

private struct TargetInterval {
  let low: Float
  let high: Float
  init(_ low: Float, _ high: Float) { self.low = low; self.high = high }
  static func + (a: Self, b: Self) -> Self { Self(a.low + b.low, a.high + b.high) }
  static func - (a: Self, b: Self) -> Self { Self(a.low - b.high, a.high - b.low) }
  static func * (a: Self, b: Self) -> Self {
    let products = [a.low * b.low, a.low * b.high, a.high * b.low, a.high * b.high]
    return Self(products.min()!, products.max()!)
  }
  func clamped(lower: Self, upper: Self) -> Self {
    Self(min(upper.low, max(lower.low, low)), min(upper.high, max(lower.high, high)))
  }
}

private struct TargetRay {
  static let tolerance: Float = 0.01
  let origin: V3
  let direction: V3
  static func finite(_ p: V3) -> Bool { p.x.isFinite && p.y.isFinite && p.z.isFinite }

  func box(_ bounds: Bounds, limit: Float) -> (entry: Float, exit: Float, normal: V3)? {
    var entry: Float = 0, exit = limit, normal = V3.zero
    for axis in 0..<3 {
      let d = direction[axis]
      if abs(d) < 0.0000001 {
        if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] { return nil }
        continue
      }
      let a = (bounds.min[axis] - origin[axis]) / d
      let b = (bounds.max[axis] - origin[axis]) / d
      let near = min(a, b), far = max(a, b)
      if near > entry {
        entry = near; normal = .zero; normal[axis] = d > 0 ? -1 : 1
      }
      exit = min(exit, far)
      if entry > exit { return nil }
    }
    return (entry, exit, normal)
  }

  /// Front-to-back interval exclusion, valid for general implicit fields too. A possible thin
  /// hit can never be skipped by a fixed march step. Unresolved leaves/budget fail closed.
  func firstContact(from start: Float, to end: Float, budget: inout Int,
    value: (V3) -> Float, lower: (V3, V3) -> Float
  ) -> (distance: Float, confirmed: Bool)? {
    var stack: [(Float, Float)] = [(start, end)]
    while let (a, b) = stack.popLast() {
      guard budget > 0 else { return (a, false) }
      budget -= 1
      let pa = origin + direction * a, pb = origin + direction * b
      let bound = lower(pa, pb)
      guard bound.isFinite else { return (a, false) }
      if bound > 0 { continue }
      let va = value(pa)
      guard va.isFinite else { return (a, false) }
      if va <= Self.tolerance { return (a, true) }
      if b - a <= 0.00005 { return (a, false) }
      let middle = (a + b) * 0.5
      stack.append((middle, b)); stack.append((a, middle))
    }
    return nil
  }
}
