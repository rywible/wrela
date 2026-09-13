import FieldCore
import MetalKit

/// Values that affect cache construction. Camera, exposure, wetness and scene
/// grading deliberately do not invalidate the cache. Time is frozen per cycle.
package struct SkyCacheSignature: Hashable, Sendable {
  package let speed: Float
  package let preset: Float
  package let sun: SIMD3<Float>
  package let sky: SIMD4<Float>
  package init(_ u: Uniforms) {
    speed = u.options.y; preset = u.environment.x
    sun = SIMD3(u.sunExposure.x, u.sunExposure.y, u.sunExposure.z); sky = u.sky
  }
  package func applyLighting(to u: inout Uniforms) {
    u.sunExposure.x = sun.x; u.sunExposure.y = sun.y; u.sunExposure.z = sun.z
    u.sky = sky; u.options.y = speed; u.environment.x = preset
  }
  package var density: SIMD3<Float> { SIMD3(sky.x, sky.y, sky.w) }
  package var diagnostic: [String: Any] {
    ["speed": speed, "preset": preset, "sun": [sun.x, sun.y, sun.z],
     "sky": [sky.x, sky.y, sky.z, sky.w]]
  }
}

/// CPU-only publication gate, also exercised without a Metal device in tests.
/// Completion may advance/publish only the most recently requested generation.
package struct SkyPublicationGate {
  package private(set) var desired: SkyCacheSignature?
  package private(set) var desiredGeneration: UInt64 = 0
  package private(set) var activeGeneration: UInt64 = 0
  package private(set) var publications = 0
  package private(set) var retired = 0
  package mutating func request(_ signature: SkyCacheSignature, invalidate: Bool = false) {
    if desired != signature || invalidate {
      desired = signature; desiredGeneration &+= 1
    }
  }
  package mutating func complete(_ generation: UInt64, success: Bool, final: Bool) -> Bool {
    guard success, generation == desiredGeneration else { retired += 1; return false }
    if final { activeGeneration = generation; publications += 1 }
    return true
  }
}

package enum SkyCacheDelivery: String {
  case live, frozen, exactPaused, startup, disabled
}

/// Observe source requests even when a drawable cannot be acquired. Otherwise
/// skipped live frames can look like a time edit on the first paused draw.
package struct SkyCacheDeliveryPolicy {
  private var signature: SkyCacheSignature?
  private var time: Float?
  package private(set) var pendingAuthoring = false
  package mutating func observe(_ u: Uniforms, paused: Bool) {
    let next = SkyCacheSignature(u)
    if paused {
      if let signature, signature != next { pendingAuthoring = true }
      if let time, time != u.cameraTime.w { pendingAuthoring = true }
    } else {
      pendingAuthoring = false
    }
    signature = next; time = u.cameraTime.w
  }
  package func delivery(paused: Bool, capture: Bool, invalidated: Bool) -> SkyCacheDelivery {
    guard paused else { return .live }
    return capture || invalidated || pendingAuthoring ? .exactPaused : .frozen
  }
  package mutating func encodedExact() { pendingAuthoring = false }
}

/// A single acknowledged unit keeps CPU submission from running ahead of cache
/// progress. The completion thread writes only this mailbox, never renderer state.
private final class SkyStepCompletion: @unchecked Sendable {
  private let lock = NSLock()
  private var result: Bool?
  func finish(_ success: Bool) { lock.lock(); result = success; lock.unlock() }
  func take() -> Bool? { lock.lock(); defer { lock.unlock() }; return result }
}

private struct SkyWeatherSnapshot {
  var replay: WeatherReplay
  let weather: MoistWeather
  let texture: MTLTexture
  let bounds: SIMD4<Float>
}
private final class SkyWeatherPreparation: @unchecked Sendable {
  private let lock = NSLock()
  private var result: SkyWeatherSnapshot?
  private var failure = false
  var failed: Bool { lock.lock(); defer { lock.unlock() }; return failure }
  func fail() { lock.lock(); failure = true; lock.unlock() }
  func finish(_ value: SkyWeatherSnapshot) { lock.lock(); result = value; lock.unlock() }
  func take() -> SkyWeatherSnapshot? {
    lock.lock(); defer { lock.unlock() }; let value = result; result = nil; return value
  }
}

/// Exactly the texture references consumed by this presented frame. Metal's
/// retained command buffers and the renderer completion closure hold them alive.
package struct AtmosphereFrameCache {
  package let trans, sky, previousSky, irradiance, previousIrradiance: MTLTexture
  package let clearSky, cloudAmbient: MTLTexture
}


