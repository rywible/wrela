import AppKit
import CryptoKit
import FieldCompiler
import FieldCore
import MetalKit

/// Owns GPU work only. Scene assembly and gameplay live in the application session.
package final class MetalRenderer {
  package let device: MTLDevice
  package let queue: MTLCommandQueue
  package var atmosphere: Atmosphere!
  package var post: PostProcess!
  package var grassMeshPipeline: MTLRenderPipelineState!
  package var grassVertexPipeline: MTLRenderPipelineState!
  package var meshPipeline: MTLRenderPipelineState!
  package var materialPipelines: [Int: MTLRenderPipelineState] = [:]
  package var materialMeshPipelines: [Int: MTLRenderPipelineState] = [:]
  private var layeredPipeline: MTLRenderPipelineState!
  private var layeredMeshPipeline: MTLRenderPipelineState!
  private var layeredGrassPipeline: MTLRenderPipelineState!
  private var layeredGrassMeshPipeline: MTLRenderPipelineState!
  package var useMeshlets = true
  package var windBuffers: [MTLBuffer] = []
  package let view: MTKView
  package var pipeline: MTLRenderPipelineState!
  package var skyPipeline: MTLRenderPipelineState!
  package var groundPipeline: MTLRenderPipelineState!
  package var shadowPipeline: MTLRenderPipelineState!
  package var groomShadowPipeline: MTLRenderPipelineState!
  package var groomPipeline: MTLRenderPipelineState!
  package var groomMeshPipeline: MTLRenderPipelineState!
  package var layeredGroomPipeline: MTLRenderPipelineState!
  package var layeredGroomMeshPipeline: MTLRenderPipelineState!
  package var depthState: MTLDepthStencilState!
  package var skyDepthState: MTLDepthStencilState!
  package var shadowTexture: MTLTexture!
  package var shadowTexelsPerMetre: Float = 1
  package var frame = 0
  package var gpuTimes: [Double] = []
  package var gpuPhaseTimes: [String: [Double]] = [:]
  package var gpuPassTimes: [String: [Double]] = [:]
  package var profiling = false
  package var metricsGeneration = 0
  package var cpuTimes: [Double] = []
  package var simulationTimes: [Double] = []
  package var atmosphereCPUTimes: [Double] = []
  package var completionTimes: [Double] = []
  package var presentedIntervals: [Double] = []
  package var lastPresentedTime: Double = 0
  package var presentedFrames = 0
  package var frameTimes: [Double] = []
  package var drawableWaitTimes: [Double] = []
  package var visibleTriangles = 0
  package var shadowTriangles = 0
  private var cullMatrix = matrix_identity_float4x4
  private var measuredScene = ""
  package var shaderDigest = ""
  package var gpuErrors: [String] = []
  package var captureRequest: ((URL, [String: Any], (Result<URL, Error>) -> Void))?
  private let inFlight = DispatchSemaphore(value: 3)
  private var drawableFrames = 0

  package var metadataProvider: (() -> [String: Any])?
  private var scene = ""
  private var camera = V3.zero
  private var wind: Float = 1
  package let atmosphericRendering:Bool
  package var materialSourceURL:URL?
  package init(view: MTKView, materialSourceURL:URL? = nil, atmosphericRendering:Bool = true) throws {
    self.atmosphericRendering=atmosphericRendering
    self.materialSourceURL=materialSourceURL
    guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
      throw RuntimeError.message("Metal is unavailable")
    }
    self.device = device
    self.queue = queue
    self.view = view
    view.device = device
    view.colorPixelFormat = .bgra8Unorm_srgb
    view.sampleCount = 4
    view.depthStencilPixelFormat = .depth32Float
    view.framebufferOnly = false
    view.autoResizeDrawable = false
    view.drawableSize = CGSize(width: 1920, height: 1080)
    view.preferredFramesPerSecond = 60
    view.clearColor = MTLClearColor(red: 0.4, green: 0.6, blue: 0.65, alpha: 1)
    try buildPipelines()
    windBuffers = (0..<3).map { _ in
      device.makeBuffer(length: 1024 * 16, options: .storageModeShared)!
    }
  }
  package func upload(_ batch: SceneBatch) -> GPUBatch {
    if !batch.grass.isEmpty { return uploadGrass(batch) }
    precondition(
      batch.instances.allSatisfy { $0.tint.w == batch.instances.first?.tint.w },
      "Each draw batch must have one material")
    precondition(batch.skinJoints.isEmpty || (batch.skinJoints.count <= 64 && batch.lodMeshes.isEmpty
      && batch.skinWeights.count == batch.mesh.vertices.count
      && batch.skinWeights.allSatisfy { $0.validate(count: batch.skinJoints.count) }), "Invalid skin binding")
    let vertices = device.makeBuffer(
      bytes: batch.mesh.vertices, length: MemoryLayout<Vertex>.stride * batch.mesh.vertices.count)!
    let indices = device.makeBuffer(
      bytes: batch.mesh.indices, length: MemoryLayout<UInt32>.stride * batch.mesh.indices.count)!
    let instances = device.makeBuffer(
      bytes: batch.instances, length: MemoryLayout<Instance>.stride * batch.instances.count)!
    vertices.label = batch.name + " vertices"
    indices.label = batch.name + " indices"
    let points = batch.mesh.vertices.map { V3($0.position.x, $0.position.y, $0.position.z) }
    let lower = points.reduce(V3(repeating: Float.greatestFiniteMagnitude), simd_min)
    let upper = points.reduce(V3(repeating: -Float.greatestFiniteMagnitude), simd_max)
    let center = (lower + upper) / 2
    let radius = length(upper - lower) / 2 + 1.0
    let buffers = (0..<6).map { _ in
      device.makeBuffer(
        length: MemoryLayout<UInt32>.stride * max(1, batch.instances.count),
        options: .storageModeShared)!
    }
    let lods = batch.lodMeshes.map { mesh -> GPUBatch in
      var lod = SceneBatch(name: batch.name, mesh: mesh, instances: batch.instances)
      lod.doubleSided = batch.doubleSided
      lod.roughness = batch.roughness
      lod.metallic = batch.metallic
      return upload(lod)
    }
    let clusters = MeshletData(batch.mesh)
    struct Cluster {
      var ranges: SIMD4<UInt32>
      var sphere: SIMD4<Float>
    }
    let descriptors = zip(clusters.descriptors, clusters.spheres).map {
      Cluster(ranges: $0.0, sphere: $0.1)
    }
    return GPUBatch(
      name: batch.name, vertices: vertices, indices: indices, instances: instances,
      indexCount: batch.mesh.indices.count, instanceCount: batch.instances.count,
      sourceInstances: batch.instances, visibleBuffers: buffers,
      selection: BatchSelection(
        instances: batch.instances.count, levels: batch.lodMeshes.count + 1), center: center,
      radius: radius, lods: lods, error: batch.mesh.geometricError,
      meshlets: device.makeBuffer(bytes: descriptors, length: descriptors.count * 32)!,
      meshletVertices: device.makeBuffer(
        bytes: clusters.vertices, length: clusters.vertices.count * 4)!,
      meshletTriangles: device.makeBuffer(
        bytes: clusters.triangles, length: clusters.triangles.count)!,
      material: SIMD4(batch.roughness, batch.metallic, 0, 0), meshletCount: descriptors.count,
      doubleSided: batch.doubleSided,hasGroomCoverage:batch.mesh.vertices.contains{$0.groom.z>0},
      skinWeights: batch.skinWeights.isEmpty ? nil : device.makeBuffer(bytes: batch.skinWeights,
        length: batch.skinWeights.count * MemoryLayout<SkinWeight>.stride), skinJoints: batch.skinJoints)
  }

  package func uploadGrass(_ batch: SceneBatch) -> GPUBatch {
    // Stable spatial ordering keeps each 12-blade patch compact. No blade
    // is dropped at any distance; this changes storage and launch work only.
    let blades = batch.grass.enumerated().sorted {
      func cell(_ b: GrassBlade) -> Int {
        let z = Int(((b.anchorAngle.z + 160) / 4).rounded(.down))
        let x = Int(((b.anchorAngle.x + 160) / 4).rounded(.down))
        return z * 80 + x
      }
      let a = cell($0.element)
      let b = cell($1.element)
      return a == b ? $0.offset < $1.offset : a < b
    }.map(\.element)
    struct Patch {
      var ranges: SIMD4<UInt32>
      var sphere: SIMD4<Float>
    }
    var patches: [Patch] = []
    var allBounds = Bounds(
      V3(repeating: .greatestFiniteMagnitude), V3(repeating: -Float.greatestFiniteMagnitude))
    for start in stride(from: 0, to: blades.count, by: 12) {
      let count = min(12, blades.count - start)
      var lower = V3(repeating: Float.greatestFiniteMagnitude)
      var upper = -lower
      for blade in blades[start..<start + count] {
        for v in blade.vertices {
          let p = V3(v.position.x, v.position.y, v.position.z)
          lower = simd_min(lower, p)
          upper = simd_max(upper, p)
        }
      }
      let center = (lower + upper) / 2
      patches.append(
        Patch(
          ranges: SIMD4(UInt32(start), 0, UInt32(count), UInt32(count * 3)),
          sphere: SIMD4(center, length(upper - lower) / 2 + 0.0001)))
      allBounds = allBounds.union(Bounds(lower, upper))
    }
    let descriptor = device.makeBuffer(
      bytes: blades, length: blades.count * MemoryLayout<GrassBlade>.stride)!
    descriptor.label = "Grass blade descriptors (32 bytes)"
    let dummy = device.makeBuffer(length: 4)!
    let instances = device.makeBuffer(
      bytes: batch.instances, length: batch.instances.count * MemoryLayout<Instance>.stride)!
    return GPUBatch(
      name: batch.name, vertices: descriptor, indices: dummy, instances: instances,
      indexCount: blades.count * 9, instanceCount: batch.instances.count,
      sourceInstances: batch.instances,
      visibleBuffers: (0..<6).map { _ in
        device.makeBuffer(length: 4 * max(1, batch.instances.count), options: .storageModeShared)!
      }, selection: BatchSelection(instances: batch.instances.count, levels: 1),
      center: (allBounds.min + allBounds.max) / 2,
      radius: length(allBounds.max - allBounds.min) / 2 + 1, lods: [], error: 0,
      meshlets: device.makeBuffer(bytes: patches, length: patches.count * 32)!,
      meshletVertices: dummy, meshletTriangles: dummy, meshletCount: patches.count,
      doubleSided: batch.doubleSided, proceduralGrass: true)
  }

  package func buildPipelines(sourceURL: URL? = nil) throws {
    var source = try String(
      contentsOf: sourceURL ?? Bundle.main.url(forResource: "Surface", withExtension: "metal")
        ?? Bundle.module.url(forResource: "Surface", withExtension: "metal")!, encoding: .utf8)
    let atmosphereURL =
      sourceURL?.deletingLastPathComponent().appendingPathComponent("Atmosphere.metal") ?? Bundle
      .main.url(forResource: "Atmosphere", withExtension: "metal") ?? Bundle.module.url(
        forResource: "Atmosphere", withExtension: "metal")!
    source = source.replacingOccurrences(
      of: "// ATMOSPHERE", with: try String(contentsOf: atmosphereURL, encoding: .utf8))
    let surfaces = try materialSourceURL.map {try String(contentsOf:$0,encoding:.utf8)} ?? "void projectSurface(int kind,float3 local,float3 world,float3 n,thread float3 &color,thread float &rough,thread float &bump,thread float &strength) {}"
    source = source.replacingOccurrences(of:"// PROJECT_SURFACES",with:surfaces)
    let library = try device.makeLibrary(source: source, options: nil)
    func fragment(_ kind: Int, _ name: String = "surfaceFragment", layers: Bool = false, groom:Bool = false) throws -> MTLFunction {
      let constants = MTLFunctionConstantValues()
      var k = Int32(kind)
      constants.setConstantValue(&k, type: .int, index: 0)
      var enabled = layers
      constants.setConstantValue(&enabled, type: .bool, index: 1)
      var covered=groom
      constants.setConstantValue(&covered,type:.bool,index:2)
      return try library.makeFunction(name: name, constantValues: constants)
    }
    let genericFragment = try fragment(-1)
    let d = MTLRenderPipelineDescriptor()
    d.vertexFunction = library.makeFunction(name: "surfaceVertex")
    d.fragmentFunction = genericFragment
    d.rasterSampleCount = view.sampleCount
    d.colorAttachments[0].pixelFormat = .rgba16Float
    d.depthAttachmentPixelFormat = .depth32Float
    let newPipeline = try device.makeRenderPipelineState(descriptor: d)
    d.vertexFunction = library.makeFunction(name: "skyVertex")
    d.fragmentFunction = library.makeFunction(name: "skyFragment")
    let newSkyPipeline = try device.makeRenderPipelineState(descriptor: d)
    d.fragmentFunction = try fragment(7, "groundFragment")
    let newGroundPipeline = try device.makeRenderPipelineState(descriptor: d)
    let sd = MTLRenderPipelineDescriptor()
    sd.vertexFunction = library.makeFunction(name: "shadowVertex")
    sd.depthAttachmentPixelFormat = .depth32Float
    let newShadowPipeline = try device.makeRenderPipelineState(descriptor: sd)
    sd.vertexFunction=library.makeFunction(name:"groomShadowVertex")
    sd.fragmentFunction=library.makeFunction(name:"groomShadowFragment")
    let newGroomShadowPipeline=try device.makeRenderPipelineState(descriptor:sd)
    let md = MTLMeshRenderPipelineDescriptor()
    md.objectFunction = library.makeFunction(name: "surfaceObject")
    md.maxTotalThreadsPerObjectThreadgroup = 32
    md.payloadMemoryLength = 132
    md.meshFunction = library.makeFunction(name: "surfaceMesh")
    md.fragmentFunction = genericFragment
    md.maxTotalThreadsPerMeshThreadgroup = 64
    md.rasterSampleCount = view.sampleCount
    md.colorAttachments[0].pixelFormat = .rgba16Float
    md.depthAttachmentPixelFormat = .depth32Float
    let (newMeshPipeline, _) = try device.makeRenderPipelineState(descriptor: md, options: [])
    // Compile finish fields out of ordinary scene draws. A uniform zero-length
    // loop still imposes the complex material function's register pressure.
    d.vertexFunction = library.makeFunction(name: "surfaceVertex")
    d.fragmentFunction = try fragment(-1, layers: true)
    md.fragmentFunction = d.fragmentFunction
    let newLayeredPipeline = try device.makeRenderPipelineState(descriptor: d)
    let newLayeredMeshPipeline = try device.makeRenderPipelineState(descriptor: md, options: []).0
    d.isAlphaToCoverageEnabled=true;md.isAlphaToCoverageEnabled=true
    d.fragmentFunction=try fragment(-1,groom:true);md.fragmentFunction=d.fragmentFunction
    let newGroomPipeline=try device.makeRenderPipelineState(descriptor:d)
    let newGroomMeshPipeline=try device.makeRenderPipelineState(descriptor:md,options:[]).0
    d.fragmentFunction=try fragment(-1,layers:true,groom:true);md.fragmentFunction=d.fragmentFunction
    let newLayeredGroomPipeline=try device.makeRenderPipelineState(descriptor:d)
    let newLayeredGroomMeshPipeline=try device.makeRenderPipelineState(descriptor:md,options:[]).0
    d.isAlphaToCoverageEnabled=false;md.isAlphaToCoverageEnabled=false
    var specialized: [Int: MTLRenderPipelineState] = [:]
    var specializedMesh: [Int: MTLRenderPipelineState] = [:]
    d.vertexFunction = library.makeFunction(name: "surfaceVertex")
    for kind in 0...9 {
      let f = try fragment(kind)
      d.fragmentFunction = f
      md.fragmentFunction = f
      specialized[kind] = try device.makeRenderPipelineState(descriptor: d)
      specializedMesh[kind] = try device.makeRenderPipelineState(descriptor: md, options: []).0
    }
    md.meshFunction = library.makeFunction(name: "grassMesh")
    md.fragmentFunction = try fragment(3)
    let newGrassMesh = try device.makeRenderPipelineState(descriptor: md, options: []).0
    d.vertexFunction = library.makeFunction(name: "grassVertex")
    d.fragmentFunction = try fragment(3)
    let newGrassVertex = try device.makeRenderPipelineState(descriptor: d)
    d.fragmentFunction = try fragment(3, layers: true)
    md.fragmentFunction = d.fragmentFunction
    let newLayeredGrassPipeline = try device.makeRenderPipelineState(descriptor: d)
    let newLayeredGrassMeshPipeline = try device.makeRenderPipelineState(descriptor: md, options: []).0
    let newAtmosphere = try Atmosphere(device: device, library: library, enabled:atmosphericRendering)
    let newPost = try PostProcess(device: device, library: library)
    grassMeshPipeline = newGrassMesh
    grassVertexPipeline = newGrassVertex
    materialPipelines = specialized
    materialMeshPipelines = specializedMesh
    layeredPipeline = newLayeredPipeline
    layeredMeshPipeline = newLayeredMeshPipeline
    layeredGrassPipeline = newLayeredGrassPipeline
    layeredGrassMeshPipeline = newLayeredGrassMeshPipeline
    post = newPost
    atmosphere = newAtmosphere
    meshPipeline = newMeshPipeline
    pipeline = newPipeline
    skyPipeline = newSkyPipeline
    groundPipeline = newGroundPipeline
    shadowPipeline = newShadowPipeline
    groomShadowPipeline=newGroomShadowPipeline
    groomPipeline=newGroomPipeline;groomMeshPipeline=newGroomMeshPipeline
    layeredGroomPipeline=newLayeredGroomPipeline;layeredGroomMeshPipeline=newLayeredGroomMeshPipeline
    shaderDigest = SHA256.hash(data: Data(source.utf8)).map { String(format: "%02x", $0) }.joined()
    let depth = MTLDepthStencilDescriptor()
    depth.depthCompareFunction = .less
    depth.isDepthWriteEnabled = true
    depthState = device.makeDepthStencilState(descriptor: depth)
    depth.depthCompareFunction = .lessEqual
    depth.isDepthWriteEnabled = false
    skyDepthState = device.makeDepthStencilState(descriptor: depth)
    let texture = MTLTextureDescriptor.texture2DDescriptor(
      pixelFormat: .depth32Float, width: 2048, height: 2048, mipmapped: false)
    texture.usage = [.renderTarget, .shaderRead]
    texture.storageMode = .private
    shadowTexture = device.makeTexture(descriptor: texture)
  }

  package func draw(_ input: RenderFrame, elapsed: Double, simulationMilliseconds: Double) {
    scene = input.id
    camera = input.camera
    wind = input.wind
    if measuredScene != scene {
      clearMetrics()
      measuredScene = scene
    }
    if frame > 3 {
      frameTimes.append(elapsed * 1000)
      if frameTimes.count > 3600 { frameTimes.removeFirst() }
    }
    simulationTimes.append(simulationMilliseconds)
    if simulationTimes.count > 3600 { simulationTimes.removeFirst() }
    guard inFlight.wait(timeout: .now()) == .success else { return }
    let drawableStart = CACurrentMediaTime()
    guard let drawable = view.currentDrawable, let pass = view.currentRenderPassDescriptor,
      let cb = queue.makeCommandBuffer()
    else {
      inFlight.signal()
      return
    }
    let encodeStart = CACurrentMediaTime()
    drawableWaitTimes.append((encodeStart - drawableStart) * 1000)
    if drawableWaitTimes.count > 3600 { drawableWaitTimes.removeFirst() }
    drawableFrames += 1
    var uniforms = input.uniforms
    let vp = uniforms.viewProjection
    let lvp = uniforms.lightVP
    let eye = V3(uniforms.cameraTime.x, uniforms.cameraTime.y, uniforms.cameraTime.z)
    let sun = V3(uniforms.sunExposure.x, uniforms.sunExposure.y, uniforms.sunExposure.z)
    let lightSize = input.lightSize
    let paused = input.paused
    let sceneLook = input.look
    let atmosphereWind = input.windState
    let profile = profiling && frame % 5 == 0 ? GPUProfile(device) : nil
    let atmosphereStart = CACurrentMediaTime()
    atmosphere.encode(cb, &uniforms, paused: paused, profile: profile)
    atmosphereCPUTimes.append((CACurrentMediaTime() - atmosphereStart) * 1000)
    if atmosphereCPUTimes.count > 3600 { atmosphereCPUTimes.removeFirst() }
    let lightState = post.encodeLighting(
      cb, uniforms: &uniforms, atmosphere: atmosphere, index: frame % 3, profile: profile)
    shadowTexelsPerMetre = 1024 / lightSize
    let windBuffer = windBuffers[frame % 3]
    atmosphereWind.writeGPUCells(
      to: UnsafeMutableBufferPointer(
        start: windBuffer.contents().assumingMemoryBound(to: SIMD4<Float>.self), count: 1024))
    let shadow = MTLRenderPassDescriptor()
    shadow.depthAttachment.texture = shadowTexture
    shadow.depthAttachment.loadAction = .clear
    shadow.depthAttachment.storeAction = .store
    shadow.depthAttachment.clearDepth = 1
    shadowTriangles = 0
    if !input.items.isEmpty {
      profile?.render(shadow, "shadow")
      if let enc = cb.makeRenderCommandEncoder(descriptor: shadow) {
        enc.label = "Sun shadows"
        enc.setRenderPipelineState(shadowPipeline)
        enc.setDepthStencilState(depthState)
        enc.setDepthBias(1, slopeScale: 1, clamp: 0.001)
        enc.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        enc.setVertexBuffer(windBuffer, offset: 0, index: 3)
        cullMatrix = lvp
        shadowTriangles = 0
        renderObjects(input.items, enc, shadow: true)
        enc.endEncoding()
      }
    }
    pass.colorAttachments[0].texture = post.multisample
    pass.colorAttachments[0].resolveTexture = post.hdr
    pass.colorAttachments[0].storeAction = .multisampleResolve
    profile?.render(pass, "scene")
    if let enc = cb.makeRenderCommandEncoder(descriptor: pass) {
      enc.label = "Field surfaces"
      enc.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
      enc.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
      enc.setFragmentBuffer(lightState, offset: 0, index: 2)
      enc.setFragmentTexture(shadowTexture, index: 0)
      enc.setFragmentTexture(atmosphere.sky, index: 1)
      enc.setFragmentTexture(atmosphere.irradiance, index: 2)
      enc.setFragmentTexture(atmosphere.trans, index: 3)
      enc.setFragmentTexture(atmosphere.previousSky, index: 4)
      enc.setFragmentTexture(atmosphere.previousIrradiance, index: 5)
      enc.setFragmentTexture(atmosphere.clearSky, index: 6)
      enc.setVertexBuffer(windBuffer, offset: 0, index: 3)
      enc.setObjectBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
      enc.setMeshBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
      enc.setMeshBuffer(windBuffer, offset: 0, index: 3)
      enc.setRenderPipelineState(useMeshlets ? meshPipeline : pipeline)
      enc.setDepthStencilState(depthState)
      cullMatrix = vp
      visibleTriangles = 0
      renderObjects(input.items, enc, shadow: false)
      if input.infiniteGround {
        var material = SIMD4<Float>(-1, 0, 0, 2)
        enc.setFragmentBytes(&material, length: MemoryLayout<SIMD4<Float>>.stride, index: 4)
        enc.setRenderPipelineState(groundPipeline)
        enc.setDepthStencilState(depthState)
        enc.setCullMode(.none)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
      }
      enc.setRenderPipelineState(skyPipeline)
      enc.setDepthStencilState(skyDepthState)
      enc.setCullMode(.none)
      enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
      enc.endEncoding()
    }
    post.encode(
      cb, to: drawable.texture, lightState: lightState, look: sceneLook.grade, profile: profile)
    var pending: (URL, [String: Any], (Result<URL, Error>) -> Void, MTLBuffer, Int)?
    if let request = captureRequest {
      captureRequest = nil
      let row = 1920 * 4
      if let buffer = device.makeBuffer(length: row * 1080, options: .storageModeShared),
        let blit = cb.makeBlitCommandEncoder()
      {
        blit.copy(
          from: drawable.texture, sourceSlice: 0, sourceLevel: 0,
          sourceOrigin: MTLOrigin(x: 0, y: 0, z: 0),
          sourceSize: MTLSize(width: 1920, height: 1080, depth: 1), to: buffer,
          destinationOffset: 0, destinationBytesPerRow: row, destinationBytesPerImage: row * 1080)
        var metadata = metadataProvider?() ?? [:]
        metadata["frame"] = frame + 1
        metadata["captureIncludesHUD"] = false
        let sunClip = vp * SIMD4(eye + sun * 100, 1)
        if sunClip.w > 0 {
          metadata["projectedSunPixels"] = [
            (sunClip.x / sunClip.w * 0.5 + 0.5) * 1920, (0.5 - sunClip.y / sunClip.w * 0.5) * 1080,
          ]
        }
        blit.endEncoding()
        pending = (request.0, metadata, request.2, buffer, row)
      } else {
        request.2(.failure(RuntimeError.message("Unable to allocate capture buffer")))
      }
    }
    let presentationGeneration = metricsGeneration
    drawable.addPresentedHandler { [weak self] shown in
      let timestamp = shown.presentedTime
      DispatchQueue.main.async {
        guard let self, self.metricsGeneration == presentationGeneration, timestamp > 0 else {
          return
        }
        self.presentedFrames += 1
        if self.lastPresentedTime > 0 && timestamp > self.lastPresentedTime {
          self.presentedIntervals.append((timestamp - self.lastPresentedTime) * 1000)
          if self.presentedIntervals.count > 3600 { self.presentedIntervals.removeFirst() }
        }
        self.lastPresentedTime = max(self.lastPresentedTime, timestamp)
      }
    }
    cb.present(drawable)
    let semaphore = inFlight
    let capture = pending
    let submittedScene = scene
    let submittedPhase = atmosphere.updatePhase
    let submittedGeneration = metricsGeneration
    let submissionTime = CACurrentMediaTime()
    cb.addCompletedHandler { [weak self] command in
      semaphore.signal()
      let completion = (CACurrentMediaTime() - submissionTime) * 1000
      let ms = (command.gpuEndTime - command.gpuStartTime) * 1000
      let passes = profile?.resolve() ?? [:]
      DispatchQueue.main.async {
        guard let self else { return }
        if let error = command.error { self.gpuErrors.append(error.localizedDescription) }
        if ms > 0 && self.scene == submittedScene && self.metricsGeneration == submittedGeneration {
          for (name, duration) in passes {
            self.gpuPassTimes[name, default: []].append(duration)
            if self.gpuPassTimes[name]!.count > 512 { self.gpuPassTimes[name]!.removeFirst() }
          }
          self.completionTimes.append(completion)
          if self.completionTimes.count > 3600 { self.completionTimes.removeFirst() }
          self.gpuTimes.append(ms)
          if self.gpuTimes.count > 3600 { self.gpuTimes.removeFirst() }
          self.gpuPhaseTimes[submittedPhase, default: []].append(ms)
          if self.gpuPhaseTimes[submittedPhase]!.count > 512 {
            self.gpuPhaseTimes[submittedPhase]!.removeFirst()
          }
        }
        if let p = capture {
          if let error = command.error {
            p.2(.failure(error))
          } else {
            self.writeCapture(p.0, p.1, p.2, p.3, p.4)
          }
        }
      }
    }
    cb.commit()
    frame += 1
    cpuTimes.append((CACurrentMediaTime() - encodeStart) * 1000)
    if cpuTimes.count > 3600 { cpuTimes.removeFirst() }
  }

  package func renderObjects(_ items: [RenderItem], _ enc: MTLRenderCommandEncoder, shadow: Bool) {
    let rows = cullMatrix.transpose
    var planes = [
      rows[3] + rows[0], rows[3] - rows[0], rows[3] + rows[1], rows[3] - rows[1], rows[2],
      rows[3] - rows[2],
    ].map { $0 / max(length(V3($0.x, $0.y, $0.z)), 1e-20) }
    if !shadow && useMeshlets { enc.setObjectBytes(&planes, length: 6 * 16, index: 8) }
    enc.setFrontFacing(.counterClockwise)
    func draw(
      _ b: GPUBatch, instance: Instance? = nil, selectedCount: Int? = nil,
      override: SIMD4<Float>? = nil, skinPalette: [simd_float4x4] = [], layers:[SurfaceLayer] = [], correctives:[CorrectiveUniform] = []
    ) {
      if instance == nil && selectedCount == nil {
        let levels = [b] + b.lods
        let state = b.selection
        let slot = (frame % 3) * 2 + (shadow ? 1 : 0)
        for k in state.counts.indices { state.counts[k] = 0 }
        for (index, inst) in b.sourceInstances.enumerated() {
          let center = inst.model * SIMD4(b.center, 1)
          let scale = max(length(inst.model[0]), max(length(inst.model[1]), length(inst.model[2])))
          let kind = inst.tint.w
          let top = max(0, b.center.y + b.radius)
          let deformation: Float =
            (kind == 2 || kind == 4 || kind == 8)
            ? 0.35 * sqrt(2) * abs(wind) * pow(top / 5, 2) * 0.75
            : (kind == 3 ? 0.35 * sqrt(2) * abs(wind) * 0.65 : 0)
          let radius = b.radius * scale + deformation
          guard !skinPalette.isEmpty || !correctives.isEmpty || planes.allSatisfy({ dot($0, center) >= -radius }) else { continue }
          let distance = max(
            1, length(V3(center.x, center.y, center.z) - camera) - b.radius * scale)
          let current = state.lod[index]
          var selected = 0
          for k in 1..<levels.count {
            let delta = levels[k].error
            var error = delta * scale
            if kind == 8 {
              // response components are bounded by ±0.35; bilinear
              // 10 m cells have Jacobian norm at most 0.14/m.
              // Bound both changing response and y² bend weight.
              let amplitude = 0.35 * sqrt(2) * abs(wind)
              let slope = 0.14 * abs(wind)
              error +=
                amplitude * 0.03 * delta * (2 * top + delta) + 0.03 * pow(top + delta, 2) * slope
                * scale * delta
            }
            if shadow
              ? error * shadowTexelsPerMetre < 0.8
              : error * 935 / distance < (k == current ? 0.65 : 0.4)
            {
              selected = k
            }
          }
          if !shadow { state.lod[index] = selected }
          let ids = levels[selected].visibleBuffers[slot].contents().assumingMemoryBound(
            to: UInt32.self)
          ids[state.counts[selected]] = UInt32(index)
          state.counts[selected] += 1
        }
        // Keep the previous draw order for stable depth ties.
        for k in levels.indices.dropFirst() {
          draw(levels[k], selectedCount: state.counts[k], override: override, skinPalette: skinPalette,layers:layers,correctives:correctives)
        }
        draw(b, selectedCount: state.counts[0], override: override, skinPalette: skinPalette,layers:layers,correctives:correctives)
        return
      }
      if let selectedCount, selectedCount == 0 { return }
      if shadow {enc.setRenderPipelineState(b.hasGroomCoverage ? groomShadowPipeline:shadowPipeline)}
      if !shadow {
        var material = override ?? b.material
        enc.setFragmentBytes(&material, length: MemoryLayout<SIMD4<Float>>.stride, index: 4)
        let kind = Int(instance?.tint.w ?? b.sourceInstances.first?.tint.w ?? 0)
        let selected:MTLRenderPipelineState
        if b.hasGroomCoverage {
          if layers.isEmpty {selected=useMeshlets ? groomMeshPipeline:groomPipeline}
          else {selected=useMeshlets ? layeredGroomMeshPipeline:layeredGroomPipeline}
        } else if b.proceduralGrass {
          if layers.isEmpty {selected=useMeshlets ? grassMeshPipeline:grassVertexPipeline}
          else {selected=useMeshlets ? layeredGrassMeshPipeline:layeredGrassPipeline}
        } else if !layers.isEmpty {
          selected=useMeshlets ? layeredMeshPipeline:layeredPipeline
        } else {
          selected=useMeshlets ? (materialMeshPipelines[kind] ?? meshPipeline):(materialPipelines[kind] ?? pipeline)
        }
        enc.setRenderPipelineState(selected)
      }
      if !shadow {
        precondition(layers.count<=16 && MemoryLayout<SurfaceLayerUniform>.stride==64)
        var layerCount=UInt32(layers.count)
        var uniforms=layers.map(\.uniform)
        if uniforms.isEmpty {uniforms=[SurfaceLayer(id:"empty",part:"empty",center:.zero,radius:V3(repeating:1),tint:.zero).uniform]}
        enc.setFragmentBytes(&uniforms,length:uniforms.count*64,index:5)
        enc.setFragmentBytes(&layerCount,length:4,index:6)
      }
      precondition(correctives.count<=16 && MemoryLayout<CorrectiveUniform>.stride==64)
      var correctiveCount=UInt32(correctives.count)
      var correctiveData=correctives.isEmpty ? [CorrectiveUniform(SurfaceEdit(id:"empty",part:"empty",center:.zero,radius:V3(repeating:1)),weight:0)]:correctives
      enc.setVertexBytes(&correctiveData,length:correctiveData.count*64,index:13)
      enc.setVertexBytes(&correctiveCount,length:4,index:14)
      if !shadow && useMeshlets {
        enc.setMeshBytes(&correctiveData,length:correctiveData.count*64,index:13)
        enc.setMeshBytes(&correctiveCount,length:4,index:14)
      }
      let skinning = SkinningPalette(skinPalette)
      var skinCount = skinning.encodedCount
      precondition(skinPalette.isEmpty || skinPalette.count == b.skinJoints.count)
      var palette = skinPalette.isEmpty ? [matrix_identity_float4x4] : skinning.encodedMatrices
      enc.setVertexBuffer(b.skinWeights ?? b.vertices, offset: 0, index: 10)
      enc.setVertexBytes(&palette, length: palette.count * 64, index: 11)
      enc.setVertexBytes(&skinCount, length: 4, index: 12)
      if !shadow && useMeshlets {
        var cullingDeformed = max(skinCount,correctiveCount)
        enc.setObjectBytes(&cullingDeformed, length: 4, index: 12)
        enc.setMeshBuffer(b.skinWeights ?? b.vertices, offset: 0, index: 10)
        enc.setMeshBytes(&palette, length: palette.count * 64, index: 11)
        enc.setMeshBytes(&skinCount, length: 4, index: 12)
      }
      var count = 1
      enc.setVertexBuffer(b.vertices, offset: 0, index: 0)
      if !shadow && useMeshlets { enc.setMeshBuffer(b.vertices, offset: 0, index: 0) }
      if var instance {
        var id: UInt32 = 0
        enc.setVertexBytes(&instance, length: MemoryLayout<Instance>.stride, index: 2)
        enc.setVertexBytes(&id, length: 4, index: 7)
        if !shadow && useMeshlets {
          enc.setObjectBytes(&instance, length: MemoryLayout<Instance>.stride, index: 2)
          enc.setMeshBytes(&instance, length: MemoryLayout<Instance>.stride, index: 2)
          enc.setObjectBytes(&id, length: 4, index: 7)
        }
      } else {
        count = selectedCount ?? 0
        let buffer = b.visibleBuffers[(frame % 3) * 2 + (shadow ? 1 : 0)]
        enc.setVertexBuffer(b.instances, offset: 0, index: 2)
        enc.setVertexBuffer(buffer, offset: 0, index: 7)
        if !shadow && useMeshlets {
          enc.setObjectBuffer(b.instances, offset: 0, index: 2)
          enc.setMeshBuffer(b.instances, offset: 0, index: 2)
          enc.setObjectBuffer(buffer, offset: 0, index: 7)
        }
      }
      enc.setCullMode(
        b.doubleSided ? .none : .back)
      if !shadow && useMeshlets {
        var meshletCount = UInt32(b.meshletCount)
        enc.setObjectBytes(&meshletCount, length: 4, index: 9)
        enc.setObjectBuffer(b.meshlets, offset: 0, index: 4)
        enc.setMeshBuffer(b.meshlets, offset: 0, index: 4)
        enc.setMeshBuffer(b.meshletVertices, offset: 0, index: 5)
        enc.setMeshBuffer(b.meshletTriangles, offset: 0, index: 6)
        enc.drawMeshThreadgroups(
          MTLSize(width: (b.meshletCount + 31) / 32, height: count, depth: 1),
          threadsPerObjectThreadgroup: MTLSize(width: 32, height: 1, depth: 1),
          threadsPerMeshThreadgroup: MTLSize(width: 64, height: 1, depth: 1))
      } else if b.proceduralGrass {
        enc.drawPrimitives(
          type: .triangle, vertexStart: 0, vertexCount: b.indexCount, instanceCount: count)
      } else {
        enc.drawIndexedPrimitives(
          type: .triangle, indexCount: b.indexCount, indexType: .uint32, indexBuffer: b.indices,
          indexBufferOffset: 0, instanceCount: count)
      }
      if shadow {
        shadowTriangles += b.indexCount / 3 * count
      } else {
        visibleTriangles += b.indexCount / 3 * count
      }
    }
    for item in items where !shadow || item.castsShadow {
      draw(item.batch, instance: item.instance, override: item.material, skinPalette: item.skinPalette,layers:item.surfaceLayers,correctives:item.correctives)
    }
  }

  package func writeCapture(
    _ url: URL, _ metadata: [String: Any], _ completion: (Result<URL, Error>) -> Void,
    _ buffer: MTLBuffer, _ row: Int
  ) {
    do {
      let bytes = buffer.contents().bindMemory(to: UInt8.self, capacity: row * 1080)
      guard
        let bitmap = NSBitmapImageRep(
          bitmapDataPlanes: nil, pixelsWide: 1920, pixelsHigh: 1080, bitsPerSample: 8,
          samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
          bytesPerRow: row, bitsPerPixel: 32)
      else { throw RuntimeError.message("Capture bitmap allocation failed") }
      let out = bitmap.bitmapData!
      for i in stride(from: 0, to: row * 1080, by: 4) {
        out[i] = bytes[i + 2]
        out[i + 1] = bytes[i + 1]
        out[i + 2] = bytes[i]
        out[i + 3] = 255
      }
      guard let png = bitmap.representation(using: .png, properties: [:]) else {
        throw RuntimeError.message("PNG encoding failed")
      }
      try png.write(to: url, options: .atomic)
      try JSONSerialization.data(withJSONObject: metadata, options: [.prettyPrinted, .sortedKeys])
        .write(to: url.deletingPathExtension().appendingPathExtension("json"), options: .atomic)
      completion(.success(url))
    } catch { completion(.failure(error)) }
  }

  package func stats(_ values: [Double]) -> [String: Double] {
    guard !values.isEmpty else { return [:] }
    let sorted = values.sorted()
    func percentile(_ p: Double) -> Double {
      sorted[min(sorted.count - 1, Int(Double(sorted.count - 1) * p))]
    }
    return [
      "median": percentile(0.5), "p95": percentile(0.95), "p99": percentile(0.99),
      "max": sorted.last!,
    ]
  }
  package func clearMetrics() {
    metricsGeneration += 1
    gpuPassTimes = [:]
    gpuPhaseTimes = [:]
    gpuTimes = []
    cpuTimes = []
    simulationTimes = []
    atmosphereCPUTimes = []
    completionTimes = []
    presentedIntervals = []
    lastPresentedTime = 0
    presentedFrames = 0
    frameTimes = []
    drawableWaitTimes = []
  }
}
package enum RuntimeError: LocalizedError {
  case message(String)
  package var errorDescription: String? {
    if case .message(let s) = self { return s }
    return nil
  }
}
