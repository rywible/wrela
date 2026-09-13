import XCTest

@testable import SanctuaryContent

final class NatureTimingTests: XCTestCase {
  func testExpeditionStampsNatureActionsWithSavedPlayTime() throws {
    var expedition = Expedition()
    expedition.advanceClock(seconds: 0.75)
    _ = try expedition.applyNature(.plant(.grove, at: .init(x: 0, z: 0), radius: 3))
    XCTAssertEqual(
      try XCTUnwrap(try XCTUnwrap(expedition.garden?.patches.first).createdAtPlaySeconds),
      0.75)
    XCTAssertNoThrow(try expedition.validate())
  }
}
