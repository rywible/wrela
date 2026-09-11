import Foundation

public struct Workload {
  public let name: String
  public let definition: String
  public let operation: () throws -> Double
  public init(_ name: String, definition: String? = nil, operation: @escaping () throws -> Double) {
    self.name = name
    self.definition = definition ?? name
    self.operation = operation
  }
}
public struct Measurement: Codable {
  public let name: String
  public let definitionDigest: String
  public let unit: String
  public let samples: Int
  public let median: Double
  public let p95: Double
  public let max: Double
  public let checksum: Double
}
public enum Performance {
  /// Keep a consumed result and time only the workload, never report writing or JSON encoding.
  public static func measure(_ workload: Workload, samples: Int = 20, warmup: Int = 3) throws
    -> Measurement
  {
    guard (3...1000).contains(samples), (0...100).contains(warmup) else {
      throw NSError(
        domain: "Performance", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Invalid sample count"])
    }
    for _ in 0..<warmup { _ = try workload.operation() }
    var times: [Double] = []
    var checksum = 0.0
    for _ in 0..<samples {
      let start = DispatchTime.now().uptimeNanoseconds
      checksum += try workload.operation()
      times.append(Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6)
    }
    times.sort()
    return Measurement(
      name: workload.name, definitionDigest: TestDigest.data(Data(workload.definition.utf8)),
      unit: "ms/workload", samples: samples, median: times[times.count / 2],
      p95: times[min(times.count - 1, Int(ceil(Double(times.count) * 0.95)) - 1)], max: times.last!,
      checksum: checksum)
  }
}
