import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import Metal
import simd

extension WorkshopRenderer {
  /// Tests the exact shared fragment coverage math as a compute function.
  /// Raster MSAA and shadow-image appearance still require native inspection.
  func verifyGroomCoverage() throws -> Float {
    guard MemoryLayout<Vertex>.stride==64 else {throw RuntimeError.message("Groom vertex ABI must contain four aligned float4 values")}
    let url=Bundle.main.url(forResource:"Surface",withExtension:"metal")
      ?? ProjectContext.workspace.appendingPathComponent("Engine/FieldEngine/Resources/Surface.metal")
    let source=try String(contentsOf:url,encoding:.utf8)
    guard let begin=source.range(of:"// GROOM_COVERAGE_BEGIN"),let end=source.range(of:"// GROOM_COVERAGE_END")
    else {throw RuntimeError.message("Missing production groom coverage function")}
    let kernel="#include <metal_stdlib>\nusing namespace metal;\n"+String(source[begin.upperBound..<end.lowerBound])+"""
    kernel void verifyCoverage(uint id [[thread_position_in_grid]],const device float4 *coordinates [[buffer(0)]],const device float4 *footprints [[buffer(1)]],device float *output [[buffer(2)]]) {
      output[id]=groomCoverageAt(coordinates[id],footprints[id].xy);
    }
    """
    let library=try device.makeLibrary(source:kernel,options:nil)
    let pipeline=try device.makeComputePipelineState(function:library.makeFunction(name:"verifyCoverage")!)
    var coordinates:[SIMD4<Float>]=[],footprints:[SIMD4<Float>]=[]
    for i in 0..<8192 {
      let seed=UInt32(i)
      coordinates.append(i%23==0 ? .zero:SIMD4(GroomCoverage.random(seed)*4600,
        Float(i%257)/256,0.42+GroomCoverage.random(seed &+ 7)*0.46,0.12+GroomCoverage.random(seed &+ 31)*0.6))
      let width:Float=[0.002,0.03,0.15,0.7,1.4,8,32][i%7]
      footprints.append(SIMD4(width,Float(i%9)*0.003,0,0))
    }
    let input=device.makeBuffer(bytes:coordinates,length:coordinates.count*16)!
    let footprintBuffer=device.makeBuffer(bytes:footprints,length:footprints.count*16)!
    let output=device.makeBuffer(length:coordinates.count*4,options:.storageModeShared)!
    let command=queue.makeCommandBuffer()!,encoder=command.makeComputeCommandEncoder()!
    encoder.setComputePipelineState(pipeline);encoder.setBuffer(input,offset:0,index:0)
    encoder.setBuffer(footprintBuffer,offset:0,index:1);encoder.setBuffer(output,offset:0,index:2)
    encoder.dispatchThreads(MTLSize(width:coordinates.count,height:1,depth:1),threadsPerThreadgroup:MTLSize(width:pipeline.threadExecutionWidth,height:1,depth:1))
    encoder.endEncoding();command.commit();command.waitUntilCompleted()
    if let error=command.error {throw error}
    let gpu=output.contents().assumingMemoryBound(to:Float.self)
    var worst:Float=0
    for i in coordinates.indices {
      let cpu=GroomCoverage.sample(coordinates[i],footprint:SIMD2(footprints[i].x,footprints[i].y))
      let error=abs(cpu-gpu[i])
      guard gpu[i].isFinite && error<0.0005 else {
        throw RuntimeError.message("CPU/GPU procedural strand coverage mismatch at sample \(i): \(cpu) / \(gpu[i])")
      }
      worst=max(worst,error)
    }
    return worst
  }
}
