import FieldCore
import simd

/// Soundstage camera-depth policy for metre-authored subjects.
///
/// This keeps the existing forward depth projection. It only moves the near
/// plane from 0.1% to at most 1% of the subject extent when a conservative
/// subject bound leaves enough clear space in front of the camera.
enum StudioProjection {
  static let minimumNear: Float = 0.00001

  static func conservativeRadius(minimum: V3, maximum: V3, around focus: V3) -> Float {
    guard [minimum.x, minimum.y, minimum.z, maximum.x, maximum.y, maximum.z,
      focus.x, focus.y, focus.z].allSatisfy(\.isFinite),
      all(minimum .<= maximum)
    else { return .infinity }
    let reach = simd_max(abs(minimum - focus), abs(maximum - focus))
    return length(reach)
  }

  static func nearPlane(
    cameraDistance: Float, subjectRadius: Float, subjectExtent: Float
  ) -> Float {
    let validExtent = subjectExtent.isFinite && subjectExtent > 0 ? subjectExtent : 0
    let baseline = max(minimumNear, validExtent * 0.001)
    guard cameraDistance.isFinite, subjectRadius.isFinite, cameraDistance > 0,
      subjectRadius >= 0
    else { return baseline }

    let clearance = cameraDistance - subjectRadius
    guard clearance > baseline else { return baseline }
    let precisionTarget = max(baseline, validExtent * 0.01)
    // Half the conservative clearance retains room for bound rounding and
    // avoids changing close or interior inspection cameras.
    return max(baseline, min(precisionTarget, clearance * 0.5))
  }
}
