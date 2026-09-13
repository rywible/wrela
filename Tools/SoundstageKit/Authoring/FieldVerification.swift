import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import Metal
import simd

enum NightEnvironmentVerification {
  struct Sample {
    var direction: SIMD3<Float>
    var transmission: Float
    var floor: SIMD3<Float>
    var outdoor: Bool
  }

  static func unit(direction: SIMD3<Float>, transmission: Float) -> Float {
    let t = transmission.isFinite ? clamp(transmission, 0, 1) : 0
    let shape = direction.y >= 0 ? 0.1 + 1.05 * clamp(direction.y, 0, 1) : 0.18
    return shape * (0.25 + 0.75 * t)
  }

  static func coefficient(floor: SIMD3<Float>, outdoor: Bool) -> SIMD3<Float> {
    guard outdoor else { return .zero }
    return SIMD3<Float>(
      floor.x.isFinite ? clamp(floor.x, 0, 0.25) : 0,
      floor.y.isFinite ? clamp(floor.y, 0, 0.25) : 0,
      floor.z.isFinite ? clamp(floor.z, 0, 0.25) : 0) * 1.25
  }

  static func evaluate(_ sample: Sample) -> SIMD4<Float> {
    let value = unit(direction: sample.direction, transmission: sample.transmission)
    return SIMD4<Float>(coefficient(floor: sample.floor, outdoor: sample.outdoor) * value, value)
  }

  /// Independent equal-solid-angle integration of the authored clear night environment.
  /// This deliberately does not reuse the production 128-direction golden-angle quadrature.
  static func clearSkyIrradianceOverPi(
    normal: SIMD3<Float>, rings: Int = 256, azimuths: Int = 512
  ) -> Float {
    guard rings > 0, azimuths > 0, normal.x.isFinite, normal.y.isFinite,
      normal.z.isFinite, length_squared(normal) > 0 else { return 0 }
    let n = normalize(SIMD3<Double>(Double(normal.x), Double(normal.y), Double(normal.z)))
    var sum = 0.0
    for ring in 0..<rings {
      let y = -1.0 + (Double(ring) + 0.5) * 2.0 / Double(rings)
      let radial = sqrt(max(0, 1 - y * y))
      let radiance = y >= 0 ? 0.1 + 1.05 * y : 0.18
      for azimuth in 0..<azimuths {
        let phi = (Double(azimuth) + 0.5) * 2.0 * Double.pi / Double(azimuths)
        let direction = SIMD3<Double>(radial * cos(phi), y, radial * sin(phi))
        sum += radiance * max(dot(n, direction), 0)
      }
    }
    return Float(sum * 4 / Double(rings * azimuths))
  }
}

