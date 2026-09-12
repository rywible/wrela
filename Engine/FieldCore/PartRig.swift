import Foundation
import simd

/// Fields are authored in shared bind coordinates (metres). Each joint applies a
/// delta about its explicit bind pivot; descendants inherit that entire delta.
public struct PartJoint: Codable, Equatable, Sendable {
  public var id: String
  public var parent: String?
  public var pivot: SIMD3<Float> = .zero
  public var offset: SIMD3<Float> = .zero
  public var rotation: SIMD3<Float> = .zero  // degrees, XYZ
  public var scale: Float = 1
  public init(id: String, parent: String? = nil, pivot: SIMD3<Float> = .zero) {
    self.id = id
    self.parent = parent
    self.pivot = pivot
  }
}
public struct JointPose: Codable, Equatable, Sendable {
  public var scale = SIMD3<Float>(repeating: 1)
  public var offset: SIMD3<Float> = .zero
  public var rotation: SIMD3<Float> = .zero
  public init(
    offset: SIMD3<Float> = .zero, rotation: SIMD3<Float> = .zero,
    scale: SIMD3<Float> = SIMD3(repeating: 1)
  ) {
    self.offset = offset
    self.rotation = rotation
    self.scale = scale
  }
}
public enum PartRig {
  public static func validate(_ joints: [PartJoint]) throws {
    let ids = Set(joints.map(\.id))
    guard ids.count == joints.count, joints.count <= 64 else { throw RigError.invalid }
    for j in joints {
      guard !j.id.isEmpty, j.id.count <= 80, j.parent == nil || ids.contains(j.parent!),
        [j.pivot, j.offset, j.rotation].allSatisfy({ p in
          (0..<3).allSatisfy { p[$0].isFinite && abs(p[$0]) <= 360 }
        }), j.scale.isFinite, (0.25...4).contains(j.scale)
      else { throw RigError.invalid }
      var seen: Set<String> = [j.id]
      var parent = j.parent
      while let id = parent {
        guard seen.insert(id).inserted else { throw RigError.invalid }
        parent = joints.first { $0.id == id }?.parent
      }
    }
  }
  /// Input must pass validate once at the authoring boundary, never in a draw loop.
  public static func matrices(_ joints: [PartJoint], poses: [String: JointPose] = [:]) -> [String:
    simd_float4x4]
  {
    var result: [String: simd_float4x4] = [:]
    func translation(_ p: SIMD3<Float>) -> simd_float4x4 {
      var m = matrix_identity_float4x4
      m.columns.3 = SIMD4(p, 1)
      return m
    }
    func visit(_ j: PartJoint) -> simd_float4x4 {
      if let m = result[j.id] { return m }
      let pose = poses[j.id] ?? JointPose()
      let r = (j.rotation + pose.rotation) * (.pi / 180)
      let q =
        simd_quatf(angle: r.z, axis: SIMD3(0, 0, 1))
        * simd_quatf(angle: r.y, axis: SIMD3(0, 1, 0))
        * simd_quatf(angle: r.x, axis: SIMD3(1, 0, 0))
      let s = simd_float4x4(diagonal: SIMD4(pose.scale * j.scale, 1))
      let parent =
        j.parent.flatMap { id in joints.first { $0.id == id } }.map(visit)
        ?? matrix_identity_float4x4
      let m =
        parent * translation(j.pivot + j.offset + pose.offset) * simd_float4x4(q) * s
        * translation(-j.pivot)
      result[j.id] = m
      return m
    }
    for j in joints { _ = visit(j) }
    return result
  }
}
public enum RigError: LocalizedError {
  case invalid
  public var errorDescription: String? {
    "Invalid rig: use unique part IDs, finite transforms, existing parents and no attachment cycles."
  }
}

extension PartJoint {
  enum CodingKeys: String, CodingKey { case id, parent, pivot, offset, rotation, scale }
  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.init(
      id: try c.decode(String.self, forKey: .id),
      parent: try c.decodeIfPresent(String.self, forKey: .parent),
      pivot: try c.decodeIfPresent(SIMD3<Float>.self, forKey: .pivot) ?? .zero)
    offset = try c.decodeIfPresent(SIMD3<Float>.self, forKey: .offset) ?? .zero
    rotation = try c.decodeIfPresent(SIMD3<Float>.self, forKey: .rotation) ?? .zero
    scale = try c.decodeIfPresent(Float.self, forKey: .scale) ?? 1
  }
}
