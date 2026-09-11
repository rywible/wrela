import MetalKit

/// Preserve radiance through rasterization; apply glare and display mapping once.
final class PostProcess {
  let lightingPipeline: MTLComputePipelineState
  let lightStates: [MTLBuffer]
  let hdr: MTLTexture
  let multisample: MTLTexture
  let bloomA: MTLTexture
  let bloomB: MTLTexture
  let extract: MTLComputePipelineState
  let blur: MTLComputePipelineState
  let composite: MTLRenderPipelineState
  init(device: MTLDevice, library: MTLLibrary) throws {
    func texture(_ w: Int, _ h: Int, _ samples: Int = 1, _ writable: Bool = true) -> MTLTexture {
      let d = MTLTextureDescriptor.texture2DDescriptor(
        pixelFormat: .rgba16Float, width: w, height: h, mipmapped: false)
      d.storageMode = .private
      d.usage = writable ? [.shaderRead, .shaderWrite] : [.renderTarget, .shaderRead]
      if samples > 1 {
        d.storageMode = device.hasUnifiedMemory ? .memoryless : .private
        d.textureType = .type2DMultisample
        d.sampleCount = samples
        d.usage = .renderTarget
      }
      return device.makeTexture(descriptor: d)!
    }
    lightingPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "sceneLightState")!)
    lightStates = (0..<3).map { _ in device.makeBuffer(length: 32, options: .storageModePrivate)! }
    hdr = texture(1920, 1080, 1, false)
    multisample = texture(1920, 1080, 4)
    bloomA = texture(480, 270)
    bloomB = texture(480, 270)
    extract = try device.makeComputePipelineState(
      function: library.makeFunction(name: "bloomExtract")!)
    blur = try device.makeComputePipelineState(function: library.makeFunction(name: "bloomBlur")!)
    let d = MTLRenderPipelineDescriptor()
    d.vertexFunction = library.makeFunction(name: "skyVertex")
    d.fragmentFunction = library.makeFunction(name: "displayComposite")
    d.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
    composite = try device.makeRenderPipelineState(descriptor: d)
  }
  func encodeLighting(
    _ cb: MTLCommandBuffer, uniforms: inout Uniforms, atmosphere: Atmosphere, index: Int,
    profile: GPUProfile?
  ) -> MTLBuffer {
    let state = lightStates[index]
    let e = profile?.compute(cb, "lighting.state") ?? cb.makeComputeCommandEncoder()!
    e.setComputePipelineState(lightingPipeline)
    for (i, t) in [
      atmosphere.trans, atmosphere.previousSky, atmosphere.sky, atmosphere.cloudAmbient,
    ].enumerated() { e.setTexture(t, index: i) }
    e.setBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
    e.setBuffer(state, offset: 0, index: 1)
    e.dispatchThreads(
      MTLSize(width: 1, height: 1, depth: 1),
      threadsPerThreadgroup: MTLSize(width: 1, height: 1, depth: 1))
    e.endEncoding()
    return state
  }
  func encode(
    _ cb: MTLCommandBuffer, to output: MTLTexture, lightState: MTLBuffer, look: SIMD4<Float>,
    profile: GPUProfile? = nil
  ) {
    func compute(
      _ pipeline: MTLComputePipelineState, _ source: MTLTexture, _ target: MTLTexture,
      _ direction: SIMD2<Int32> = .zero
    ) {
      let e =
        profile?.compute(cb, pipeline === extract ? "post.extract" : "post.blur")
        ?? cb.makeComputeCommandEncoder()!
      e.setComputePipelineState(pipeline)
      e.setTexture(source, index: 0)
      e.setTexture(target, index: 1)
      var d = direction
      e.setBytes(&d, length: MemoryLayout<SIMD2<Int32>>.stride, index: 0)
      e.dispatchThreads(
        MTLSize(width: 480, height: 270, depth: 1),
        threadsPerThreadgroup: MTLSize(width: 8, height: 8, depth: 1))
      e.endEncoding()
    }
    compute(extract, hdr, bloomA)
    compute(blur, bloomA, bloomB, SIMD2(1, 0))
    compute(blur, bloomB, bloomA, SIMD2(0, 1))
    let pass = MTLRenderPassDescriptor()
    pass.colorAttachments[0].texture = output
    pass.colorAttachments[0].loadAction = .dontCare
    pass.colorAttachments[0].storeAction = .store
    profile?.render(pass, "display")
    let e = cb.makeRenderCommandEncoder(descriptor: pass)!
    e.label = "HDR glare and display mapping"
    e.setRenderPipelineState(composite)
    e.setFragmentTexture(hdr, index: 0)
    e.setFragmentTexture(bloomA, index: 1)
    e.setFragmentBuffer(lightState, offset: 0, index: 0)
    var grade = look
    e.setFragmentBytes(&grade, length: MemoryLayout<SIMD4<Float>>.stride, index: 1)
    e.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
    e.endEncoding()
  }
}
