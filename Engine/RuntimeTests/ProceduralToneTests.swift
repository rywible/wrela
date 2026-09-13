import FieldEngine
import XCTest

final class ProceduralToneTests: XCTestCase {
  func testTonalGesturesAreBoundedAndFadeAtBothEnds() {
    for frequency: Float in [130, 280, 880, 1180] {
      let tone = ProceduralTone(frequency: frequency, endFrequency: frequency * 1.24, duration: 0.4)
      let samples = tone.samples()
      XCTAssertEqual(samples.count, 19_200)
      XCTAssertTrue(samples.allSatisfy { $0.isFinite && abs($0) <= 1 })
      XCTAssertEqual(samples.first!, 0, accuracy: 0.00001)
      XCTAssertEqual(samples.last!, 0, accuracy: 0.00001)
      XCTAssertGreaterThan(samples.map { abs($0) }.max()!, 0.4)
      XCTAssertEqual(samples, tone.samples())
    }
  }
  func testInvalidToneCannotAllocateUnboundedAudioOrPublishNaNs() {
    XCTAssertTrue(ProceduralTone(frequency: .nan, endFrequency: 200, duration: 1).samples().isEmpty)
    XCTAssertTrue(ProceduralTone(frequency: 200, endFrequency: 200, duration: .infinity).samples().isEmpty)
    XCTAssertEqual(ProceduralTone(frequency: 200, endFrequency: 400, duration: 1000).samples().count, 96_000)
  }
}
