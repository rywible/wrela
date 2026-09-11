import AppKit
import CryptoKit
import FieldCompiler
import FieldCore
import MetalKit
import simd

struct Uniforms {
  var viewProjection: simd_float4x4
  var lightVP: simd_float4x4
  var inverseVP: simd_float4x4
  var cameraTime: SIMD4<Float>
  var sunExposure: SIMD4<Float>
  var options: SIMD4<Float>
  var sky: SIMD4<Float>
  var environment: SIMD4<Float>
  var weatherBounds: SIMD4<Float> = .zero
  var skyTimes: SIMD4<Float> = .zero
  var rigPosition: SIMD4<Float> = .zero
  var rigColor: SIMD4<Float> = .zero
  var rigParams: SIMD4<Float> = .zero
  var lookSurface: SIMD4<Float> = SceneLook().surface
  var lookLight: SIMD4<Float> = SceneLook().light
  var lookGrade: SIMD4<Float> = SceneLook().grade
}

final class BatchSelection {
  var lod: [Int]
  var counts: [Int]
  init(instances: Int, levels: Int) {
    lod = Array(repeating: 0, count: instances)
    counts = Array(repeating: 0, count: levels)
  }
}

struct GPUBatch {
  var name: String
  var vertices: MTLBuffer
  var indices: MTLBuffer
  var instances: MTLBuffer
  var indexCount: Int
  var instanceCount: Int
  var sourceInstances: [Instance]
  var visibleBuffers: [MTLBuffer]
  var selection: BatchSelection
  var center: V3
  var radius: Float
  var lods: [GPUBatch]
  var error: Float
  var meshlets: MTLBuffer
  var meshletVertices: MTLBuffer
  var meshletTriangles: MTLBuffer
  var material: SIMD4<Float> = SIMD4(-1, 0, 0, 0)
  var meshletCount: Int
  var doubleSided = false
  var proceduralGrass = false
}
