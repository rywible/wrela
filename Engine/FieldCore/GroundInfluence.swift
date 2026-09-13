import Foundation
import simd

public enum GroundInfluenceKind: String, Codable, Hashable, Sendable {
  case body, foot
}

/// Source-owned contact with a supporting surface. Positions/radii/displacement
/// are metres. Times are saved simulation seconds, never wall-clock timestamps.
/// Heading is a horizontal direction; compression is a bounded artistic response,
/// not a force or a claim of pressure-calibrated plant/soil mechanics.
public struct GroundInfluenceEvent: Codable, Hashable, Sendable {
  public var id: UInt64
  public var sourceID: String
  public var kind: GroundInfluenceKind
  public var start, end: SIMD3<Float>
  public var radius: Float
  public var displacement: Float
  public var compression: Float
  public var supportTolerance: Float
  public var heading: SIMD2<Float>
  public var startTime, endTime: Double
  public var recoverySeconds: Float

  public init(id: UInt64, sourceID: String, kind: GroundInfluenceKind,
    start: SIMD3<Float>, end: SIMD3<Float>, radius: Float,
    displacement: Float, compression: Float, supportTolerance: Float = 0.45,
    heading: SIMD2<Float> = SIMD2(0, -1), startTime: Double, endTime: Double,
    recoverySeconds: Float)
  {
    self.id = id; self.sourceID = sourceID; self.kind = kind
    self.start = start; self.end = end; self.radius = radius
    self.displacement = displacement; self.compression = compression
    self.supportTolerance = supportTolerance; self.heading = heading
    self.startTime = startTime; self.endTime = endTime; self.recoverySeconds = recoverySeconds
  }

  public func validate() throws {
    guard !sourceID.isEmpty, sourceID.utf8.count <= 128,
      [start.x, start.y, start.z, end.x, end.y, end.z, heading.x, heading.y].allSatisfy(\.isFinite),
      abs(start.x) <= 1_000_000, abs(start.y) <= 1_000_000, abs(start.z) <= 1_000_000,
      abs(end.x) <= 1_000_000, abs(end.y) <= 1_000_000, abs(end.z) <= 1_000_000,
      length(end - start) <= 8, (0.000001...1_000_000).contains(length_squared(heading)),
      (0.05...2).contains(radius), (0...1.5).contains(displacement),
      (0...1).contains(compression), (0.05...2).contains(supportTolerance),
      (0.1...120).contains(recoverySeconds),
      startTime.isFinite, endTime.isFinite, startTime >= 0,
      endTime >= startTime, endTime <= 1_000_000_000
    else { throw GroundInfluenceError.invalidEvent }
  }

  /// Critical-damping-shaped recovery has zero slope on contact release.
  /// The compact tail reaches exactly zero, so bounded event histories may
  /// remove old events without leaving a permanent residual field.
  public func response(at time: Double) -> Float {
    guard time >= startTime else { return 0 }
    let age = max(0, time - endTime) / Double(recoverySeconds)
    guard age < 8 else { return 0 }
    let tail = min(1, max(0, (8 - age) / 2))
    return Float((1 + age) * exp(-age) * tail * tail * (3 - 2 * tail))
  }
}

public enum GroundInfluenceError: Error, Equatable {
  case invalidEvent, invalidSnapshot, invalidCenter
}

/// The game owns this bounded history and its time. A renderer may derive a
/// local grid from it but must not create contacts or advance the event clock.
public struct SurfaceInfluenceSnapshot: Codable, Hashable, Sendable {
  public static let maximumEvents = 256
  public var time: Double
  public var events: [GroundInfluenceEvent]
  public init(time: Double, events: [GroundInfluenceEvent]) {
    self.time = time; self.events = events
  }
  public static let empty = SurfaceInfluenceSnapshot(time: 0, events: [])
  public func validate() throws {
    guard time.isFinite, (0...1_000_000_000).contains(time),
      events.count <= Self.maximumEvents, Set(events.map(\.id)).count == events.count
    else { throw GroundInfluenceError.invalidSnapshot }
    for event in events {
      try event.validate()
      guard event.endTime <= time else { throw GroundInfluenceError.invalidSnapshot }
    }
  }
}

/// Two aligned float4s, shared verbatim with the Metal contact-field reader.
/// bend = horizontal displacement metres, compression, activation.
/// support = world support height, permitted height difference, dominant weight, reserved.
public struct GroundInfluenceCell: Equatable, Sendable {
  public var bend: SIMD4<Float> = .zero
  public var support: SIMD4<Float> = .zero
  public init() {}
}

/// Bounded derived representation of source contacts, not additional simulation
/// state. A single support layer per cell prevents unrelated elevations from
/// blending; when layers conflict the stronger contact wins deterministically.
public struct GroundInfluenceField: Sendable {
  public static let size = 128
  public static let cellMetres: Float = 0.25
  public static let maximumVertexDisplacement: Float = 1.5
  public private(set) var origin = SIMD2<Float>.zero
  public private(set) var cells = Array(repeating: GroundInfluenceCell(), count: GroundInfluenceField.size * GroundInfluenceField.size)
  public private(set) var activeCellCount = 0
  public private(set) var visitedCells = 0
  public init() {}

  public static func origin(around center: SIMD2<Float>) -> SIMD2<Float> {
    SIMD2(floor(center.x / cellMetres), floor(center.y / cellMetres)) * cellMetres
      - SIMD2(repeating: Float(size) * cellMetres * 0.5)
  }

