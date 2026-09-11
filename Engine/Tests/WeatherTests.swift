import XCTest
@testable import FieldCore

final class WeatherTests:XCTestCase {
    func testWaterBudgetAndStableEvolution() {
        var weather=MoistWeather(seed:4)
        let initial=weather.totalWater,original=weather.cells
        weather.advance(to:600,sun:0.6)
        XCTAssertEqual(weather.totalWater,initial+weather.evaporated-weather.precipitated,accuracy:initial*0.00002)
        XCTAssertTrue(weather.cells.allSatisfy {$0.x>=0 && $0.y>=0 && $0.z.isFinite && abs($0.w)<=8})
        XCTAssertGreaterThan(zip(original,weather.cells).reduce(Float(0)){$0+abs($1.0.y-$1.1.y)},10)
    }
    func testFixedTimeDoesNotDependOnFrameCadence() {
        var once=MoistWeather(seed:17),incremental=once
        once.advance(to:80,sun:0.3)
        for t in stride(from:Float(0),through:80,by:0.25) {incremental.advance(to:t,sun:0.3)}
        XCTAssertEqual(once.cells,incremental.cells)
        XCTAssertNotEqual(once.cells,MoistWeather(seed:83).cells)
        XCTAssertGreaterThan(MoistWeather.saturation(20),MoistWeather.saturation(10))
    }
}
