import Foundation

/// Whether a test is driving the app, and so whether anything chosen during a
/// call may be written down: TOMOSHIBI_BASE, honoured only in a debug build,
/// the same rule as AppModel's `rehearsal`.
///
/// The tests drive the very app somebody uses, under the same bundle
/// identifier, so whatever a rehearsal writes to the standard defaults is the
/// owner's from then on. One run's `monitor50` step wrote half volume for the
/// music in the owner's own speakers into their defaults, and `noduck` would
/// turn the duck off in every call after it. So the choices a call makes as it goes -- the song
/// desk's quality, duck and monitor, the sound panel's book, the watched
/// video's volume -- go through `choices`, which in a rehearsal is nothing: a
/// test starts from the defaults, is not steered by whatever the owner chose,
/// and leaves nothing behind, as the device memory does for it
/// (`DeviceMemory.nothing`).
public enum Rehearsal {
    public static let running: Bool = {
        #if DEBUG
        ProcessInfo.processInfo.environment["TOMOSHIBI_BASE"] != nil
        #else
        false
        #endif
    }()

    /// Where a call's own choices are read from and kept: the person's
    /// defaults, or nowhere at all in a rehearsal.
    public static var choices: UserDefaults? { running ? nil : .standard }
}
