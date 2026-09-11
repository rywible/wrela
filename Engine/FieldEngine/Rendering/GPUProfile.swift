import Metal

/// Stage-boundary counters supported by Apple GPUs. Each sampled frame owns its
/// buffer until completion; timings are stage intervals, not additive frame time.
package final class GPUProfile {
  package let samples: MTLCounterSampleBuffer
  package var intervals: [(String, Int, Int)] = []
  package var count = 0
  package var cpuOrigin: MTLTimestamp = 0
  package var gpuOrigin: MTLTimestamp = 0
  package let device: MTLDevice
  package static func supported(_ device: MTLDevice) -> Bool {
    device.hasUnifiedMemory && device.supportsCounterSampling(.atStageBoundary)
      && device.counterSets?.contains(where: { $0.name == MTLCommonCounterSet.timestamp.rawValue })
        == true
  }
  init?(_ device: MTLDevice) {
    guard Self.supported(device),
      let set = device.counterSets?.first(where: {
        $0.name == MTLCommonCounterSet.timestamp.rawValue
      })
    else { return nil }
    let d = MTLCounterSampleBufferDescriptor()
    d.counterSet = set
    d.storageMode = .shared
    d.sampleCount = 256
    guard let buffer = try? device.makeCounterSampleBuffer(descriptor: d) else { return nil }
    samples = buffer
    self.device = device
    (cpuOrigin, gpuOrigin) = device.sampleTimestamps()
  }
  package func pair(_ name: String) -> (Int, Int) {
    precondition(count + 2 <= 256)
    let start = count
    count += 2
    intervals.append((name, start, start + 1))
    return (start, start + 1)
  }
  package func compute(_ cb: MTLCommandBuffer, _ name: String) -> MTLComputeCommandEncoder {
    let d = MTLComputePassDescriptor()
    let p = pair(name)
    let a = d.sampleBufferAttachments[0]!
    a.sampleBuffer = samples
    a.startOfEncoderSampleIndex = p.0
    a.endOfEncoderSampleIndex = p.1
    let e = cb.makeComputeCommandEncoder(descriptor: d)!
    e.label = name
    return e
  }
  package func render(_ d: MTLRenderPassDescriptor, _ name: String) {
    let v = pair(name + ".vertex")
    let f = pair(name + ".fragment")
    let a = d.sampleBufferAttachments[0]!
    a.sampleBuffer = samples
    a.startOfVertexSampleIndex = v.0
    a.endOfVertexSampleIndex = v.1
    a.startOfFragmentSampleIndex = f.0
    a.endOfFragmentSampleIndex = f.1
  }
  package func resolve() -> [String: Double] {
    guard let data = try? samples.resolveCounterRange(0..<count),
      data.count >= count * MemoryLayout<MTLCounterResultTimestamp>.stride
    else { return [:] }
    let (cpuEnd, gpuEnd) = device.sampleTimestamps()
    guard cpuEnd > cpuOrigin, gpuEnd > gpuOrigin else { return [:] }
    // Metal CPU timestamps are already nanoseconds (not mach clock ticks).
    let milliseconds = Double(cpuEnd - cpuOrigin) / Double(gpuEnd - gpuOrigin) / 1_000_000
    return data.withUnsafeBytes { bytes in
      let values = bytes.bindMemory(to: MTLCounterResultTimestamp.self)
      var result: [String: Double] = [:]
      for (name, start, end) in intervals {
        let a = values[start].timestamp
        let b = values[end].timestamp
        if a != 0 && b != UInt64.max && b >= a {
          result[name, default: 0] += Double(b - a) * milliseconds
        }
      }
      return result
    }
  }
}
