import XCTest
@testable import FieldCore

final class WindReplayTests:XCTestCase {
    func testImportedResponseCannotExceedRendererDeformationBound() throws {
        var wind=WindSimulation();wind.response[0].x=0.36
        XCTAssertFalse(wind.validSnapshot)
        let data=try JSONEncoder().encode(WindSimulation())
        var json=try JSONSerialization.jsonObject(with:data) as! [String:Any]
        var previous=json["previousResponse"] as! [[Float]];previous[0]=[0,-0.36];json["previousResponse"]=previous
        let restored=try JSONDecoder().decode(WindSimulation.self,from:JSONSerialization.data(withJSONObject:json))
        XCTAssertFalse(restored.validSnapshot)
    }
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
