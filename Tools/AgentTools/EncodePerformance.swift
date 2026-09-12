import AVFoundation
import CoreGraphics
import Foundation
import ImageIO

// Encode exact renderer PNGs. This is an offline review artifact, not FPS evidence.
let arguments = CommandLine.arguments
guard arguments.count == 4, let fps = Int32(arguments[2]), fps > 0 else {
  fatalError("Usage: EncodePerformance frames-directory fps movie.mp4")
}
let directory = URL(fileURLWithPath: arguments[1])
let output = URL(fileURLWithPath: arguments[3])
let frames = try FileManager.default.contentsOfDirectory(
  at: directory, includingPropertiesForKeys: nil
)
.filter { $0.pathExtension == "png" }.sorted { $0.lastPathComponent < $1.lastPathComponent }
guard !frames.isEmpty, !FileManager.default.fileExists(atPath: output.path) else {
  fatalError("Need frames and a new output path")
}
let writer = try AVAssetWriter(outputURL: output, fileType: .mp4)
let input = AVAssetWriterInput(
  mediaType: .video,
  outputSettings: [
    AVVideoCodecKey: AVVideoCodecType.h264,
    AVVideoWidthKey: 1920, AVVideoHeightKey: 1080,
    AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 12_000_000],
  ])
input.expectsMediaDataInRealTime = false
let adapter = AVAssetWriterInputPixelBufferAdaptor(
  assetWriterInput: input,
  sourcePixelBufferAttributes: [
    kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
    kCVPixelBufferWidthKey as String: 1920, kCVPixelBufferHeightKey as String: 1080,
    kCVPixelBufferCGImageCompatibilityKey as String: true,
    kCVPixelBufferCGBitmapContextCompatibilityKey as String: true,
  ])
writer.add(input)
guard writer.startWriting() else { fatalError("Writer: \(String(describing: writer.error))") }
writer.startSession(atSourceTime: .zero)
for (index, url) in frames.enumerated() {
  while !input.isReadyForMoreMediaData {
    if writer.status == .failed { fatalError("Writer failed: \(String(describing: writer.error))") }
    Thread.sleep(forTimeInterval: 0.005)
  }
  try autoreleasepool {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
      let pool = adapter.pixelBufferPool
    else { throw NSError(domain: "Frames", code: 1) }
    var pixel: CVPixelBuffer?
    guard CVPixelBufferPoolCreatePixelBuffer(nil, pool, &pixel) == kCVReturnSuccess, let pixel
    else {
      throw NSError(domain: "Frames", code: 2)
    }
    CVPixelBufferLockBaseAddress(pixel, [])
    guard
      let context = CGContext(
        data: CVPixelBufferGetBaseAddress(pixel), width: 1920, height: 1080,
        bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(pixel),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue)
    else { fatalError("Pixel context") }
    context.draw(image, in: CGRect(x: 0, y: 0, width: 1920, height: 1080))
    CVPixelBufferUnlockBaseAddress(pixel, [])
    guard adapter.append(pixel, withPresentationTime: CMTime(value: Int64(index), timescale: fps))
    else {
      throw writer.error ?? NSError(domain: "Frames", code: 3)
    }
  }
}
input.markAsFinished()
let finished = DispatchSemaphore(value: 0)
writer.finishWriting { finished.signal() }
finished.wait()
guard writer.status == .completed else { fatalError("Writer: \(String(describing: writer.error))") }
print(output.path)
