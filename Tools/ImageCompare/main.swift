import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct ImageError: LocalizedError {
  let errorDescription: String?
  init(_ text: String) { errorDescription = text }
}
func pixels(_ path: String) throws -> (Int, Int, [UInt8]) {
  guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
    let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
  else { throw ImageError("Cannot open PNG: \(path)") }
  let width = image.width
  let height = image.height
  guard width > 0, height > 0, width * height <= 40_000_000 else {
    throw ImageError("Image dimensions outside limits")
  }
  var bytes = [UInt8](repeating: 0, count: width * height * 4)
  try bytes.withUnsafeMutableBytes { buffer in
    guard
      let context = CGContext(
        data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { throw ImageError("Cannot create pixel context") }
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
  }
  return (width, height, bytes)
}
do {
  let args = CommandLine.arguments
  guard args.count == 4 else {
    throw ImageError("Usage: ImageCompare actual.png expected.png diff.png")
  }
  let (width, height, a) = try pixels(args[1])
  let (w, h, b) = try pixels(args[2])
  guard width == w, height == h else { throw ImageError("Image dimensions differ") }
  var delta = [UInt8](repeating: 255, count: a.count)
  var total: UInt64 = 0
  var maximum = 0
  var changed = 0
  for i in stride(from: 0, to: a.count, by: 4) {
    var any = false
    for c in 0..<3 {
      let d = abs(Int(a[i + c]) - Int(b[i + c]))
      total += UInt64(d)
      maximum = max(maximum, d)
      delta[i + c] = UInt8(min(255, d * 5))
      any = any || d > 0
    }
    if any { changed += 1 }
  }
  let image = CGImage(
    width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
    space: CGColorSpace(name: CGColorSpace.sRGB)!,
    bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
    provider: CGDataProvider(data: Data(delta) as CFData)!, decode: nil, shouldInterpolate: false,
    intent: .defaultIntent)!
  guard
    let destination = CGImageDestinationCreateWithURL(
      URL(fileURLWithPath: args[3]) as CFURL, UTType.png.identifier as CFString, 1, nil)
  else { throw ImageError("Cannot write diff") }
  CGImageDestinationAddImage(destination, image, nil)
  guard CGImageDestinationFinalize(destination) else { throw ImageError("Cannot finish diff") }
  let value: [String: Any] = [
    "meanAbsoluteError": Double(total) / Double(width * height * 3 * 255), "maximum": maximum,
    "changedPixels": changed, "exact": changed == 0, "diff": args[3],
  ]
  print(
    String(
      decoding: try JSONSerialization.data(withJSONObject: value, options: .sortedKeys),
      as: UTF8.self))
} catch {
  fputs("ImageCompare: \(error.localizedDescription)\n", stderr)
  exit(1)
}
