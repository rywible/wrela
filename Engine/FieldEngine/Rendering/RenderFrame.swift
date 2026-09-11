import FieldCore
import MetalKit

package struct RenderItem {
  package init(batch:GPUBatch,instance:Instance?=nil,castsShadow:Bool=true,material:SIMD4<Float>?=nil) {self.batch=batch;self.instance=instance;self.castsShadow=castsShadow;self.material=material}

  package var batch: GPUBatch
  package var instance: Instance? = nil
  package var castsShadow = true
  package var material: SIMD4<Float>? = nil
}

package struct RenderFrame {
  package init(id:String,uniforms:Uniforms,camera:V3,lightSize:Float,paused:Bool,wind:Float,windState:WindSimulation,look:SceneLook,items:[RenderItem],infiniteGround:Bool) {self.id=id;self.uniforms=uniforms;self.camera=camera;self.lightSize=lightSize;self.paused=paused;self.wind=wind;self.windState=windState;self.look=look;self.items=items;self.infiniteGround=infiniteGround}

  package var id: String
  package var uniforms: Uniforms
  package var camera: V3
  package var lightSize: Float
  package var paused: Bool
  package var wind: Float
  package var windState: WindSimulation
  package var look: SceneLook
  package var items: [RenderItem]
  package var infiniteGround: Bool
}
