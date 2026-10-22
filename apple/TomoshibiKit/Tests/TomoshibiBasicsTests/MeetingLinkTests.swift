import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * One link is opened by a browser, by every app and by the deployment's
 * universal-link pattern, so a card made by the Messages extension has to
 * carry exactly the link the web and the core would have made. What these
 * guard: a token escaped Foundation's way rather than encodeURIComponent's
 * (a `+` arriving as a space, a `/` or `&` ending the item early), the app's
 * scheme losing the port a test deployment is on, and a link read back as a
 * different deployment from the one it names.
 */
final class MeetingLinkTests: XCTestCase {
    func testEscapesAsEncodeURIComponentDoes() {
        // What a browser's encodeURIComponent gives for each of these.
        XCTAssertEqual(MeetingLink.encodeComponent("a b&c=d+e/f?g#h"), "a%20b%26c%3Dd%2Be%2Ff%3Fg%23h")
        XCTAssertEqual(MeetingLink.encodeComponent("-_.!~*'()"), "-_.!~*'()")
        XCTAssertEqual(MeetingLink.encodeComponent("日本"), "%E6%97%A5%E6%9C%AC")
        XCTAssertEqual(MeetingLink.encodeComponent("AZaz09"), "AZaz09")
        XCTAssertEqual(MeetingLink.encodeComponent("%"), "%25")
    }

    func testAMeetingsLinkIsTheWebsAndTheCoresForm() {
        XCTAssertEqual(MeetingLink.link(.meeting, base: "https://meet.example.com/", token: "t/1+2", room: "standup")?.absoluteString,
                       "https://meet.example.com/?meeting=t%2F1%2B2#/standup")
        XCTAssertEqual(MeetingLink.link(.invite, base: "https://meet.example.com", token: "abc", room: "standup")?.absoluteString,
                       "https://meet.example.com/?invite=abc#/standup", "a base without its slash is given one")
    }

    func testTheAppsSchemeKeepsTheHostAndThePort() throws {
        let opened = MeetingLink.Opened(kind: .meeting, token: "t/1+2", room: "standup", base: "https://meet.example.com/")
        XCTAssertEqual(MeetingLink.inApp(opened)?.absoluteString, "tomoshibi://meet.example.com/?meeting=t%2F1%2B2#/standup")
        let local = MeetingLink.Opened(kind: .invite, token: "x", room: "", base: "http://127.0.0.1:8080/")
        XCTAssertEqual(MeetingLink.inApp(local)?.absoluteString, "tomoshibi://127.0.0.1:8080/?invite=x",
                       "no room is no fragment, as landing.ts leaves it out")
    }

    func testReadsBackWhatItMade() throws {
        let link = try XCTUnwrap(MeetingLink.link(.meeting, base: "http://127.0.0.1:8080/", token: "t/1+2 ", room: "a-b-c-0001"))
        let opened = try XCTUnwrap(MeetingLink.read(link))
        XCTAssertEqual(opened, MeetingLink.Opened(kind: .meeting, token: "t/1+2", room: "a-b-c-0001", base: "http://127.0.0.1:8080/"))
        let inApp = try XCTUnwrap(MeetingLink.inApp(opened))
        XCTAssertEqual(MeetingLink.read(inApp)?.base, "https://127.0.0.1:8080/", "the app's scheme stands for https")
    }

    func testReadsLinksAsTheLandingPageReadsThem() {
        func read(_ text: String) -> MeetingLink.Opened? { MeetingLink.read(URL(string: text)!) }
        XCTAssertEqual(read("tomoshibi://Meet.Example.com/?invite=abc#/r")?.base, "https://meet.example.com/")
        XCTAssertEqual(read("https://meet.example.com/?meeting=m&invite=i#/r")?.kind, .invite, "an invitation before a meeting")
        XCTAssertNil(read("https://meet.example.com/?invite=%20#/r"), "an empty token is no link")
        XCTAssertNil(read("https://meet.example.com/#/room"), "a room's own address is not an invitation")
        XCTAssertNil(read("ftp://meet.example.com/?invite=abc"))
        XCTAssertEqual(read("https://meet.example.com/?meeting=m")?.room, "")
    }
}

/// The user agent the extension sends, against the core's own cases
/// (core/protocol/src/api/agent.rs tests).
final class ClientAgentTests: XCTestCase {
    func testSaysWhatTheCoreSays() {
        XCTAssertEqual(ClientAgent.userAgent(platform: "iPhone", version: "0.1.0.8", system: "iOS 27.0.1", device: "iPhone17,1"),
                       "Tomoshibi/0.1.0.8 (iPhone; iOS 27.0.1; iPhone17,1)")
        XCTAssertEqual(ClientAgent.userAgent(platform: "Android", version: "0.1 (2)", system: "Android 16; beta", device: "小米 (Pro)"),
                       "Tomoshibi/0.1.2 (Android; Android 16 beta; Pro)")
        XCTAssertEqual(ClientAgent.userAgent(platform: "Windows", version: "0.1.0.1", system: "", device: ""),
                       "Tomoshibi/0.1.0.1 (Windows)")
    }

    func testTheVersionIsTheCoresVersionWithBuild() {
        XCTAssertEqual(ClientAgent.version("0.1.4", build: "1"), "0.1.4")
        XCTAssertEqual(ClientAgent.version("0.1.4", build: "2"), "0.1.4.2")
        XCTAssertEqual(ClientAgent.version(" 0.1.4 ", build: "seven"), "0.1.4", "a build that is not a number is the first")
    }
}
