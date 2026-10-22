import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The Messages extension asks the deployment for meetings without the core,
 * and the deployment has to see what it sees from the core and from a
 * browser. What these guard: the arrangement's JSON in another key order or
 * with a time in another form (the server refuses a time without its zone),
 * the session left off or sent after a cookie value that would split the
 * header, a refusal's code lost on its way to the sentence, and a redirect to
 * another host followed with somebody's session on it.
 */
final class MeetingsAPITests: XCTestCase {
    override func tearDown() {
        Stub.reset()
        super.tearDown()
    }

    private func api(cookie: String? = "s3ssion", base: String = "http://127.0.0.1:8080/") -> MeetingsAPI {
        MeetingsAPI(base: URL(string: base)!, cookie: cookie, userAgent: "Tomoshibi/0.1.4 (iPhone; iOS 27.0; iPhone17,3)",
                    protocols: [Stub.self])
    }

    func testListsTheAccountsOwnMeetingsWithItsSession() async throws {
        Stub.answer = { _ in (200, #"{"meetings":[{"id":"m1","room":"standup","at":"2026-10-23T10:00:00Z","relay":"","started":true,"ended":"no","token":"tok","mine":true,"added":1}]}"#) }
        let found = try await api().mine()
        XCTAssertEqual(found, [Meeting(id: "m1", room: "standup", at: "2026-10-23T10:00:00Z", relay: nil, started: true,
                                       ended: false, token: "tok", mine: true)],
                       "read as leniently as the core reads: an empty relay is none, a field of the wrong type is absent")
        let asked = try XCTUnwrap(Stub.asked.first)
        XCTAssertEqual(asked.httpMethod, "GET")
        XCTAssertEqual(asked.url?.absoluteString, "http://127.0.0.1:8080/api/meetings")
        XCTAssertEqual(asked.value(forHTTPHeaderField: "Content-Type"), "application/json", "meeting.ts sends it on GETs as well")
        XCTAssertEqual(asked.value(forHTTPHeaderField: "Cookie"), "meet-live.account=s3ssion")
        XCTAssertEqual(asked.value(forHTTPHeaderField: "User-Agent"), "Tomoshibi/0.1.4 (iPhone; iOS 27.0; iPhone17,3)")
    }

    func testArrangesInTheWebsKeyOrderWithAnInstantInUTC() async throws {
        Stub.answer = { _ in (200, #"{"id":"m2","room":"amber-abbey-abide-0007","at":"2026-10-23T02:30:00Z","token":"t","mine":true,"started":false,"ended":false}"#) }
        let at = Date(timeIntervalSince1970: 1_792_722_600.25)
        let made = try await api().arrange(room: "amber-abbey-abide-0007", at: at)
        XCTAssertEqual(made.token, "t")
        let asked = try XCTUnwrap(Stub.asked.first)
        XCTAssertEqual(asked.httpMethod, "POST")
        XCTAssertEqual(asked.url?.path, "/api/meetings")
        XCTAssertEqual(Stub.bodies.first.flatMap { String(data: $0, encoding: .utf8) },
                       #"{"room":"amber-abbey-abide-0007","at":"2026-10-23T02:30:00.250Z","relay":""}"#)
    }

    func testAsksAfterALinkByItsTokenEscaped() async throws {
        Stub.answer = { _ in (200, #"{"id":"m3","room":"r","at":"2026-10-23T02:30:00Z","started":false,"ended":false}"#) }
        _ = try await api(cookie: nil).arranged(token: "a/b+c")
        let asked = try XCTUnwrap(Stub.asked.first)
        XCTAssertEqual(asked.url?.absoluteString, "http://127.0.0.1:8080/api/meetings/a%2Fb%2Bc")
        XCTAssertNil(asked.value(forHTTPHeaderField: "Cookie"), "nobody signed in sends no session")
    }

    func testRefusalsKeepTheirCode() async {
        func refusal(_ status: Int, _ body: String) async -> MeetingsFault? {
            Stub.answer = { _ in (status, body) }
            do {
                _ = try await api().mine()
                return nil
            } catch {
                return error as? MeetingsFault
            }
        }
        let signedOut = await refusal(401, #"{"error":"not_signed_in"}"#)
        XCTAssertEqual(signedOut, .refused(status: 401, code: "not_signed_in"))
        let absent = await refusal(404, "404 page not found\n")
        XCTAssertEqual(absent, .refused(status: 404, code: "not_found"), "Go's plain 404 is a feature not configured")
        let broken = await refusal(500, "")
        XCTAssertEqual(broken, .refused(status: 500, code: ""))
        let shapeless = await refusal(200, "[]")
        XCTAssertEqual(shapeless, .unexpected)
    }

    func testNothingAnsweringIsTheNetwork() async {
        Stub.fails = true
        do {
            _ = try await api().mine()
            XCTFail("an answer from nowhere")
        } catch {
            XCTAssertEqual(error as? MeetingsFault, .network)
        }
    }

    func testAKeptSessionThatWouldSplitTheHeaderIsNotSent() async throws {
        Stub.answer = { _ in (200, #"{"meetings":[]}"#) }
        _ = try await api(cookie: "abc; other=1").mine()
        XCTAssertNil(Stub.asked.first?.value(forHTTPHeaderField: "Cookie"))
    }

    func testARedirectToAnotherHostIsNotFollowed() async {
        Stub.answer = { request in
            request.url?.host == "127.0.0.1" ? (302, "elsewhere") : (200, #"{"meetings":[]}"#)
        }
        Stub.redirect = URL(string: "http://elsewhere.example.net/api/meetings")
        do {
            _ = try await api().mine()
            XCTFail("followed a redirect to another host")
        } catch {
            XCTAssertEqual(error as? MeetingsFault, .unexpected)
        }
        XCTAssertEqual(Stub.asked.compactMap(\.url?.host), ["127.0.0.1"], "the other host was never asked")
    }
}

/// Stands in for the network: records what was asked and answers as told.
final class Stub: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var answer: (URLRequest) -> (Int, String) = { _ in (500, "") }
    nonisolated(unsafe) static var redirect: URL?
    nonisolated(unsafe) static var fails = false
    nonisolated(unsafe) static var asked: [URLRequest] = []
    nonisolated(unsafe) static var bodies: [Data] = []

    static func reset() {
        answer = { _ in (500, "") }
        redirect = nil
        fails = false
        asked = []
        bodies = []
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.asked.append(request)
        Self.bodies.append(request.httpBody ?? request.httpBodyStream.map(Self.read) ?? Data())
        if Self.fails {
            client?.urlProtocol(self, didFailWithError: URLError(.cannotConnectToHost))
            return
        }
        let (status, body) = Self.answer(request)
        var headers = ["Content-Type": "application/json"]
        if (300 ..< 400).contains(status), let to = Self.redirect {
            headers["Location"] = to.absoluteString
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
            var next = URLRequest(url: to)
            next.httpMethod = request.httpMethod
            client?.urlProtocol(self, wasRedirectedTo: next, redirectResponse: response)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(body.utf8))
            client?.urlProtocolDidFinishLoading(self)
            return
        }
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(body.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    private static func read(_ stream: InputStream) -> Data {
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            guard count > 0 else { break }
            data.append(buffer, count: count)
        }
        return data
    }
}
