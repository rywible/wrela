import FieldCompiler
import FieldCore
import Foundation
import Metal

extension SanctuarySession {
  func verifyFields() throws -> [String: Any] {
    let fields: [(String, Shape)] = [
      ("seed-pod", world.studioShape),
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
