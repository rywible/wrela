import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import simd

/// A quiet, transient brush survey. A saved nature contribution owns the actual
/// growth/ripple response; this presentation never creates or advances an event.
final class SanctuaryNaturePreviewPresentation {
  private static let readyColor = V3(0.52, 0.68, 0.46)
  private static let rejectedColor = V3(0.76, 0.43, 0.30)
  private static let outerCount = 40
  private static let innerCount = 20
  private static let jointCount = outerCount + innerCount

  private enum Mark: Int, CaseIterable {
    case planting, raise, lower, smooth, rejected
  }

  private struct SupportKey: Equatable {
    let point: V3
    let radius: Float
    let revision: UInt64
  }

  /// The game and studio use the same cached visible-surface survey. The caller
  /// composes published ground/water; this cache owns no editing or water rules.
  struct SupportCache {
    private var key: SupportKey?
    private(set) var palette = [simd_float4x4](repeating: matrix_identity_float4x4,
      count: SanctuaryNaturePreviewPresentation.jointCount)
    private(set) var markerPoint = V3.zero
    private(set) var queriesLastFrame = 0

    mutating func reset() {
      key = nil
      queriesLastFrame = 0
    }

    mutating func update(
      point: V3, radius: Float, revision: UInt64, supportHeight: (Float, Float) -> Float
    ) {
      queriesLastFrame = 0
      let next = SupportKey(point: point, radius: radius, revision: revision)
      guard key != next else { return }
      var queries = 0
      func sample(_ x: Float, _ z: Float) -> Float {
        queries += 1
        return supportHeight(x, z)
      }
      palette = SanctuaryNaturePreviewPresentation.supportPalette(
        point: point, radius: radius, supportHeight: sample)
      // The exact editing target remains unchanged. Only the transient marker
      // is lifted onto a visible water surface covering that target.
      let height = sample(point.x, point.z)
      markerPoint = point
      if height.isFinite, abs(height - point.y) <= 128 {
        markerPoint.y = max(point.y, height)
      }
      queriesLastFrame = queries
      key = next
    }
  }

  private let boundary: GPUBatch
  private let marks: [Mark: GPUBatch]
  private var support = SupportCache()
  var supportQueriesLastFrame: Int { support.queriesLastFrame }

  init(graphics: MetalRenderer) {
    boundary = graphics.upload(Self.boundaryRecipe)
    marks = Dictionary(uniqueKeysWithValues: Mark.allCases.map {
      ($0, graphics.upload(Self.markRecipe($0)))
    })
  }

  func reset() {
    support.reset()
  }

  /// Sixty boundary samples plus one marker sample when the target/radius or visible-support
  /// revision changes. Camera-only facing changes reuse the existing skin palette.
  func items(
    preview: SanctuaryNatureSession.Preview?, supportRevision: UInt64,
    supportHeight: (Float, Float) -> Float
  ) -> [RenderItem] {
    guard let preview,
      [preview.targetPoint.x, preview.targetPoint.y, preview.targetPoint.z, preview.radius].allSatisfy(\.isFinite),
      abs(preview.targetPoint.x) <= 20_000, abs(preview.targetPoint.z) <= 20_000,
      abs(preview.targetPoint.y) <= 16_000, (1...16).contains(preview.radius)
    else { reset(); return [] }
    support.update(point: preview.targetPoint, radius: preview.radius,
      revision: supportRevision, supportHeight: supportHeight)
    let color = preview.valid ? Self.readyColor : Self.rejectedColor
    var ringInstance = boundary.sourceInstances[0]
    ringInstance.model = transform(preview.targetPoint, V3(repeating: 1), 0)
    ringInstance.tint = SIMD4(color, 7)
    var ringItem = RenderItem(batch: boundary, instance: ringInstance, castsShadow: false)
    ringItem.skinPalette = support.palette

    let mark = Self.mark(for: preview.currentDraft.action, valid: preview.valid)
    guard let batch = marks[mark] else { return [ringItem] }
    let direction = preview.currentDraft.direction
    let yaw = direction.x.isFinite && direction.z.isFinite ? atan2(direction.x, direction.z) : 0
    var markerInstance = batch.sourceInstances[0]
    markerInstance.model = transform(support.markerPoint,
      V3(repeating: min(1.55, 1 + preview.radius * 0.035)), yaw)
    markerInstance.tint = SIMD4(color, 7)
    return [ringItem, RenderItem(batch: batch, instance: markerInstance, castsShadow: false)]
  }

