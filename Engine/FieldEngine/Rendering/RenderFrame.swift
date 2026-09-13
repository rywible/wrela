import FieldCore
import MetalKit
import simd

package struct RenderItem {
  package init(batch:GPUBatch,instance:Instance?=nil,castsShadow:Bool=true,material:SIMD4<Float>?=nil) {self.batch=batch;self.instance=instance;self.castsShadow=castsShadow;self.material=material}

  package var batch: GPUBatch
  package var instance: Instance? = nil
  package var correctives:[CorrectiveUniform] = []
  package var surfaceLayers:[SurfaceLayer] = []
  package var skinPalette: [simd_float4x4] = []
  package var castsShadow = true
  package var material: SIMD4<Float>? = nil
}

/// Camera-dependent shadow placement supplied by the scene composer. Keeping
/// this CPU description lets a completed sky choose its own sun direction while
/// the shadow region continues to follow the current scene camera.
package struct OutdoorShadow {
  package let center: V3
  package let size, near, far, distance: Float
  package init(center: V3, size: Float, near: Float, far: Float, distance: Float) {
    self.center = center; self.size = size; self.near = near; self.far = far; self.distance = distance
  }
}

package struct RenderFrame {
  package init(id:String,uniforms:Uniforms,camera:V3,lightSize:Float,paused:Bool,wind:Float,windState:WindSimulation,look:SceneLook,items:[RenderItem],infiniteGround:Bool,outdoorShadow:OutdoorShadow?=nil,surfaceInfluences:SurfaceInfluenceSnapshot = .empty) {self.id=id;self.uniforms=uniforms;self.camera=camera;self.lightSize=lightSize;self.paused=paused;self.wind=wind;self.windState=windState;self.look=look;self.items=items;self.infiniteGround=infiniteGround;self.outdoorShadow=outdoorShadow;self.surfaceInfluences=surfaceInfluences}

  package var id: String
  package var uniforms: Uniforms
  package var camera: V3
  package var surfaceInfluences: SurfaceInfluenceSnapshot
  package var outdoorShadow: OutdoorShadow?
  package var lightSize: Float
  package var paused: Bool
  package var wind: Float
  package var windState: WindSimulation
  package var look: SceneLook
  package var items: [RenderItem]
  package var infiniteGround: Bool
}