  /// Validation precedes mutation: malformed replay input cannot destroy the
  /// previous derived field. Every event covers at most a bounded 8m sweep.
  public mutating func update(_ snapshot: SurfaceInfluenceSnapshot, center: SIMD2<Float>) throws {
    try snapshot.validate()
    guard center.x.isFinite, center.y.isFinite, abs(center.x) <= 1_000_000,
      abs(center.y) <= 1_000_000 else { throw GroundInfluenceError.invalidCenter }
    origin = Self.origin(around: center)
    for i in cells.indices { cells[i] = GroundInfluenceCell() }
    visitedCells = 0
    for event in snapshot.events.sorted(by: { $0.id < $1.id }) {
      let response = event.response(at: snapshot.time)
      guard response > 0, event.displacement > 0 || event.compression > 0 else { continue }
      let a = SIMD2(event.start.x, event.start.z), b = SIMD2(event.end.x, event.end.z)
      // Prefilter sub-cell contacts so a narrow foot cannot disappear between
      // sample centers. The authored event radius itself remains unchanged.
      let radius = event.radius + Self.cellMetres * 0.70710678
      let low = (simd_min(a, b) - SIMD2(repeating: radius) - origin) / Self.cellMetres
      let high = (simd_max(a, b) + SIMD2(repeating: radius) - origin) / Self.cellMetres
      let x0 = max(0, Int(floor(low.x))), z0 = max(0, Int(floor(low.y)))
      let x1 = min(Self.size - 1, Int(ceil(high.x))), z1 = min(Self.size - 1, Int(ceil(high.y)))
      guard x0 <= x1, z0 <= z1 else { continue }
      let segment = b - a, segmentLength = length_squared(segment)
      let heading = normalize(event.heading)
      for z in z0...z1 { for x in x0...x1 {
        visitedCells += 1
        let p = origin + (SIMD2(Float(x), Float(z)) + SIMD2(repeating: 0.5)) * Self.cellMetres
        let t = segmentLength > 0.000001 ? clamp(dot(p - a, segment) / segmentLength, 0, 1) : 0
        let delta = p - (a + segment * t)
        let squared = length_squared(delta) / (radius * radius)
        guard squared < 1 else { continue }
        // C1 compact spatial support and a two-metre outer-window fade.
        let edge = min(min(Float(x) + 0.5, Float(z) + 0.5),
          min(Float(Self.size - x) - 0.5, Float(Self.size - z) - 0.5)) * Self.cellMetres
        let edgeWeight = clamp(edge / 2, 0, 1)
        let weight = (1 - squared) * (1 - squared) * response
          * edgeWeight * edgeWeight * (3 - 2 * edgeWeight)
        guard weight > 0 else { continue }
        let dominance = weight * max(event.compression, event.displacement / 1.5)
        let height = event.start.y + (event.end.y - event.start.y) * t
        let index = z * Self.size + x
        var cell = cells[index]
        if cell.bend.w > 0,
          abs(cell.support.x - height) > min(cell.support.y, event.supportTolerance) {
          guard dominance > cell.support.z else { continue }
          cell = GroundInfluenceCell()
        }
        let radial = length_squared(delta) > 0.000001 ? normalize(delta) : heading
        let direction = normalize(radial + heading * 0.35)
        let amount = max(event.displacement, event.compression * 0.06) * weight
        var bend = SIMD2(cell.bend.x, cell.bend.y) + direction * amount
        if length_squared(bend) > 1.5 * 1.5 { bend = normalize(bend) * 1.5 }
        cell.bend = SIMD4(bend.x, bend.y, max(cell.bend.z, event.compression * weight),
          max(cell.bend.w, weight))
        if dominance >= cell.support.z { cell.support = SIMD4(height, event.supportTolerance, dominance, 0) }
        cells[index] = cell
      } }
    }
    activeCellCount = cells.reduce(0) { $0 + ($1.bend.w > 0 ? 1 : 0) }
  }

  /// The height gate is applied separately to each bilinear tap; averaging
  /// support heights first would spuriously bend grass between two floors.
  public func sample(at root: SIMD3<Float>) -> SIMD3<Float> {
    let p = (SIMD2(root.x, root.z) - origin) / Self.cellMetres - SIMD2(repeating: 0.5)
    let x = Int(floor(p.x)), z = Int(floor(p.y)), f = p - SIMD2(Float(x), Float(z))
    var result = SIMD3<Float>.zero
    for dz in 0...1 { for dx in 0...1 {
      let ix = x + dx, iz = z + dz
      guard ix >= 0, iz >= 0, ix < Self.size, iz < Self.size else { continue }
      let c = cells[iz * Self.size + ix]
      guard c.bend.w > 0 else { continue }
      let height = abs(root.y - c.support.x), tolerance = c.support.y
      let q = clamp((height - tolerance * 0.5) / max(tolerance * 0.5, 0.0001), 0, 1)
      let w = (dx == 0 ? 1 - f.x : f.x) * (dz == 0 ? 1 - f.y : f.y)
        * (1 - q * q * (3 - 2 * q))
      result += SIMD3(c.bend.x, c.bend.y, c.bend.z) * w
    } }
    return result
  }

  /// A source blade rotates about its root. The angle limit provides a strict
  /// extra-displacement bound for CPU/meshlet culling, including tall reeds.
  public static func rotation(_ response: SIMD3<Float>, bladeRadius: Float) -> SIMD4<Float> {
    let displacement = SIMD2(response.x, response.y), magnitude = length(displacement)
    guard magnitude > 0.00001 else { return SIMD4(1, 0, 0, 0) }
    let direction = displacement / magnitude
    let desired = min(1.3, atan2(magnitude, max(bladeRadius, 0.05)) + response.z * 1.2)
    let limit = 2 * asin(min(1, maximumVertexDisplacement / (2 * max(bladeRadius, 0.0001))))
    return SIMD4(direction.y, 0, -direction.x, min(desired, limit))
  }
}