  private static func mark(for action: SanctuaryNatureIntent.Action, valid: Bool) -> Mark {
    guard valid else { return .rejected }
    switch action {
    case .plant: return .planting
    case .sculpt(.raise): return .raise
    case .sculpt(.lower): return .lower
    case .sculpt(.smooth): return .smooth
    }
  }

  /// The inner contour is the half-influence locus of HabitatGarden's current
  /// radial smoothstep (r / sqrt(2)). It is a guide, never another terrain kernel.
  private static let rings: [(count: Int, radius: Float, width: Float, tint: V3)] = [
    (outerCount, 1, 0.019, V3(repeating: 1)),
    (innerCount, 1 / sqrt(2), 0.009, V3(0.78, 0.86, 0.73)),
  ]

  // Internal so regressions can inspect the exact skinned source used by the game.
  static let boundaryRecipe: SceneBatch = {
    var mesh = Mesh()
    var weights: [SkinWeight] = []
    var jointOffset = 0
    for ring in rings {
      for segment in 0..<ring.count {
        let next = (segment + 1) % ring.count
        for side in 0..<4 {
          func vertex(_ sample: Int, _ corner: Int) -> Vertex {
            let angle = Float(sample) * 2 * Float.pi / Float(ring.count)
            let section = Float(corner) * 2 * Float.pi / 4
            let radial = V3(cos(angle), 0, sin(angle))
            let normal = radial * cos(section) + V3(0, sin(section), 0)
            return Vertex(radial * ring.radius + V3(0, 0.055, 0) + normal * ring.width,
              normal, ring.tint)
          }
          let a = vertex(segment, side), b = vertex(next, side)
          let c = vertex(next, side + 1), d = vertex(segment, side + 1)
          mesh.triangle(a, c, b)
          mesh.triangle(a, d, c)
          let first = SkinWeight(UInt32(jointOffset + segment))
          let second = SkinWeight(UInt32(jointOffset + next))
          weights += [first, second, second, first, first, second]
        }
      }
      jointOffset += ring.count
    }
    var batch = SceneBatch(name: "Nature brush · boundary and falloff", mesh: mesh,
      instances: [Instance(tint: readyColor, kind: 7)], roughness: 0.88)
    batch.skinJoints = (0..<jointCount).map { "Nature brush support \($0)" }
    batch.skinWeights = weights
    return batch
  }()

  /// Endpoint frames preserve metre-sized stroke width as the radius changes.
  /// Adjacent vertices share frames, so the brush remains a continuous surface.
  /// The visible surface between samples is linearly bridged; this is a support survey.
  private static func supportPalette(
    point: V3, radius: Float, supportHeight: (Float, Float) -> Float
  ) -> [simd_float4x4] {
    var result: [simd_float4x4] = []
    for ring in rings {
      var anchors: [V3] = []
      var contacts: [V3] = []
      for index in 0..<ring.count {
        let angle = Float(index) * 2 * Float.pi / Float(ring.count)
        let anchor = V3(cos(angle) * ring.radius, 0.055, sin(angle) * ring.radius)
        let x = point.x + anchor.x * radius, z = point.z + anchor.z * radius
        let height = supportHeight(x, z)
        let offset = height.isFinite && abs(height - point.y) <= 128 ? height - point.y : 0
        anchors.append(anchor)
        contacts.append(V3(anchor.x * radius, offset + 0.055, anchor.z * radius))
      }
      for index in 0..<ring.count {
        let previous = (index + ring.count - 1) % ring.count, next = (index + 1) % ring.count
        let oldTangent = normalize(anchors[next] - anchors[previous])
        let newTangent = normalize(contacts[next] - contacts[previous])
        let rotation = simd_float4x4(simd_quatf(from: oldTangent, to: newTangent))
        result.append(transform(contacts[index], V3(repeating: 1), 0) * rotation
          * transform(-anchors[index], V3(repeating: 1), 0))
      }
    }
    return result
  }