extension WorkshopRenderer {
  /// Run the production deformation function on the GPU and compare it with
  /// compiler-side auditing: correctives, rigid bends, antipodal rotations,
  /// sparse/four-way weights, and the exact affine guide-frame fallback.
  func verifyCraftDeformation() throws -> Float {
    let url=Bundle.main.url(forResource:"Surface",withExtension:"metal")
      ?? ProjectContext.workspace.appendingPathComponent("Engine/FieldEngine/Resources/Surface.metal")
    let source=try String(contentsOf:url,encoding:.utf8)
    guard let boundary=source.range(of:"struct Instance") else {throw RuntimeError.message("Missing production skin function")}
    let kernel=String(source[..<boundary.lowerBound])+"""
    kernel void verifyCraft(uint id [[thread_position_in_grid]],const device Vertex *input [[buffer(0)]],device Vertex *output [[buffer(1)]],const device SkinWeight *weights [[buffer(2)]],constant float4x4 *bones [[buffer(3)]],constant CorrectiveUniform *correctives [[buffer(4)]],constant uint &skinCount [[buffer(5)]]) {
      output[id]=skinVertex(input[id],id,weights,bones,skinCount,correctives,2);
    }
    """
    let library=try device.makeLibrary(source:kernel,options:nil)
    let pipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"verifyCraft")!)
    var mesh=ParametricMesh.ellipsoid(.zero,V3(0.6,0.5,0.4),detail:16)
    for i in mesh.vertices.indices {mesh.vertices[i].groom=SIMD4(Float(i)*0.31,Float(i%17)/16,0.72,0.2)}
    let weights=mesh.vertices.map{SkinWeight(0,1,blend:($0.position.y+0.5))}
    var a=simd_float4x4(simd_quatf(angle:0.8,axis:normalize(V3(1,2,3))))
    a.columns.3=SIMD4(0.1,0.2,0.3,1)
    let b=simd_float4x4(simd_quatf(angle:-0.4,axis:V3(0,1,0)))
    var affine=b;affine.columns.0 *= 1.1;affine.columns.1 += affine.columns.2*0.1
    let nearPi=simd_float4x4(simd_quatf(angle:3.12,axis:V3(0,1,0)))
    let nearMinusPi=simd_float4x4(simd_quatf(angle:-3.12,axis:V3(0,1,0)))
    var sparse=SkinWeight(0);sparse.joints=SIMD4(0,1,0,0);sparse.weights=SIMD4(0,1,0,0)
    let fourWeights=mesh.vertices.enumerated().map { i,v -> SkinWeight in
      var w=SkinWeight(0);w.joints=SIMD4(0,1,2,3)
      w.weights=SIMD4(0.1,0.2,0.3,0.4)
      if i%3==0 {w.weights=SIMD4(0,0.5,0,0.5)}
      return w
    }
    let cases:[(String,[simd_float4x4],[SkinWeight])]=[
      ("rigid-bend",[a,b],weights),
      ("affine-guide",[a,affine],weights),
      ("antipodal",[nearPi,nearMinusPi],weights),
      ("sparse",[nearPi,nearMinusPi],Array(repeating:sparse,count:weights.count)),
      ("four-influences",[a,b,nearPi,nearMinusPi],fourWeights)]
    let fields=[SurfaceEdit(id:"a",part:"body",center:V3(0.1,0.1,0),radius:V3(1,0.8,0.6),offset:V3(0.03,0,0.02),dilation:V3(0.02,-0.01,0.03)),
      SurfaceEdit(id:"b",part:"body",center:V3(-0.2,0,0),radius:V3(repeating:0.8),offset:V3(0,0.02,-0.01))]
    var uniforms=fields.map{CorrectiveUniform($0,weight:1)}
    let input=device.makeBuffer(bytes:mesh.vertices,length:mesh.vertices.count*MemoryLayout<Vertex>.stride)!
    let output=device.makeBuffer(length:input.length,options:.storageModeShared)!
    var worst:Float=0
    for (name,matrices,skinWeights) in cases {
      let skinning=SkinningPalette(matrices)
      var palette=skinning.encodedMatrices,count=skinning.encodedCount
      let skin=device.makeBuffer(bytes:skinWeights,length:skinWeights.count*MemoryLayout<SkinWeight>.stride)!
      let command=queue.makeCommandBuffer()!,encoder=command.makeComputeCommandEncoder()!
      encoder.setComputePipelineState(pipeline);encoder.setBuffer(input,offset:0,index:0);encoder.setBuffer(output,offset:0,index:1);encoder.setBuffer(skin,offset:0,index:2)
      encoder.setBytes(&palette,length:palette.count*64,index:3);encoder.setBytes(&uniforms,length:uniforms.count*64,index:4);encoder.setBytes(&count,length:4,index:5)
      encoder.dispatchThreads(MTLSize(width:mesh.vertices.count,height:1,depth:1),threadsPerThreadgroup:MTLSize(width:pipeline.threadExecutionWidth,height:1,depth:1));encoder.endEncoding();command.commit();command.waitUntilCompleted()
      if let error=command.error {throw error}
      let cpu=DeformationAudit.deformed(DeformationAudit.corrected(mesh,fields:fields),weights:skinWeights,palette:matrices)
      let gpu=output.contents().bindMemory(to:Vertex.self,capacity:mesh.vertices.count)
      for i in mesh.vertices.indices {
        guard gpu[i].groom==mesh.vertices[i].groom else {throw RuntimeError.message("GPU deformation changed procedural groom coordinates in \(name)")}
        let p=length(gpu[i].position-cpu.vertices[i].position),n=length(gpu[i].normal-cpu.vertices[i].normal)
        guard p.isFinite && n.isFinite else {throw RuntimeError.message("Nonfinite GPU deformation in \(name)")}
        guard max(p,n)<0.00002 else {throw RuntimeError.message("CPU/GPU creature deformation mismatch in \(name), vertex \(i): position \(p), normal \(n)")}
        worst=max(worst,max(p,n))
      }
    }
    guard worst<0.00002 else {throw RuntimeError.message("CPU/GPU creature deformation mismatch: \(worst)")};return worst
  }
  func verifyFields() throws -> [String: Any] {
    let fields: [(String, Shape)] = [
      ("sphere", Shape.sphere(1)),
      ("section-loft", .loft(SectionLoft([
        LoftSection(id:"base",z:-0.8,radius:SIMD2(0.3,0.4)),
        LoftSection(id:"wide",z:0,center:SIMD2(0.1,0.2),radius:SIMD2(0.6,0.35)),
        LoftSection(id:"tip",z:0.8,center:SIMD2(-0.05,0),radius:SIMD2(0.08,0.12))]))),
      (
        "transformed-union",
        Shape.sphere(1).sized(2.3).moved(V3(-1, 0.5, 0.4)).joined(.box(V3(1, 0.4, 0.7)))
      ),
      ("capsule", .capsule(V3(-1, 0.3, 0), V3(1, 2, 0.2), 0.3)),
      ("degenerate-capsule", .capsule(.zero, .zero, 1)),
      ("torus", .torus(1.6, 0.3)),
      ("stretched-canopy", Shape.sphere(1).stretched(V3(2, 0.7, 1.2)).moved(V3(0, 1.4, 0))),
      ("rounded-cut", Shape.sphere(1).stretched(V3(0.7,1,0.85)).cut(.sphere(0.65).moved(V3(0.5,0.2,0.6)),radius:0.12)),
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
  /// Runs the production night-environment function on Metal and compares every
  /// channel with the independent CPU definition. This kernel is diagnostic only.
  func verifyNightEnvironment() throws -> Float {
    let surfaceURL = Bundle.main.url(forResource: "Surface", withExtension: "metal")
      ?? ProjectContext.workspace.appendingPathComponent("Engine/FieldEngine/Resources/Surface.metal")
    let atmosphereURL = Bundle.main.url(forResource: "Atmosphere", withExtension: "metal")
      ?? ProjectContext.workspace.appendingPathComponent("Engine/FieldEngine/Resources/Atmosphere.metal")
    var source = try String(contentsOf: surfaceURL, encoding: .utf8)
    source = source.replacingOccurrences(
      of: "// ATMOSPHERE", with: try String(contentsOf: atmosphereURL, encoding: .utf8))
    source = source.replacingOccurrences(of: "// PROJECT_SURFACES", with:
      "void projectSurface(int kind,float3 local,float3 world,float3 n,thread float3 &color,thread float &rough,thread float &bump,thread float &strength) {}")
    guard source.contains("kernel void nightEnvironmentCheck") else {
      throw RuntimeError.message("Missing production night environment check")
    }
    let library = try device.makeLibrary(source: source, options: nil)
    guard let function = library.makeFunction(name: "nightEnvironmentCheck") else {
      throw RuntimeError.message("Missing compiled night environment check")
    }
    let pipeline = try device.makeComputePipelineState(function: function)
    let nan = Float.nan
    typealias Sample = NightEnvironmentVerification.Sample
    var samples: [Sample] = []
    samples.reserveCapacity(8)
    samples.append(Sample(
      direction: SIMD3<Float>(0, 1, 0), transmission: 1,
      floor: SIMD3<Float>(0.16, 0.08, 0.24), outdoor: true))
    samples.append(Sample(
      direction: SIMD3<Float>(0, -1, 0), transmission: 0,
      floor: SIMD3<Float>(0.16, 0.08, 0.24), outdoor: true))
    samples.append(Sample(
      direction: SIMD3<Float>(1, 0, 0), transmission: 0.4,
      floor: SIMD3<Float>(0.16, 0.08, 0.24), outdoor: true))
    let oblique = normalize(SIMD3<Float>(1, 0.6, -0.2))
    samples.append(Sample(
      direction: oblique, transmission: 2,
      floor: SIMD3<Float>(-1, 0.1, 0.9), outdoor: true))
    samples.append(Sample(
      direction: SIMD3<Float>(0, 2, 0), transmission: -2,
      floor: SIMD3<Float>(nan, 0.1, 0.2), outdoor: true))
    samples.append(Sample(
      direction: SIMD3<Float>(0, 1, 0), transmission: nan,
      floor: SIMD3<Float>(0.2, 0.1, 0.05), outdoor: true))
    samples.append(Sample(
      direction: SIMD3<Float>(0, 1, 0), transmission: 1,
      floor: SIMD3<Float>(0.2, 0.1, 0.05), outdoor: false))
    samples.append(Sample(
      direction: SIMD3<Float>(0, 1, 0), transmission: 1,
      floor: SIMD3<Float>.zero, outdoor: true))
    var inputValues: [SIMD4<Float>] = []
    inputValues.reserveCapacity(samples.count * 2)
    for sample in samples {
      inputValues.append(SIMD4<Float>(sample.direction, sample.transmission))
      inputValues.append(SIMD4<Float>(sample.floor, sample.outdoor ? 1 : 0))
    }
    let input = device.makeBuffer(bytes: inputValues, length: inputValues.count * 16)!
    let output = device.makeBuffer(length: samples.count * 16, options: .storageModeShared)!
    var count = UInt32(samples.count)
    let command = queue.makeCommandBuffer()!
    let encoder = command.makeComputeCommandEncoder()!
    encoder.setComputePipelineState(pipeline)
    encoder.setBuffer(input, offset: 0, index: 0)
    encoder.setBuffer(output, offset: 0, index: 1)
    encoder.setBytes(&count, length: 4, index: 2)
    encoder.dispatchThreads(
      MTLSize(width: samples.count, height: 1, depth: 1),
      threadsPerThreadgroup: MTLSize(width: pipeline.threadExecutionWidth, height: 1, depth: 1))
    encoder.endEncoding()
    command.commit()
    command.waitUntilCompleted()
    if let error = command.error { throw error }
    let gpu = output.contents().assumingMemoryBound(to: SIMD4<Float>.self)
    var maximum: Float = 0
    for index in samples.indices {
      let expected = NightEnvironmentVerification.evaluate(samples[index])
      for channel in 0..<4 {
        let error = abs(gpu[index][channel] - expected[channel])
        guard gpu[index][channel].isFinite, error < 0.000002 else {
          throw RuntimeError.message(
            "CPU/GPU night environment mismatch at sample \(index), channel \(channel): \(expected[channel]) / \(gpu[index][channel])")
        }
        maximum = max(maximum, error)
      }
    }
    return maximum
  }

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
    var solarMaximum: Float = 0
    var nightMaximum: Float = 0
    for y in 0..<height {
      let v = (Float(y) + 0.5) / Float(height) * 2 - 1
      let ny = sin((v < 0 ? -1 : 1) * v * v * Float.pi * 0.5)
      let expected =
        SIMD3<Float>(repeating: (1 + ny) / 2) + SIMD3<Float>(0.18, 0.20, 0.12) * ((1 - ny) / 2)
      let expectedNight = NightEnvironmentVerification.clearSkyIrradianceOverPi(
        normal: SIMD3(sqrt(max(0, 1 - ny * ny)), ny, 0), rings: 128, azimuths: 256)
      for x in 0..<width {
        let actual = pixels[y * width + x]
        for channel in 0..<3 {
          guard actual[channel].isFinite else {
            throw RuntimeError.message("Nonfinite sky irradiance")
          }
          solarMaximum = max(solarMaximum, abs(actual[channel] - expected[channel]))
        }
        guard actual.w.isFinite else { throw RuntimeError.message("Nonfinite night irradiance") }
        nightMaximum = max(nightMaximum, abs(actual.w - expectedNight))
      }
    }
    guard solarMaximum < 0.009 else {
      throw RuntimeError.message("Diffuse sky quadrature error \(solarMaximum) exceeds 0.009")
    }
    guard nightMaximum < 0.0035 else {
      throw RuntimeError.message("Night environment quadrature error \(nightMaximum) exceeds 0.0035")
    }
    return max(solarMaximum, nightMaximum)
  }
}
