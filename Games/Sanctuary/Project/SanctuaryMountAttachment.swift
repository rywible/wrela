import FieldCore
import SanctuaryContent
import SimulationCore
import simd

/// Presentation attachment in authored metres. The expedition still owns the camera,
/// flight height and support elevation; this recipe only positions the visible animal.
enum SanctuaryMountAttachment {
  struct Recipe {
    /// Local eye point: +Y above the animal, +Z behind its forward-facing head.
    var eyeInBindSpace: V3
    /// Camera/support separation is a world-space contract for grounded travel.
    var fixedGroundEyeHeight: Float?
    var eyeClearance: Float

    func rootPosition(player: PlayerCamera, morphologyScale: Float) -> V3 {
      if let fixedGroundEyeHeight {
        return player.position - V3(0, fixedGroundEyeHeight, 0)
      }
      let eye = eyeInBindSpace * morphologyScale + V3(0, eyeClearance, 0)
      // The camera's positive yaw turns toward +X; creature transforms use -yaw.
      let rotation = transformDirection(eye, cameraYaw: player.yaw)
      return player.position - rotation
    }

    private func transformDirection(_ p: V3, cameraYaw: Float) -> V3 {
      let c = cos(cameraYaw), s = sin(cameraYaw)
      return V3(c * p.x - s * p.z, p.y, s * p.x + c * p.z)
    }
  }

  static func recipe(for species: WildlifeSpecies, mode: SanctuaryJourney.TravelMode) -> Recipe {
    switch mode {
    case .walking:
      return Recipe(eyeInBindSpace: .zero, fixedGroundEyeHeight: nil, eyeClearance: 0)
    case .riding:
      // Keep every riding root on the same composed ground as before, including
      // young animals. Scaling this 2.5 m camera contract would lift their feet.
      return Recipe(eyeInBindSpace: .zero, fixedGroundEyeHeight: 2.5, eyeClearance: 0)
    case .flying:
      switch species {
      case .canopyGlider, .cloudRay:
        // The old .46/.42 m offsets put the eye inside the glider's head and
        // among the ray's posed wings. Seat the eye above and slightly aft of
        // those surfaces, leaving the lower view available for animal cues.
        return Recipe(eyeInBindSpace: V3(0, 0.95, 0.25),
          fixedGroundEyeHeight: nil, eyeClearance: 0.08)
      default:
        // Unsupported flight cannot normally be entered through expedition rules.
        // Retain a finite attachment if inspecting an older/incomplete fixture.
        return Recipe(eyeInBindSpace: V3(0, 0.95, 0.25),
          fixedGroundEyeHeight: nil, eyeClearance: 0.08)
      }
    }
  }
}
