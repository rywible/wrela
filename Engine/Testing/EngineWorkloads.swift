import FieldCompiler
import FieldCore
import Foundation
import TestKit

public enum EngineWorkloads {
  public static func all() -> [Workload] {
    let shape = Shape.smoothUnion(
      .translated(.sphere(0.8), V3(-0.4, 0, 0)), .translated(.sphere(0.65), V3(0.4, 0.2, 0)), 0.2)
    var wind = WindSimulation()
    return [
      Workload("mesh-32") {
        let mesh = try Mesher.compile(shape, resolution: 32)
        return Double(mesh.indices.count)
      },
      Workload("field-10000") {
        var total: Float = 0
        for i in 0..<10000 {
          total += shape.value(
            at: V3(Float(i % 100) * 0.03 - 1.5, Float(i / 100) * 0.03 - 1.5, 0.1))
        }
        return Double(total)
      },
      Workload("wind-60-ticks") {
        for _ in 0..<60 { wind.step(1 / 60) }
        return Double(wind.flow[0].x)
      },
    ]
  }
}
