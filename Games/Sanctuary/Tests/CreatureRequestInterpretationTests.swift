import XCTest

@testable import SanctuaryContent

final class CreatureRequestInterpretationTests: XCTestCase {
  func testNegatedAndContradictoryRequestsDoNotBecomeActions() {
    XCTAssertNil(AnimalRequestParser.parse("don't follow me"))
    XCTAssertNil(AnimalRequestParser.parse("do not come closer"))
    XCTAssertNil(AnimalRequestParser.parse("follow me but stay here"))
    XCTAssertFalse(AnimalRequestParser.permits(.follow, for: "don't follow me"))
  }

  func testMultipleRequestsAreRejectedRatherThanChoosingOne() {
    XCTAssertNil(AnimalRequestParser.parse("hello, then come and play"))
    XCTAssertNil(AnimalRequestParser.parse("wait here and follow me"))
    XCTAssertFalse(AnimalRequestParser.permits(.play, for: "come play with me"))
  }

  func testBenignLongGreetingKeepsAuthoredFallbackPlayable() {
    let greeting =
      "Hello there, my gentle friend. It is lovely to see you again on this quiet morning."
    XCTAssertEqual(AnimalRequestParser.parse(greeting), .greeting)
    XCTAssertTrue(AnimalRequestParser.permits(.greeting, for: greeting))
  }

  func testUnknownPhysicalActionsCannotBeRecastAsAllowedRequests() {
    XCTAssertNil(AnimalRequestParser.parse("please fly to me"))
    XCTAssertNil(AnimalRequestParser.parse("follow me and push that boulder"))
    XCTAssertFalse(AnimalRequestParser.permits(.come, for: "please fly to me"))
    XCTAssertFalse(AnimalRequestParser.permits(.follow, for: "follow me and push that boulder"))
  }

  func testExistingBoundedPhrasesRemainAvailable() {
    XCTAssertEqual(AnimalRequestParser.parse("please follow me"), .follow)
    XCTAssertEqual(AnimalRequestParser.parse("do you want to play please"), .play)
    XCTAssertEqual(AnimalRequestParser.parse("stay here"), .wait)
  }

  func testEveryAdvertisedInvitationParsesToItsSingleAuthoredRequest() {
    let advertised: [(String, AnimalRequest)] = [
      ("hello", .greeting),
      ("play with me", .play),
      ("come here", .come),
      ("follow me", .follow),
      ("wait here", .wait),
    ]
    for (phrase, expected) in advertised {
      XCTAssertEqual(AnimalRequestParser.parse(phrase), expected, phrase)
      XCTAssertTrue(AnimalRequestParser.permits(expected, for: phrase), phrase)
    }
    XCTAssertNil(AnimalRequestParser.parse("Play with me, but leave me alone."))
  }
}
