import Foundation
@testable import TomoshibiBasics
import XCTest

/*
 * The device memory, the share's frame rates and the microphone meter, held
 * to what the web writes, reads and offers under the same names
 * (web/src/live/preferences.ts and preferences.test.ts, live/sound.ts,
 * components/room/ShareButton.tsx and its tests, MicLevel.tsx). What these
 * guard is a call that starts other than as it was left: a camera on that was
 * left off, a garbled record read as a silent microphone, a 240 carried onto
 * 4K, a test run that rewrote the owner's own choices.
 */
final class DevicesTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suite = "tomoshibi.tests.devices"

    override func setUp() {
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    func testStartsWithTheMicrophoneOnAndTheCameraOffUntilToldOtherwise() {
        let memory = DeviceMemory(defaults: defaults)
        XCTAssertEqual(memory.joining, DeviceMemory.Joining(camera: false, microphone: true))
        memory.remember(DeviceMemory.Joining(camera: true, microphone: false))
        // Byte for byte what the web's JSON.stringify writes.
        XCTAssertEqual(defaults.string(forKey: "meet-live.devices"), #"{"camera":true,"microphone":false}"#)
        XCTAssertEqual(DeviceMemory(defaults: defaults).joining, DeviceMemory.Joining(camera: true, microphone: false))
    }

    func testReadsWhatItDoesNotRecogniseAsTheDefaultsOneHalfAtATime() {
        let memory = DeviceMemory(defaults: defaults)
        for garbage in ["", "nope", "[]", "null", "42", #"{"camera":"yes","microphone":0}"#] {
            defaults.set(garbage, forKey: "meet-live.devices")
            XCTAssertEqual(memory.joining, .first, garbage)
        }
        defaults.set(#"{"camera":true}"#, forKey: "meet-live.devices")
        XCTAssertEqual(memory.joining, DeviceMemory.Joining(camera: true, microphone: true))
        // A 1 is a number, not a yes: only a boolean counts, as on the web.
        defaults.set(#"{"camera":1,"microphone":false}"#, forKey: "meet-live.devices")
        XCTAssertEqual(memory.joining, DeviceMemory.Joining(camera: false, microphone: false))
    }

    func testRemembersEachKindBesideTheOthersInTheWebsShape() throws {
        let memory = DeviceMemory(defaults: defaults)
        XCTAssertNil(memory.choice(.microphone))
        memory.remember(.microphone, id: "BuiltInMicrophoneDevice")
        memory.remember(.camera, id: "6C707041-05AC-0011-0006-000000000001")
        memory.remember(.microphone, id: "AppleUSBAudioEngine:Headset")
        XCTAssertEqual(memory.choice(.microphone), "AppleUSBAudioEngine:Headset")
        XCTAssertEqual(memory.choice(.camera), "6C707041-05AC-0011-0006-000000000001")
        XCTAssertNil(memory.choice(.speakers))
        let stored = try XCTUnwrap(defaults.string(forKey: "meet-live.device-choice"))
        XCTAssertEqual(stored, #"{"audioinput":"AppleUSBAudioEngine:Headset","videoinput":"6C707041-05AC-0011-0006-000000000001"}"#)
    }

    func testKeepsOnlyWhatCouldBeADevice() {
        let memory = DeviceMemory(defaults: defaults)
        defaults.set(#"{"audioinput":"","videoinput":42,"audiooutput":"\#(String(repeating: "x", count: 257))"}"#, forKey: "meet-live.device-choice")
        for kind in DeviceMemory.Kind.allCases { XCTAssertNil(memory.choice(kind), kind.rawValue) }
        memory.remember(.speakers, id: String(repeating: "y", count: 257))
        XCTAssertNil(memory.choice(.speakers))
        defaults.set("[1,2]", forKey: "meet-live.device-choice")
        XCTAssertNil(memory.choice(.microphone))
    }

    func testAsksForARememberedDeviceOnlyWhereItIsStillThere() {
        let memory = DeviceMemory(defaults: defaults)
        memory.remember(.camera, id: "desk")
        XCTAssertEqual(memory.restorable(.camera, present: ["laptop", "desk"]), "desk")
        XCTAssertNil(memory.restorable(.camera, present: ["laptop"]))
        XCTAssertNil(memory.restorable(.camera, present: []))
        // Still written down for the day it is plugged back in.
        XCTAssertEqual(memory.choice(.camera), "desk")
    }

    func testOriginalSoundIsOnOnlyWhereItWasAskedFor() {
        let memory = DeviceMemory(defaults: defaults)
        XCTAssertFalse(memory.originalSound)
        memory.remember(originalSound: true)
        XCTAssertEqual(defaults.string(forKey: "meet-live.original-sound"), "on")
        XCTAssertTrue(memory.originalSound)
        memory.remember(originalSound: false)
        // Off is written as nothing, which older builds read the same way.
        XCTAssertNil(defaults.object(forKey: "meet-live.original-sound"))
        defaults.set("yes", forKey: "meet-live.original-sound")
        XCTAssertFalse(memory.originalSound)
    }

    func testARehearsalKeepsNothingAndRemembersNothing() {
        defaults.set(#"{"camera":true,"microphone":false}"#, forKey: "meet-live.devices")
        let memory = DeviceMemory.nothing
        XCTAssertEqual(memory.joining, .first)
        memory.remember(DeviceMemory.Joining(camera: true, microphone: true))
        memory.remember(.camera, id: "desk")
        memory.remember(originalSound: true)
        XCTAssertNil(memory.choice(.camera))
        XCTAssertFalse(memory.originalSound)
        XCTAssertEqual(defaults.string(forKey: "meet-live.devices"), #"{"camera":true,"microphone":false}"#)
    }

    func testOffersOnlyTheFrameRatesTheChosenSizeCanCarry() {
        XCTAssertEqual(ShareRates.allowed(shorterSide: 1080), [15, 30, 60, 120, 240])
        XCTAssertEqual(ShareRates.allowed(shorterSide: 1440), [15, 30, 60, 120])
        XCTAssertEqual(ShareRates.allowed(shorterSide: 2160), [15, 30, 60])
        XCTAssertEqual(ShareRates.allowed(shorterSide: 0), [60])
    }

    func testDoesNotCarryAFastRateOntoASizeThatCannotTakeIt() {
        XCTAssertEqual(ShareRates.clamp(240, shorterSide: 2160), 60)
        XCTAssertEqual(ShareRates.clamp(240, shorterSide: 1440), 120)
        XCTAssertEqual(ShareRates.clamp(30, shorterSide: 2160), 30)
        XCTAssertEqual(ShareRates.clamp(240, shorterSide: 1080), 240)
        XCTAssertEqual(ShareRates.clamp(45, shorterSide: 1080), 240)
        XCTAssertEqual(ShareRates.clamp(15, shorterSide: 0), 60)
    }

    func testTheMeterPutsAVoiceHighAndAQuietRoomBelowTheHeardLine() {
        XCTAssertEqual(MicrophoneLevel.shown(rms: 0), 0)
        XCTAssertEqual(MicrophoneLevel.shown(rms: 1), 1)
        // Speech at about -26 dBFS.
        XCTAssertGreaterThan(MicrophoneLevel.shown(rms: 0.05), 0.5)
        // A quiet room after voice processing, about -66 dBFS.
        XCTAssertLessThan(MicrophoneLevel.shown(rms: 0.0005), MicrophoneLevel.heard)
        XCTAssertGreaterThan(MicrophoneLevel.shown(rms: 0.003), MicrophoneLevel.heard)
    }
}
