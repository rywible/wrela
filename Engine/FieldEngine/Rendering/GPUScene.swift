import AppKit
import CryptoKit
import FieldCompiler
import FieldCore
import MetalKit
import simd

package struct Uniforms {
  package init(viewProjection:simd_float4x4,lightVP:simd_float4x4,inverseVP:simd_float4x4,cameraTime:SIMD4<Float>,sunExposure:SIMD4<Float>,options:SIMD4<Float>,sky:SIMD4<Float>,environment:SIMD4<Float>) {self.viewProjection=viewProjection;self.lightVP=lightVP;self.inverseVP=inverseVP;self.cameraTime=cameraTime;self.sunExposure=sunExposure;self.options=options;self.sky=sky;self.environment=environment}

  package var viewProjection: simd_float4x4
  package var lightVP: simd_float4x4
  package var inverseVP: simd_float4x4
  package var cameraTime: SIMD4<Float>
  package var sunExposure: SIMD4<Float>
  package var options: SIMD4<Float>
  package var sky: SIMD4<Float>
  package var environment: SIMD4<Float>
  package var weatherBounds: SIMD4<Float> = .zero
  package var skyTimes: SIMD4<Float> = .zero
  package var rigPosition: SIMD4<Float> = .zero
  package var rigColor: SIMD4<Float> = .zero
  package var rigParams: SIMD4<Float> = .zero
  package var lookSurface: SIMD4<Float> = SceneLook().surface
  package var lookLight: SIMD4<Float> = SceneLook().light
  package var lookGrade: SIMD4<Float> = SceneLook().grade
}

package final class BatchSelection {
  package var lod: [Int]
  package var counts: [Int]
  package init(instances: Int, levels: Int) {
    lod = Array(repeating: 0, count: instances)
    counts = Array(repeating: 0, count: levels)
  }
}

package struct GPUBatch {
  package var name: String
  package var vertices: MTLBuffer
  package var indices: MTLBuffer
  package var instances: MTLBuffer
  package var indexCount: Int
  package var instanceCount: Int
  package var sourceInstances: [Instance]
  package var visibleBuffers: [MTLBuffer]
  package var selection: BatchSelection
  package var center: V3
  package var radius: Float
  package var lods: [GPUBatch]
  package var error: Float
  package var meshlets: MTLBuffer
  package var meshletVertices: MTLBuffer
  package var meshletTriangles: MTLBuffer
  package var material: SIMD4<Float> = SIMD4(-1, 0, 0, 0)
  package var meshletCount: Int
  package var doubleSided = false
  package var proceduralGrass = false
}
