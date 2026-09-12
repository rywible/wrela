import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import Metal
import simd

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