  private static func markRecipe(_ mark: Mark) -> SceneBatch {
    var pieces: [Mesh] = []
    func stroke(_ a: V3, _ b: V3, _ width: Float = 0.015) {
      let delta = b - a
      let rotation = simd_quatf(from: V3(0, 1, 0), to: normalize(delta))
      var mesh = SanctuaryProceduralCraft.box(.zero,
        V3(width, length(delta) * 0.5, width), color: V3(repeating: 1))
      for index in mesh.vertices.indices {
        let vertex = mesh.vertices[index]
        let p = rotation.act(V3(vertex.position.x, vertex.position.y, vertex.position.z)) + (a + b) * 0.5
        let n = rotation.act(V3(vertex.normal.x, vertex.normal.y, vertex.normal.z))
        mesh.vertices[index] = Vertex(p, n, V3(repeating: 1))
      }
      pieces.append(mesh)
    }
    // An open diamond pins the exact queried target, without covering its ground.
    let diamond = [V3(0, 0.065, -0.13), V3(0.13, 0.065, 0),
      V3(0, 0.065, 0.13), V3(-0.13, 0.065, 0)]
    for index in diamond.indices { stroke(diamond[index], diamond[(index + 1) % 4], 0.011) }
    stroke(V3(0, 0.07, 0), V3(0, 0.18, 0), 0.012)
    switch mark {
    case .planting:
      stroke(V3(0, 0.18, 0), V3(0, 0.41, 0), 0.012)
      stroke(V3(0, 0.25, 0), V3(-0.12, 0.34, 0), 0.014)
      stroke(V3(0, 0.31, 0), V3(0.12, 0.40, 0), 0.014)
    case .raise, .lower:
      let tip = mark == .raise ? V3(0, 0.47, 0) : V3(0, 0.21, 0)
      let tail = mark == .raise ? V3(0, 0.21, 0) : V3(0, 0.47, 0)
      let shoulder: Float = mark == .raise ? 0.35 : 0.33
      stroke(tail, tip)
      for side: Float in [-1, 1] { stroke(tip, V3(side * 0.10, shoulder, 0)) }
    case .smooth:
      stroke(V3(-0.16, 0.28, 0), V3(0.16, 0.28, 0))
      stroke(V3(-0.10, 0.38, 0), V3(0.10, 0.38, 0), 0.012)
    case .rejected:
      stroke(V3(-0.13, 0.23, 0), V3(0.13, 0.49, 0), 0.021)
      stroke(V3(-0.13, 0.49, 0), V3(0.13, 0.23, 0), 0.021)
    }
    return SceneBatch(name: "Nature brush · \(mark)", mesh: ParametricMesh.joined(pieces),
      instances: [Instance(tint: mark == .rejected ? rejectedColor : readyColor, kind: 7)], roughness: 0.86)
  }

  /// Soundstage compiles and deforms the exact game recipe on a study-owned plane.
  /// No game terrain or save is created by inspecting this registered subject.
  static var generators: [AssetGenerator] {
    [AssetGenerator(id: "nature-brush-preview", name: "Nature · Brush survey", controls: [
      ScalarControl("radius", "Brush radius · m", 4, 1...16),
      ScalarControl("valid", "Ready to cast", 1, 0...1),
      ScalarControl("operation", "Plant / raise / lower / smooth", 0, 0...3),
      ScalarControl("slopeX", "Study support X slope", 0, -0.6...0.6),
      ScalarControl("slopeZ", "Study support Z slope", 0, -0.6...0.6),
      ScalarControl("facing", "Marker facing · degrees", 0, -180...180),
    ], compile: { source in
      let radius = source.parameters["radius"] ?? 4
      let valid = (source.parameters["valid"] ?? 1) >= 0.5
      let operation = min(3, max(0, Int((source.parameters["operation"] ?? 0).rounded())))
      let mark = valid ? Mark(rawValue: operation)! : .rejected
      let slopeX = source.parameters["slopeX"] ?? 0, slopeZ = source.parameters["slopeZ"] ?? 0
      var support = SupportCache()
      support.update(point: .zero, radius: radius, revision: 0) { x, z in x * slopeX + z * slopeZ }
      let matrices = support.palette
      let color = valid ? readyColor : rejectedColor
      var boundary = boundaryRecipe
      for index in boundary.mesh.vertices.indices {
        let old = boundary.mesh.vertices[index]
        let matrix = boundary.skinWeights[index].matrix(matrices)
        let p = matrix * old.position
        let n = matrix * SIMD4(old.normal.x, old.normal.y, old.normal.z, 0)
        boundary.mesh.vertices[index] = Vertex(V3(p.x, p.y, p.z), normalize(V3(n.x, n.y, n.z)),
          V3(old.color.x, old.color.y, old.color.z))
      }
      boundary.skinJoints = []; boundary.skinWeights = []
      boundary.instances = [Instance(tint: color, kind: 7)]
      var marker = markRecipe(mark)
      marker.instances = [Instance(position: support.markerPoint,
        scale: V3(repeating: min(1.55, 1 + radius * 0.035)),
        yaw: (source.parameters["facing"] ?? 0) * .pi / 180, tint: color, kind: 7)]
      return [boundary, marker]
    })]
  }
}