/// Independent lighting cache. All geometry is rasterized; these small compute
/// passes integrate atmospheric scattering only when the environment changes.
package final class Atmosphere {
  package let enabled:Bool
  package let device: MTLDevice
  package var weather = MoistWeather(seed: 4)
  package var weatherReplay = WeatherReplay()
  package var weatherSeed: UInt64 = 4
  package var weatherTexture: MTLTexture
  package var weatherTime: Float = 0
  package var weatherBounds: SIMD4<Float> = .zero
  package var densityMaxLevels: [MTLTexture]
  package var emptyCells: MTLTexture
  package var lastUniforms: Uniforms?
  package let verifyPipeline: MTLComputePipelineState
  private let comparePipeline: MTLComputePipelineState
  package let maxPipeline: MTLComputePipelineState
  package let emptyPipeline: MTLComputePipelineState
  package var previousPublishedTime: Float = 0
  package var trans: MTLTexture
  package var multiple: MTLTexture
  package var sky: MTLTexture
  package var irradiance: MTLTexture
  package var previousSky: MTLTexture
  package var pendingSky: MTLTexture
  package var previousIrradiance: MTLTexture
  package var pendingIrradiance: MTLTexture
  package var clearSky: MTLTexture
  package var cloudAmbient: MTLTexture
  package var airRadiance: MTLTexture
  package var airForeground: MTLTexture
  package var airTransmission: MTLTexture
  package let airPipeline: MTLComputePipelineState
  package var cloudDensity: MTLTexture
  package var cloudLighting: MTLTexture
  package let cloudDensityPipeline: MTLComputePipelineState
  package let cloudLightingPipeline: MTLComputePipelineState
  package var updatePhase = "idle"
  package var publishedTime: Float = 0
  package var transitionTime: Float = 0
  package var transitionDuration: Float = 4.27
  private var exactPresentation = true
  package let transPipeline: MTLComputePipelineState
  package let multiplePipeline: MTLComputePipelineState
  package let skyPipeline: MTLComputePipelineState
  package let irradiancePipeline: MTLComputePipelineState
  package var noise: MTLTexture
  package let noisePipeline: MTLComputePipelineState
  package var key = ""
  package var densityKey = ""
  package var haze: Float = -1
  private struct Auxiliary {
    var trans, multiple, clearSky, cloudAmbient: MTLTexture
    var airRadiance, airForeground, airTransmission, cloudLighting: MTLTexture
  }
  private struct Density {
    var volume: MTLTexture
    var levels: [MTLTexture]
    var empty: MTLTexture
  }
  private enum Phase: String {
    case transmittance, multiple, clear, ambient, density, max0, max1, max2, empty
    case lighting, air, sky, irradiance, ready
  }
  private final class Candidate {
    let generation: UInt64
    let signature: SkyCacheSignature
    let uniforms: Uniforms
    let changesDensity: Bool
    let preparation = SkyWeatherPreparation()
    var weather: SkyWeatherSnapshot?
    var phase: Phase = .transmittance
    var offset = 0
    var completion: SkyStepCompletion?
    init(generation: UInt64, signature: SkyCacheSignature, uniforms: Uniforms, changesDensity: Bool) {
      self.generation = generation; self.signature = signature
      self.uniforms = uniforms; self.changesDensity = changesDensity
    }
  }
  private var spareAuxiliary: Auxiliary!
  private var spareDensity: Density!
  private let weatherQueue = DispatchQueue(label: "wrela.sky-weather", qos: .userInitiated)
  private var candidate: Candidate?
  private var weatherInFlight: SkyWeatherPreparation?
  private var lastFailure: String?
  private var gate = SkyPublicationGate()
  private var activeSignature: SkyCacheSignature?
  private var hasPublished = false
  private var stepUnits = 0
  private var lastNow: Float = 0
  private var sceneSun: SIMD3<Float>?
  private var addedCacheBytes = 0
  private var deliveryPolicy = SkyCacheDeliveryPolicy()
  private var delivery: SkyCacheDelivery = .startup
  private var lastExactReason: String?
  package func observeRequest(_ u: Uniforms, paused: Bool) {
    deliveryPolicy.observe(u, paused: paused)
  }
  package func frameCache() -> AtmosphereFrameCache {
    AtmosphereFrameCache(trans: trans, sky: sky, previousSky: previousSky,
      irradiance: irradiance, previousIrradiance: previousIrradiance,
      clearSky: clearSky, cloudAmbient: cloudAmbient)
  }
  package func cacheStatus() -> [String: Any] {
    ["activeGeneration": gate.activeGeneration, "desiredGeneration": gate.desiredGeneration,
     "candidateGeneration": candidate?.generation as Any? ?? NSNull(),
     "activeSignature": activeSignature?.diagnostic as Any? ?? NSNull(),
     "desiredSignature": gate.desired?.diagnostic as Any? ?? NSNull(),
     "requestedSunDiffersFromCache": activeSignature?.sun != gate.desired?.sun,
     "sceneSunLeadsCache": sceneSun != activeSignature?.sun,
     "sceneSun": sceneSun.map { [$0.x, $0.y, $0.z] } as Any? ?? NSNull(),
     "candidatePhase": candidate.map { $0.weather == nil ? "weather" : $0.phase.rawValue } ?? "idle",
     "candidateOffset": candidate?.offset ?? 0, "stepUnits": stepUnits,
     "candidateSampleTime": candidate?.uniforms.cameraTime.w as Any? ?? NSNull(),
     "publicationCount": gate.publications, "staleOrFailedRetirements": gate.retired,
     "cacheAgeSeconds": max(0, lastNow - publishedTime), "addedCacheBytes": addedCacheBytes,
     "stagingAllocatedBytes": [
       "density": spareDensity.volume.allocatedSize,
       "densityMaxLevels": spareDensity.levels.map(\.allocatedSize),
       "emptyCells": spareDensity.empty.allocatedSize,
       "cloudLighting": spareAuxiliary.cloudLighting.allocatedSize,
       "air": [spareAuxiliary.airRadiance, spareAuxiliary.airForeground,
         spareAuxiliary.airTransmission].map(\.allocatedSize),
       "luts": [spareAuxiliary.trans, spareAuxiliary.multiple,
         spareAuxiliary.clearSky, spareAuxiliary.cloudAmbient].map(\.allocatedSize)],
     "sceneLightPolicy": "Published cache sun and atmospheric inputs; shadow region follows current scene camera",
     "lastFailure": lastFailure as Any? ?? NSNull(),
     "delivery": delivery.rawValue, "pendingAuthoring": deliveryPolicy.pendingAuthoring,
     "lastExactReason": lastExactReason as Any? ?? NSNull(),
     "completionPending": candidate?.completion != nil]
  }
  package func applyPublishedLighting(to u: inout Uniforms, shadowContract: Bool) {
    if shadowContract { activeSignature?.applyLighting(to: &u) }
    sceneSun = SIMD3(u.sunExposure.x, u.sunExposure.y, u.sunExposure.z)
  }
  private var auxiliary: Auxiliary {
    get { Auxiliary(trans: trans, multiple: multiple, clearSky: clearSky, cloudAmbient: cloudAmbient,
      airRadiance: airRadiance, airForeground: airForeground, airTransmission: airTransmission,
      cloudLighting: cloudLighting) }
    set { trans = newValue.trans; multiple = newValue.multiple; clearSky = newValue.clearSky
      cloudAmbient = newValue.cloudAmbient; airRadiance = newValue.airRadiance
      airForeground = newValue.airForeground; airTransmission = newValue.airTransmission
      cloudLighting = newValue.cloudLighting }
  }
  private var densityTextures: Density {
    get { Density(volume: cloudDensity, levels: densityMaxLevels, empty: emptyCells) }
    set { cloudDensity = newValue.volume; densityMaxLevels = newValue.levels; emptyCells = newValue.empty }
  }
  package init(device: MTLDevice, library: MTLLibrary, enabled:Bool = true) throws {
    self.enabled=enabled
    self.device = device
    let wd = MTLTextureDescriptor.texture2DDescriptor(
      pixelFormat: .rgba32Float, width: 64, height: 64, mipmapped: false)
    wd.storageMode = .shared
    wd.usage = .shaderRead
    weatherTexture = device.makeTexture(descriptor: wd)!
    func texture(_ w: Int, _ h: Int) -> MTLTexture {
      let d = MTLTextureDescriptor.texture2DDescriptor(
        pixelFormat: .rgba16Float, width: enabled ? w:1, height: enabled ? h:1, mipmapped: false)
      d.usage = [.shaderRead, .shaderWrite]
      d.storageMode = .private
      return device.makeTexture(descriptor: d)!
    }
    trans = texture(256, 64)
    multiple = texture(32, 32)
    sky = texture(8192, 1024)
    irradiance = texture(64, 32)
    previousSky = texture(8192, 1024)
    pendingSky = texture(8192, 1024)
    previousIrradiance = texture(64, 32)
    pendingIrradiance = texture(64, 32)
    airRadiance = texture(512, 256)
    airForeground = texture(512, 256)
    airTransmission = texture(512, 256)
    clearSky = texture(128, 64)
    cloudAmbient = texture(64, 32)
    let nd = MTLTextureDescriptor()
    nd.textureType = .type3D
    nd.width = enabled ? 128 : 1
    nd.height = enabled ? 128 : 1
    nd.depth = enabled ? 128 : 1
    nd.pixelFormat = .rg16Float
    nd.usage = [.shaderRead, .shaderWrite]
    nd.storageMode = .private
    noise = device.makeTexture(descriptor: nd)!
    nd.width = enabled ? 1024 : 1
    nd.height = enabled ? 96 : 1
    nd.depth = enabled ? 1024 : 1
    nd.pixelFormat = .r16Float
    cloudDensity = device.makeTexture(descriptor: nd)!
    var levels: [MTLTexture] = []
    for (w, h) in [(512, 48), (256, 24), (128, 12)] {
      nd.width = enabled ? w : 1
      nd.height = enabled ? h : 1
      nd.depth = enabled ? w : 1
      levels.append(device.makeTexture(descriptor: nd)!)
    }
    densityMaxLevels = levels
    emptyCells = device.makeTexture(descriptor: nd)!
    comparePipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereCompareCache")!)
    verifyPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereVerifyEmptySpace")!)
    maxPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereMaxDensity")!)
    emptyPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereEmptyCells")!)
    nd.width = enabled ? 256 : 1
    nd.height = enabled ? 64 : 1
    nd.depth = enabled ? 256 : 1
    nd.pixelFormat = .r16Float
    cloudLighting = device.makeTexture(descriptor: nd)!
    cloudDensityPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereCloudDensity")!)
    cloudLightingPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereCloudLighting")!)
    airPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereAir")!)
    noisePipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereNoise")!)
    transPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereTrans")!)
    multiplePipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereMultiple")!)
    skyPipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereSky")!)
    irradiancePipeline = try device.makeComputePipelineState(
      function: library.makeFunction(name: "atmosphereIrradiance")!)
    // Allocate bounded staging storage at renderer construction, never at an
    // automatic climate boundary. Three existing panorama textures are reused.
    func duplicate(_ source: MTLTexture) -> MTLTexture {
      let d = MTLTextureDescriptor()
      d.textureType = source.textureType; d.pixelFormat = source.pixelFormat
      d.width = source.width; d.height = source.height; d.depth = source.depth
      d.usage = [.shaderRead, .shaderWrite]; d.storageMode = .private
      let texture = device.makeTexture(descriptor: d)!
      addedCacheBytes += texture.allocatedSize
      return texture
    }
    spareAuxiliary = Auxiliary(trans: duplicate(trans), multiple: duplicate(multiple),
      clearSky: duplicate(clearSky), cloudAmbient: duplicate(cloudAmbient),
      airRadiance: duplicate(airRadiance), airForeground: duplicate(airForeground),
      airTransmission: duplicate(airTransmission), cloudLighting: duplicate(cloudLighting))
    spareDensity = Density(volume: duplicate(cloudDensity), levels: densityMaxLevels.map(duplicate),
      empty: duplicate(emptyCells))
  }
  /// Explicit blocking diagnostic, never called by the presented frame path.
  /// Rebuild at the published snapshot's frozen time/seed/forcing into existing
  /// staging storage, then compare GPU outputs without a panorama readback.
  package func verifyPublishedCache(_ queue: MTLCommandQueue) throws -> [String: Any] {
    guard hasPublished, let source = lastUniforms, let signature = activeSignature else {
      throw RuntimeError.message("Render a completed sky before comparing its cache")
    }
    let retiredCandidate = candidate != nil
    if let c = candidate {
      _ = gate.complete(c.generation, success: false, final: false); candidate = nil
    }
    let sourceGeneration = gate.activeGeneration
    let savedAuxiliary = auxiliary, savedDensity = densityTextures
    let savedSky = sky, savedIrradiance = irradiance
    let savedWeather = weather, savedReplay = weatherReplay, savedTexture = weatherTexture
    let savedSeed = weatherSeed, savedWeatherTime = weatherTime, savedBounds = weatherBounds
    let savedKey = key, savedDensityKey = densityKey, savedHaze = haze
    let savedPreviousTime = previousPublishedTime, savedPublishedTime = publishedTime
    let savedTransition = transitionTime, savedDuration = transitionDuration, savedExact = exactPresentation
    let savedPhase = updatePhase
    defer {
      auxiliary = savedAuxiliary; densityTextures = savedDensity
      sky = savedSky; irradiance = savedIrradiance
      weather = savedWeather; weatherReplay = savedReplay; weatherTexture = savedTexture
      weatherSeed = savedSeed; weatherTime = savedWeatherTime; weatherBounds = savedBounds
      key = savedKey; densityKey = savedDensityKey; haze = savedHaze
      previousPublishedTime = savedPreviousTime; publishedTime = savedPublishedTime
      transitionTime = savedTransition; transitionDuration = savedDuration; exactPresentation = savedExact
      lastUniforms = source; updatePhase = savedPhase
    }
    guard let cb = queue.makeCommandBuffer(),
      let errors = device.makeBuffer(length: (sky.height + irradiance.height) * 16,
        options: .storageModeShared)
    else { throw RuntimeError.message("Unable to allocate cache comparison command or results") }
    auxiliary = spareAuxiliary; densityTextures = spareDensity
    sky = pendingSky; irradiance = pendingIrradiance
    // Force all equations, including density and the cold fixed-forcing weather
    // replay. This is separate from a sliced replay of the same dispatch plan.
    key = ""; densityKey = ""; haze = -1; weatherReplay = WeatherReplay()
    var exact = source
    exact.cameraTime.w = savedPublishedTime
    encodeExact(cb, &exact, paused: true, copyForPresentation: false)
    for (index, pair) in [(savedSky, sky), (savedIrradiance, irradiance)].enumerated() {
      let e = cb.makeComputeCommandEncoder()!
      e.label = "Published sky exact comparison"
      e.setComputePipelineState(comparePipeline)
      e.setTexture(pair.0, index: 0); e.setTexture(pair.1, index: 1)
      e.setBuffer(errors, offset: 0, index: 0)
      var offset = UInt32(index == 0 ? 0 : savedSky.height)
      e.setBytes(&offset, length: MemoryLayout<UInt32>.stride, index: 1)
      e.dispatchThreads(MTLSize(width: pair.0.height, height: 1, depth: 1),
        threadsPerThreadgroup: MTLSize(width: 64, height: 1, depth: 1))
      e.endEncoding()
    }
    cb.commit(); cb.waitUntilCompleted()
    guard cb.status == .completed else {
      throw cb.error ?? RuntimeError.message("Cache comparison command did not complete")
    }
    let values = errors.contents().bindMemory(to: SIMD4<Float>.self,
      capacity: savedSky.height + savedIrradiance.height)
    func result(_ start: Int, _ count: Int) -> [String: Any] {
      var maximum: Float = 0, total: Double = 0, components: Double = 0, invalid: Double = 0
      for i in start..<(start + count) {
        let value = values[i]
        maximum = max(maximum, value.x); total += Double(value.y)
        invalid += Double(value.z); components += Double(value.w)
      }
      return ["passed": maximum == 0 && invalid == 0,
        "maxAbsoluteError": maximum.isFinite ? maximum as Any : "nonfinite",
        "meanAbsoluteError": total.isFinite ? total / max(1, components) as Any : "nonfinite",
        "nonFinitePixels": invalid, "componentCount": components]
    }
    let skyResult = result(0, savedSky.height)
    let irradianceResult = result(savedSky.height, savedIrradiance.height)
    return ["passed": skyResult["passed"] as? Bool == true && irradianceResult["passed"] as? Bool == true,
      "sky": skyResult, "irradiance": irradianceResult, "absoluteTolerance": 0,
      "publishedGeneration": sourceGeneration, "sourceSampleTime": savedPublishedTime,
      "sourceSignature": signature.diagnostic, "weatherTick": savedWeather.ticks,
      "candidateRetired": retiredCandidate, "comparison": "cold exact at published sample time",
      "gpuMilliseconds": (cb.gpuEndTime - cb.gpuStartTime) * 1000,
      "resultBufferBytes": errors.length]
  }

  package func verifyEmptySpace(_ queue: MTLCommandQueue) throws -> [String: Any] {
    guard var u = lastUniforms else {
      throw RuntimeError.message("Render the sky before verifying it")
    }
    let output = device.makeBuffer(length: 16, options: .storageModeShared)!
    memset(output.contents(), 0, 16)
    let cb = queue.makeCommandBuffer()!
    let e = cb.makeComputeCommandEncoder()!
    e.setComputePipelineState(verifyPipeline)
    for (i, t) in [cloudDensity, noise, weatherTexture, emptyCells].enumerated() {
      e.setTexture(t, index: i)
    }
    e.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
    e.setBuffer(output, offset: 0, index: 1)
    e.dispatchThreads(
      MTLSize(width: 131072, height: 1, depth: 1),
      threadsPerThreadgroup: MTLSize(width: 64, height: 1, depth: 1))
    e.endEncoding()
    cb.commit()
    cb.waitUntilCompleted()
    if let error = cb.error { throw error }
    let values = output.contents().bindMemory(to: UInt32.self, capacity: 4)
    return [
      "passed": values[1] == 0, "candidateRays": 131072, "certifiedSegments": values[0],
      "densitySamples": values[0] * 9, "violations": values[1],
      "maximumSkippedDensity": Float(bitPattern: values[2]), "time": u.cameraTime.w,
    ]
  }
  package func updateWeather(_ time: Float, _ sun: Float, reset: Float? = nil) {
    if let seed = reset { weatherSeed = UInt64(seed) }
    weather = weatherReplay.advance(to: time, seed: weatherSeed, sun: sun)
    weatherTime = Float(weather.ticks) * 2
    // A fresh immutable snapshot avoids CPU writes racing in-flight GPU work.
    let d = MTLTextureDescriptor.texture2DDescriptor(
      pixelFormat: .rgba32Float, width: 64, height: 64, mipmapped: false)
    d.storageMode = .shared
    d.usage = .shaderRead
    let texture = device.makeTexture(descriptor: d)!
    let cells = weather.gpuCells
    cells.withUnsafeBytes {
      texture.replace(
        region: MTLRegionMake2D(0, 0, 64, 64), mipmapLevel: 0, withBytes: $0.baseAddress!,
        bytesPerRow: 64 * 16)
    }
    weatherTexture = texture
    var gx: Float = 0
    var gz: Float = 0
    for z in 0..<64 {
      for x in 0..<64 {
        let w = cells[z * 64 + x].z
        gx = max(gx, abs(w - cells[z * 64 + (x + 1) % 64].z))
        gz = max(gz, abs(w - cells[((z + 1) % 64) * 64 + x].z))
      }
    }
    weatherBounds = SIMD4(cells.map(\.x).max() ?? 0, gx * 0.035, gz * 0.035, 0)
  }
  private func receiveWeather(_ c: Candidate) {
    if c.weather == nil { c.weather = c.preparation.take() }
    if c.weather != nil || c.preparation.failed { weatherInFlight = nil }
  }

  private func prepareWeather(_ c: Candidate) {
    weatherInFlight = c.preparation
    let replay = weatherReplay, device = device, job = c.preparation
    let time = c.uniforms.cameraTime.w * c.uniforms.options.y
    let sun = c.uniforms.sunExposure.y, seed = UInt64(c.uniforms.sky.w)
    weatherQueue.async {
      var replay = replay
      let weather = replay.advance(to: time, seed: seed, sun: sun)
      let d = MTLTextureDescriptor.texture2DDescriptor(
        pixelFormat: .rgba32Float, width: 64, height: 64, mipmapped: false)
      d.storageMode = .shared; d.usage = .shaderRead
      guard let texture = device.makeTexture(descriptor: d) else { job.fail(); return }
      let cells = weather.gpuCells
      cells.withUnsafeBytes {
        texture.replace(region: MTLRegionMake2D(0, 0, 64, 64), mipmapLevel: 0,
          withBytes: $0.baseAddress!, bytesPerRow: 64 * 16)
      }
      var gx: Float = 0, gz: Float = 0
      for z in 0..<64 { for x in 0..<64 {
        let w = cells[z * 64 + x].z
        gx = max(gx, abs(w - cells[z * 64 + (x + 1) % 64].z))
        gz = max(gz, abs(w - cells[((z + 1) % 64) * 64 + x].z))
      } }
      job.finish(SkyWeatherSnapshot(replay: replay, weather: weather, texture: texture,
        bounds: SIMD4(cells.map(\.x).max() ?? 0, gx * 0.035, gz * 0.035, 0)))
    }
  }

  package func encode(_ cb: MTLCommandBuffer, _ u: inout Uniforms, paused: Bool,
    capture: Bool = false, profile: GPUProfile? = nil)
  {
    lastNow = u.cameraTime.w; stepUnits = 0
    guard enabled, u.environment.x != 4 else {
      delivery = .disabled; updatePhase = "disabled"; u.environment.z = 0; return
    }
    let signature = SkyCacheSignature(u)
    gate.request(signature, invalidate: key.isEmpty)
    delivery = hasPublished
      ? deliveryPolicy.delivery(paused: paused, capture: capture, invalidated: key.isEmpty) : .startup
    if delivery == .frozen {
      // Simulation pause freezes the current presentation, including its blend.
      // Preserve any submitted candidate and mailbox for normal resume handling.
      updatePhase = "frozen"; bindSnapshotTimes(&u); return
    }
    if delivery == .startup || delivery == .exactPaused {
      // Startup, explicit paused captures and paused source edits retain the
      // exact ordered rebuild. Ordinary simulation pause never enters here.
      // Already submitted candidate work precedes this on the same Metal queue;
      // dropping its mailbox prevents a later stale publication.
      if let c = candidate {
        _ = gate.complete(c.generation, success: false, final: false); candidate = nil
      }
      let needsExact = !hasPublished || key.isEmpty || activeSignature != signature
        || !exactPresentation || publishedTime != u.cameraTime.w
      encodeExact(cb, &u, paused: true, profile: profile)
      if needsExact {
        lastExactReason = delivery == .startup ? "startup"
          : capture ? "pausedCapture" : deliveryPolicy.pendingAuthoring ? "pausedEdit" : "pausedRestore"
        _ = gate.complete(gate.desiredGeneration, success: true, final: true)
        activeSignature = signature; hasPublished = true
        updatePhase = delivery.rawValue
      }
      deliveryPolicy.encodedExact()
      return
    }
    exactPresentation = false; updatePhase = "idle"
    // key is also an existing authoring invalidation hook. Mark it consumed;
    // the active typed signature remains unchanged until publication.
    if key.isEmpty { key = "pending" }
    if let c = candidate {
      if let completion = c.completion {
        guard let success = completion.take() else {
          updatePhase = "waiting"; bindSnapshotTimes(&u); return
        }
        c.completion = nil
        if !gate.complete(c.generation, success: success, final: c.phase == .ready) {
          if !success { lastFailure = "Candidate GPU command buffer failed" }
          candidate = nil
        } else if c.phase == .ready {
          publish(c, now: u.cameraTime.w); candidate = nil
        }
      } else if c.generation != gate.desiredGeneration {
        // Weather work also drains before reuse; retain only the latest desired
        // signature while it finishes, avoiding an unbounded job queue.
        receiveWeather(c)
        guard c.weather != nil || c.preparation.failed else { updatePhase = "weatherDrain"; bindSnapshotTimes(&u); return }
        _ = gate.complete(c.generation, success: false, final: false); candidate = nil
      }
    }
    if candidate == nil {
      // An exact paused rebuild can abandon a CPU job. Drain that one job
      // before scheduling another; toggling delivery never grows a work queue.
      if let job = weatherInFlight {
        guard job.take() != nil || job.failed else {
          updatePhase = "weatherDrain"; bindSnapshotTimes(&u); return
        }
        weatherInFlight = nil
      }
      let c = Candidate(generation: gate.desiredGeneration, signature: signature, uniforms: u,
        changesDensity: activeSignature?.density != signature.density)
      candidate = c; prepareWeather(c)
    }
    guard let c = candidate else { bindSnapshotTimes(&u); return }
    receiveWeather(c)
    if c.preparation.failed {
      lastFailure = "Unable to allocate candidate weather snapshot"
      _ = gate.complete(c.generation, success: false, final: false); candidate = nil
      updatePhase = "failed"; bindSnapshotTimes(&u); return
    }
    guard c.weather != nil else { updatePhase = "weather"; bindSnapshotTimes(&u); return }
    encodeStep(cb, c, profile: profile)
    bindSnapshotTimes(&u)
  }

  private func bindSnapshotTimes(_ u: inout Uniforms) {
    u.weatherBounds = weatherBounds; u.environment.w = 0
    u.skyTimes = SIMD4(previousPublishedTime, publishedTime, 0, 0)
    u.environment.z = min(1, max(0, (u.cameraTime.w - transitionTime) / transitionDuration))
  }

  private func publish(_ c: Candidate, now: Float) {
    let old = auxiliary; auxiliary = spareAuxiliary; spareAuxiliary = old
    if c.changesDensity {
      let oldDensity = densityTextures; densityTextures = spareDensity; spareDensity = oldDensity
    }
    swap(&previousSky, &sky); swap(&sky, &pendingSky)
    swap(&previousIrradiance, &irradiance); swap(&irradiance, &pendingIrradiance)
    let snapshot = c.weather!
    weather = snapshot.weather; weatherReplay = snapshot.replay; weatherTexture = snapshot.texture
    weatherSeed = UInt64(c.uniforms.sky.w); weatherTime = Float(weather.ticks) * 2
    weatherBounds = snapshot.bounds
    previousPublishedTime = publishedTime; publishedTime = c.uniforms.cameraTime.w
    transitionDuration = min(1.5, max(0.1, now - transitionTime)); transitionTime = now
    activeSignature = c.signature; haze = c.uniforms.sky.z
    let u = c.uniforms
    key = "\(u.options.y)-\(u.environment.x)-\(u.sunExposure.x)-\(u.sunExposure.y)-\(u.sunExposure.z)-\(u.sky)"
    densityKey = "\(u.sky.x)-\(u.sky.y)-\(u.sky.w)"
    lastUniforms = c.uniforms; lastUniforms?.weatherBounds = snapshot.bounds
  }

  private func encodeStep(_ cb: MTLCommandBuffer, _ c: Candidate, profile: GPUProfile?) {
    var u = c.uniforms
    u.weatherBounds = c.weather!.bounds
    let a = spareAuxiliary!, d = c.changesDensity ? spareDensity! : densityTextures
    let weather = c.weather!.texture
    func dispatch(_ pipeline: MTLComputePipelineState, _ textures: [MTLTexture],
      output: MTLTexture, rows: Int? = nil)
    {
      let e = profile?.compute(cb, "sky.candidate.\(c.phase.rawValue)") ?? cb.makeComputeCommandEncoder()!
      e.setComputePipelineState(pipeline)
      for (i, t) in textures.enumerated() { e.setTexture(t, index: i) }
      e.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
      e.dispatchThreads(MTLSize(width: output.width, height: rows ?? output.height, depth: 1),
        threadsPerThreadgroup: pipeline === skyPipeline
          ? MTLSize(width: 64, height: 1, depth: 1) : MTLSize(width: 8, height: 8, depth: 1))
      e.endEncoding(); stepUnits = rows ?? output.height
    }
    func volume(_ pipeline: MTLComputePipelineState, _ source: MTLTexture,
      _ output: MTLTexture, slices: Int)
    {
      u.environment.w = Float(c.offset)
      let e = profile?.compute(cb, "sky.candidate.\(c.phase.rawValue)") ?? cb.makeComputeCommandEncoder()!
      e.setComputePipelineState(pipeline)
      for (i, t) in [source, output, noise, weather].enumerated() { e.setTexture(t, index: i) }
      e.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
      e.dispatchThreads(MTLSize(width: output.width, height: slices, depth: output.depth),
        threadsPerThreadgroup: MTLSize(width: 8, height: 1, depth: 8))
      e.endEncoding(); stepUnits = slices
    }
    func next(_ phase: Phase) { c.phase = phase; c.offset = 0 }
    func advance(_ amount: Int, limit: Int, then phase: Phase) {
      c.offset += amount; if c.offset >= limit { next(phase) }
    }
    let textures = [a.trans, a.multiple, pendingSky, noise, a.cloudAmbient,
      d.volume, a.cloudLighting, a.airRadiance, a.airForeground, a.airTransmission, weather, d.empty]
    updatePhase = "candidate.\(c.phase.rawValue)"
    switch c.phase {
    case .transmittance:
      dispatch(transPipeline, [a.trans], output: a.trans); next(.multiple)
    case .multiple:
      dispatch(multiplePipeline, [a.trans, a.multiple], output: a.multiple); next(.clear)
    case .clear:
      u.sky.x = 0; var clearTextures = textures; clearTextures[2] = a.clearSky
      dispatch(skyPipeline, clearTextures, output: a.clearSky); next(.ambient)
    case .ambient:
      dispatch(irradiancePipeline, [a.clearSky, a.cloudAmbient], output: a.cloudAmbient)
      next(c.changesDensity ? .density : .lighting)
    case .density:
      u.cameraTime.w = 0
      volume(cloudDensityPipeline, noise, d.volume, slices: 1)
      advance(1, limit: d.volume.height, then: .max0)
    case .max0, .max1, .max2:
      let index = c.phase == .max0 ? 0 : (c.phase == .max1 ? 1 : 2)
      let target = d.levels[index]
      volume(maxPipeline, index == 0 ? d.volume : d.levels[index - 1], target, slices: 1)
      advance(1, limit: target.height, then: index == 0 ? .max1 : (index == 1 ? .max2 : .empty))
    case .empty:
      volume(emptyPipeline, d.levels[2], d.empty, slices: 1)
      advance(1, limit: d.empty.height, then: .lighting)
    case .lighting:
      volume(cloudLightingPipeline, d.volume, a.cloudLighting, slices: 1)
      advance(1, limit: a.cloudLighting.height, then: .air)
    case .air:
      u.environment.w = Float(c.offset)
      dispatch(airPipeline, [a.trans, a.multiple, a.airRadiance, a.airForeground,
        a.airTransmission, a.cloudLighting], output: a.airRadiance, rows: 16)
      advance(16, limit: a.airRadiance.height, then: .sky)
    case .sky:
      let count = u.sunExposure.y < 0.26 ? 2 : 4
      u.environment.w = Float(c.offset)
      dispatch(skyPipeline, textures, output: pendingSky, rows: count)
      advance(count, limit: pendingSky.height, then: .irradiance)
    case .irradiance:
      dispatch(irradiancePipeline, [pendingSky, pendingIrradiance], output: pendingIrradiance)
      next(.ready)
    case .ready: return
    }
    let completion = SkyStepCompletion(); c.completion = completion
    cb.addCompletedHandler { command in completion.finish(command.status == .completed) }
  }

  private func encodeExact(_ cb: MTLCommandBuffer, _ u: inout Uniforms, paused: Bool, profile: GPUProfile? = nil,
    copyForPresentation: Bool = true)
  {
    if !enabled || u.environment.x == 4 { updatePhase = "disabled"; u.environment.z = 0; return }
    let next =
      "\(u.options.y)-\(u.environment.x)-\(u.sunExposure.x)-\(u.sunExposure.y)-\(u.sunExposure.z)-\(u.sky)"
    let nextDensity = "\(u.sky.x)-\(u.sky.y)-\(u.sky.w)"
    let densityChanged = densityKey != nextDensity
    densityKey = nextDensity
    let changed = key != next
    let now = u.cameraTime.w
    u.weatherBounds = weatherBounds
    let full = changed || (paused && (!exactPresentation || publishedTime != now))
    updatePhase = "idle"
    exactPresentation = paused
    let first = key.isEmpty
    let mediumChanged = haze != u.sky.z
    key = next
    haze = u.sky.z
    if first {
      let e = profile?.compute(cb, "sky.noise") ?? cb.makeComputeCommandEncoder()!
      e.setComputePipelineState(noisePipeline)
      e.setTexture(noise, index: 0)
      e.dispatchThreads(
        MTLSize(width: 128, height: 128, depth: 128),
        threadsPerThreadgroup: MTLSize(width: 4, height: 4, depth: 4))
      e.endEncoding()
    }
    func dispatch(
      _ pipeline: MTLComputePipelineState, _ textures: [MTLTexture], _ output: MTLTexture,
      rows: Int? = nil
    ) {
      let e =
        profile?.compute(
          cb,
          pipeline === airPipeline
            ? "sky.air"
            : (pipeline === skyPipeline
              ? "sky.rays"
              : (pipeline === irradiancePipeline
                ? "sky.irradiance"
                : (pipeline === transPipeline ? "sky.transmittance" : "sky.multiple"))))
        ?? cb.makeComputeCommandEncoder()!
      e.setComputePipelineState(pipeline)
      for (i, t) in textures.enumerated() { e.setTexture(t, index: i) }
      e.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
      e.dispatchThreads(
        MTLSize(width: output.width, height: rows ?? output.height, depth: 1),
        threadsPerThreadgroup: pipeline === skyPipeline
          ? MTLSize(width: 64, height: 1, depth: 1) : MTLSize(width: 8, height: 8, depth: 1))
      e.endEncoding()
    }
    func volume(
      _ pipeline: MTLComputePipelineState, _ input: MTLTexture, _ output: MTLTexture,
      offset: Int = 0, slices: Int? = nil
    ) {
      let count = slices ?? output.height
      let isLighting = pipeline === cloudLightingPipeline
      let dispatchCount = isLighting ? count : 1
      for pass in 0..<dispatchCount {
        u.environment.w = Float(offset + (isLighting ? pass : 0))
        let e =
          profile?.compute(cb, isLighting ? "sky.cloudLighting" : "sky.density")
          ?? cb.makeComputeCommandEncoder()!
        e.setComputePipelineState(pipeline)
        e.setTexture(input, index: 0)
        e.setTexture(output, index: 1)
        e.setTexture(noise, index: 2)
        e.setTexture(weatherTexture, index: 3)
        e.setBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
        e.dispatchThreads(
          MTLSize(width: output.width, height: isLighting ? 1 : count, depth: output.depth),
          threadsPerThreadgroup: MTLSize(width: 8, height: 1, depth: 8))
        e.endEncoding()
      }
      u.environment.w = 0
    }
    if mediumChanged {
      dispatch(transPipeline, [trans], trans)
      dispatch(multiplePipeline, [trans, multiple], multiple)
    }
    if changed {
      let coverage = u.sky.x
      u.sky.x = 0
      u.environment.w = 0
      dispatch(
        skyPipeline,
        [
          trans, multiple, clearSky, noise, cloudAmbient, cloudDensity, cloudLighting, airRadiance,
          airForeground, airTransmission, weatherTexture, emptyCells,
        ], clearSky)
      dispatch(irradiancePipeline, [clearSky, cloudAmbient], cloudAmbient)
      u.sky.x = coverage
    }
    if full {
      updateWeather(now * u.options.y, u.sunExposure.y, reset: changed ? u.sky.w : nil)
      u.weatherBounds = weatherBounds
      updatePhase = "rebuild"
      if densityChanged {
        u.cameraTime.w = 0
        volume(cloudDensityPipeline, noise, cloudDensity)
        u.cameraTime.w = now
        var source = cloudDensity
        for target in densityMaxLevels {
          volume(maxPipeline, source, target)
          source = target
        }
        volume(emptyPipeline, source, emptyCells)
      }
      volume(cloudLightingPipeline, cloudDensity, cloudLighting)
      u.environment.w = 0
      dispatch(
        airPipeline, [trans, multiple, airRadiance, airForeground, airTransmission, cloudLighting],
        airRadiance)
      dispatch(
        skyPipeline,
        [
          trans, multiple, sky, noise, cloudAmbient, cloudDensity, cloudLighting, airRadiance,
          airForeground, airTransmission, weatherTexture, emptyCells,
        ], sky)
      dispatch(irradiancePipeline, [sky, irradiance], irradiance)
      if copyForPresentation {
        let b = cb.makeBlitCommandEncoder()!
        b.copy(from: sky, to: previousSky)
        b.copy(from: sky, to: pendingSky)
        b.copy(from: irradiance, to: previousIrradiance)
        b.endEncoding()
      }
      previousPublishedTime = now
      publishedTime = now
      transitionTime = now - 1
    }
    lastUniforms = u
    u.cameraTime.w = now
    u.environment.w = 0
    u.skyTimes = SIMD4(previousPublishedTime, publishedTime, 0, 0)
    u.environment.z = paused ? 1 : min(1, max(0, (now - transitionTime) / transitionDuration))
  }
}
