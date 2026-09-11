import XCTest
@testable import FieldCore

final class WindReplayTests:XCTestCase {
    func testSavedWindRestoresPoseAndFutureGusts() throws {
        var original=WindSimulation()
        for _ in 0..<241 {original.step(1/60)}
        var restored=try JSONDecoder().decode(WindSimulation.self,from:JSONEncoder().encode(original))
        XCTAssertTrue(restored.validSnapshot)
        XCTAssertEqual(original.gpuCells,restored.gpuCells)
        for _ in 0..<480 {original.step(1/60);restored.step(1/60)}
        XCTAssertEqual(original.gpuCells,restored.gpuCells)
        restored.flow.removeLast();XCTAssertFalse(restored.validSnapshot)
    }
}
