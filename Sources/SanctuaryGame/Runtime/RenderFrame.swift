import FieldCore
import MetalKit

struct RenderItem {
  var batch: GPUBatch
  var instance: Instance? = nil
  var castsShadow = true
  var material: SIMD4<Float>? = nil
}

struct RenderFrame {
  var id: String
  var uniforms: Uniforms
  var camera: V3
  var lightSize: Float
  var paused: Bool
  var wind: Float
  var windState: WindSimulation
  var look: SceneLook
  var items: [RenderItem]
  var infiniteGround: Bool
}
