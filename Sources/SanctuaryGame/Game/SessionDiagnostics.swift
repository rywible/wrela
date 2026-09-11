import FieldCompiler
import MetalKit

// Compatibility for the existing inspector/protocol. GPU state is owned only by graphics.
extension SanctuarySession {
  var device: MTLDevice { graphics.device }
  var queue: MTLCommandQueue { graphics.queue }
  var atmosphere: Atmosphere {
    get { graphics.atmosphere }
    set { graphics.atmosphere = newValue }
  }
  var frame: Int {
    get { graphics.frame }
    set { graphics.frame = newValue }
  }
  var useMeshlets: Bool {
    get { graphics.useMeshlets }
    set { graphics.useMeshlets = newValue }
  }
  var profiling: Bool {
    get { graphics.profiling }
    set { graphics.profiling = newValue }
  }
  var shaderDigest: String {
    get { graphics.shaderDigest }
    set { graphics.shaderDigest = newValue }
  }
  var gpuErrors: [String] {
    get { graphics.gpuErrors }
    set { graphics.gpuErrors = newValue }
  }
  var gpuTimes: [Double] {
    get { graphics.gpuTimes }
    set { graphics.gpuTimes = newValue }
  }
  var gpuPassTimes: [String: [Double]] {
    get { graphics.gpuPassTimes }
    set { graphics.gpuPassTimes = newValue }
  }
  var gpuPhaseTimes: [String: [Double]] {
    get { graphics.gpuPhaseTimes }
    set { graphics.gpuPhaseTimes = newValue }
  }
  var cpuTimes: [Double] {
    get { graphics.cpuTimes }
    set { graphics.cpuTimes = newValue }
  }
  var simulationTimes: [Double] {
    get { graphics.simulationTimes }
    set { graphics.simulationTimes = newValue }
  }
  var atmosphereCPUTimes: [Double] {
    get { graphics.atmosphereCPUTimes }
    set { graphics.atmosphereCPUTimes = newValue }
  }
  var completionTimes: [Double] {
    get { graphics.completionTimes }
    set { graphics.completionTimes = newValue }
  }
  var presentedIntervals: [Double] {
    get { graphics.presentedIntervals }
    set { graphics.presentedIntervals = newValue }
  }
  var presentedFrames: Int {
    get { graphics.presentedFrames }
    set { graphics.presentedFrames = newValue }
  }
  var frameTimes: [Double] {
    get { graphics.frameTimes }
    set { graphics.frameTimes = newValue }
  }
  var drawableWaitTimes: [Double] {
    get { graphics.drawableWaitTimes }
    set { graphics.drawableWaitTimes = newValue }
  }
  var visibleTriangles: Int {
    get { graphics.visibleTriangles }
    set { graphics.visibleTriangles = newValue }
  }
  var shadowTriangles: Int {
    get { graphics.shadowTriangles }
    set { graphics.shadowTriangles = newValue }
  }
  var captureRequest: (URL, [String: Any], (Result<URL, Error>) -> Void)? {
    get { graphics.captureRequest }
    set { graphics.captureRequest = newValue }
  }
  func upload(_ batch: SceneBatch) -> GPUBatch { graphics.upload(batch) }
  func buildPipelines(sourceURL: URL? = nil) throws {
    try graphics.buildPipelines(sourceURL: sourceURL)
  }
  func clearMetrics() { graphics.clearMetrics() }
  func stats(_ values: [Double]) -> [String: Double] { graphics.stats(values) }
}
