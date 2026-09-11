import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import Metal
import simd

extension WorkshopRenderer {
  func verifyFields() throws -> [String: Any] {
    let fields: [(String, Shape)] = [
      ("sphere", Shape.sphere(1)),
      (
        "transformed-union",
        Shape.sphere(1).sized(2.3).moved(V3(-1, 0.5, 0.4)).joined(.box(V3(1, 0.4, 0.7)))
      ),
      ("capsule", .capsule(V3(-1, 0.3, 0), V3(1, 2, 0.2), 0.3)),
      ("degenerate-capsule", .capsule(.zero, .zero, 1)),
      ("torus", .torus(1.6, 0.3)),
      ("stretched-canopy", Shape.sphere(1).stretched(V3(2, 0.7, 1.2)).moved(V3(0, 1.4, 0))),
    ]
    var reports: [[String: Any]] = []
    var worst: Float = 0
    for (name, shape) in fields {
      let source = try MetalField.source(for: shape)
      let library = try device.makeLibrary(source: source, options: nil)
      let pipeline = try device.makeComputePipelineState(
        function: library.makeFunction(name: "evaluateField")!)
      var rng = SeededRandom(seed: 731)
      let points = (0..<4096).map { _ in
        SIMD4<Float>(rng.range(-4, 4), rng.range(-4, 4), rng.range(-4, 4), 1)
      }
      let input = device.makeBuffer(
        bytes: points, length: MemoryLayout<SIMD4<Float>>.stride * points.count)!
      let output = device.makeBuffer(
        length: MemoryLayout<Float>.stride * points.count, options: .storageModeShared)!
      let command = queue.makeCommandBuffer()!
      let encoder = command.makeComputeCommandEncoder()!
      encoder.setComputePipelineState(pipeline)
      encoder.setBuffer(input, offset: 0, index: 0)
      encoder.setBuffer(output, offset: 0, index: 1)
      encoder.dispatchThreads(
        MTLSize(width: points.count, height: 1, depth: 1),
        threadsPerThreadgroup: MTLSize(width: pipeline.threadExecutionWidth, height: 1, depth: 1))
      encoder.endEncoding()
      command.commit()
      command.waitUntilCompleted()
      if let error = command.error { throw error }
      let values = output.contents().bindMemory(to: Float.self, capacity: points.count)
      var maximum: Float = 0
      for (i, p) in points.enumerated() {
        guard values[i].isFinite else {
          throw RuntimeError.message("GPU field produced nonfinite output")
        }
        maximum = max(maximum, abs(values[i] - shape.value(at: V3(p.x, p.y, p.z))))
      }
      worst = max(worst, maximum)
      reports.append([
        "field": name, "samples": points.count, "maxAbsoluteError": maximum,
        "passed": maximum < 0.0002,
      ])
    }
    return [
      "passed": worst < 0.0002, "tolerance": 0.0002, "maxAbsoluteError": worst, "fields": reports,
      "device": device.name,
    ]
  }
}

extension WorkshopRenderer {
  /// Actual GPU quadrature against the closed-form diffuse view factor of two
  /// constant hemispheres. Catches normal-dependent sampling bands and energy errors.
  func verifyDiffuseLighting() throws -> Float {
    let width = 64
    let height = 32
    let descriptor = MTLTextureDescriptor.texture2DDescriptor(
      pixelFormat: .rgba32Float, width: width, height: height, mipmapped: false)
    descriptor.storageMode = .shared
    descriptor.usage = [.shaderRead, .shaderWrite]
    let sky = device.makeTexture(descriptor: descriptor)!
    let output = device.makeTexture(descriptor: descriptor)!
    let white = Array(repeating: SIMD4<Float>(repeating: 1), count: width * height)
    white.withUnsafeBytes {
      sky.replace(
        region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: $0.baseAddress!,
        bytesPerRow: width * 16)
    }
    var u = Uniforms(
      viewProjection: matrix_identity_float4x4, lightVP: matrix_identity_float4x4,
      inverseVP: matrix_identity_float4x4,
      cameraTime: .zero, sunExposure: SIMD4(1, 0, 0, 1), options: .zero, sky: .zero,
      environment: .zero)
    let command = queue.makeCommandBuffer()!
    let encoder = command.makeComputeCommandEncoder()!
    encoder.setComputePipelineState(atmosphere.irradiancePipeline)
    encoder.setTexture(sky, index: 0)
    encoder.setTexture(output, index: 1)
    encoder.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
    encoder.dispatchThreads(
      MTLSize(width: width, height: height, depth: 1),
      threadsPerThreadgroup: MTLSize(width: 8, height: 8, depth: 1))
    encoder.endEncoding()
    command.commit()
    command.waitUntilCompleted()
    if let error = command.error { throw error }
    var pixels = white
    pixels.withUnsafeMutableBytes {
      output.getBytes(
        $0.baseAddress!, bytesPerRow: width * 16, from: MTLRegionMake2D(0, 0, width, height),
        mipmapLevel: 0)
    }
    var maximum: Float = 0
    for y in 0..<height {
      let v = (Float(y) + 0.5) / Float(height) * 2 - 1
      let ny = sin((v < 0 ? -1 : 1) * v * v * Float.pi * 0.5)
      let expected =
        SIMD3<Float>(repeating: (1 + ny) / 2) + SIMD3<Float>(0.18, 0.20, 0.12) * ((1 - ny) / 2)
      for x in 0..<width {
        let actual = pixels[y * width + x]
        for channel in 0..<3 {
          guard actual[channel].isFinite else {
            throw RuntimeError.message("Nonfinite sky irradiance")
          }
          maximum = max(maximum, abs(actual[channel] - expected[channel]))
        }
      }
    }
    guard maximum < 0.009 else {
      throw RuntimeError.message("Diffuse sky quadrature error \(maximum) exceeds 0.009")
    }
    return maximum
  }
}
