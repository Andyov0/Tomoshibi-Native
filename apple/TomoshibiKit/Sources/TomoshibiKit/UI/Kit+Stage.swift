import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

// The pieces the stage is built from that are not a picture: what the room
// remembers between redraws, when the controls may step aside, the window's
// full screen, and the small glass controls drawn over pictures.

// MARK: - What the stage remembers

/// The stage's record of one call: who spoke last, whether this participant
/// is speaking, and whether a plain link lets anybody in.
///
/// Kept in the call's `parts` rather than in the view, for the reason
/// WatchFeature gives: the room view's initialiser runs on every redraw of the
/// screen around it, and a listener made there would be made again each time.
@MainActor
@Observable
final class StageRecord {
    static let part = "stage"

    /// When each person was last heard (useSpeakingOrder.ts).
    private(set) var speaking = SpeakingRecord()
    /// Whether this participant is speaking, from the same event that says it
    /// of everybody else: the web marks your own picture as it marks theirs.
    private(set) var meSpeaking = false
    /// Who may enter a room that already exists, asked once for the call
    /// (useJoining.ts); nil until the deployment has answered, so nothing is
    /// drawn on a guess and then taken away.
    private(set) var joining: Joining?

    @ObservationIgnored private unowned let call: CallModel

    static func of(_ call: CallModel) -> StageRecord {
        if let kept = call.parts[part] as? StageRecord { return kept }
        let made = StageRecord(call: call)
        call.parts[part] = made
        return made
    }

    private init(call: CallModel) {
        self.call = call
        let me = call.localIdentity
        call.listen { [weak self] event in
            guard let self, case let .speaking(identities) = event else { return }
            self.meSpeaking = identities.contains(me)
            self.speaking.hear(speaking: identities, present: self.call.identities, at: Date().timeIntervalSince1970)
        }
        Task { [weak self] in
            // A deployment that will not say is read as the one every
            // deployment starts as, as the web's `deployment()` falls back to
            // PLAIN: a page that cannot ask must not invent a restriction.
            let said = (try? await call.deployment.deploymentInfo())?.joinedBy ?? .anyone
            self?.joining = StageDebug.joining ?? said
        }
    }

    /// Whether a plain link to this room is a way into it (useJoining.ts
    /// `useLinkWorks`): only where anybody may use a room that exists, and
    /// never for a room under a scope, whose members can already name it and
    /// whose other people need an invitation.
    var linkWorks: Bool {
        joining == .anyone && scopeOf(name: call.room).isEmpty
    }

    /// The room's plain address, `{base}/#/{room}`, which is what the web
    /// copies from its address bar: the base as the deployment gives it, so a
    /// deployment behind a path or another port keeps them.
    var link: String {
        var base = call.deployment.baseUrl()
        while base.hasSuffix("/") { base.removeLast() }
        return base + "/#/" + call.room
    }
}

/// Puts words on the pasteboard, on either system.
enum Pasteboard {
    @MainActor static func copy(_ text: String) {
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        #endif
    }
}

// MARK: - Whether anybody is there

/// Whether nobody has touched anything for a while (hooks/useIdle.ts).
///
/// Only the pointer moving, a click, a key or a touch count; not the frames
/// of a picture, because a call where somebody is talking and nobody is
/// touching anything is exactly the call whose controls should get out of the
/// way. Movement is written to an unobserved field, so a pointer crossing the
/// window redraws nothing until it wakes something that was asleep.
@MainActor
@Observable
final class IdleWatch {
    private(set) var idle = false
    @ObservationIgnored private var last = Date()
    #if os(macOS)
    @ObservationIgnored private var monitor: Any?
    #endif

    func poke() {
        last = Date()
        if idle { idle = false }
    }

    /// Watches until the task it runs in is cancelled, which is the room
    /// view going or the controls no longer being allowed to step aside.
    func run() async {
        poke()
        while !Task.isCancelled {
            try? await Task.sleep(for: .milliseconds(250))
            if !idle, Date().timeIntervalSince(last) >= Plan.idleAfter { idle = true }
        }
        idle = false
    }

    #if os(macOS)
    /// Keys, clicks and the wheel, from anywhere in the app: listened for
    /// before any view takes them, as the web listens on the window with
    /// capture, so a press that a picture or a field swallows still counts as
    /// somebody being there.
    func listen() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .leftMouseDown, .rightMouseDown, .scrollWheel, .mouseMoved]) { [weak self] event in
            MainActor.assumeIsolated { self?.poke() }
            return event
        }
    }

    func stopListening() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
    #endif
}

/// The last press on a picture, for telling a double press from two single
/// ones. Not observed: remembering a press redraws nothing.
@MainActor
final class Presses {
    var last: (id: String, at: Date)?

    /// The system's own interval on the Mac, which the person may have set;
    /// on a touch screen the interval UIKit's own double tap uses.
    static var interval: TimeInterval {
        #if os(macOS)
        NSEvent.doubleClickInterval
        #else
        0.3
        #endif
    }
}

// MARK: - The window's full screen

#if os(macOS)
/// The window a call is in, put into full screen for the stage and taken out
/// of it again (the web's useFullscreen.ts, with the window for the element).
///
/// Its state is read from the window rather than remembered, as the web reads
/// the document's, because there are ways out this never hears about: the
/// green button, the View menu, the system's own shortcut. And only a full
/// screen this entered is left again by leaving the stage: somebody whose
/// window was already full screen keeps it.
@MainActor
@Observable
final class WindowFill {
    @ObservationIgnored private weak var window: NSWindow?
    @ObservationIgnored private var entered = false
    @ObservationIgnored private var observers: [NSObjectProtocol] = []
    /// How many times the window has left full screen, by any way at all,
    /// for the room to follow with `onChange`.
    ///
    /// A count rather than a closure handed in: the room's closure held the
    /// room view, whose state holds this object, so the two kept each other
    /// and with them the call's model and the core's call, every one of its
    /// loops still running, for every call made since the app started.
    private(set) var exits = 0

    func attach(_ window: NSWindow?) {
        guard window !== self.window else { return }
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
        self.window = window
        guard let window else { return }
        observers.append(NotificationCenter.default.addObserver(forName: NSWindow.didExitFullScreenNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.entered = false
                self?.exits += 1
            }
        })
    }

    var isFull: Bool { window?.styleMask.contains(.fullScreen) == true }

    func enter() {
        guard let window, !isFull else { return }
        entered = true
        window.toggleFullScreen(nil)
    }

    func leave() {
        guard entered, let window, isFull else {
            entered = false
            return
        }
        entered = false
        window.toggleFullScreen(nil)
    }
}

/// Hands over the window this view ends up in.
struct WindowReader: NSViewRepresentable {
    let found: (NSWindow?) -> Void

    func makeNSView(context: Context) -> Reader {
        let view = Reader()
        view.found = found
        return view
    }

    func updateNSView(_ view: Reader, context: Context) {}

    final class Reader: NSView {
        var found: ((NSWindow?) -> Void)?
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            let window = window
            DispatchQueue.main.async { [found] in found?(window) }
        }
    }
}
#endif

// MARK: - Glass over a picture

/// A round control laid over a picture: dark glass, so it reads on a bright
/// slide and on a black one alike, lit a step under the pointer and another
/// held down, and pressed over a fingertip on a touch screen.
struct PictureButtonStyle: ButtonStyle {
    var size: CGFloat = 36
    var words = false

    func makeBody(configuration: Configuration) -> some View {
        PictureButtonBody(configuration: configuration, size: size, words: words)
    }

    private struct PictureButtonBody: View {
        let configuration: Configuration
        let size: CGFloat
        let words: Bool
        @State private var hovering = false
        @Environment(\.accessibilityReduceMotion) private var still

        var body: some View {
            let shape = words ? AnyShape(Capsule()) : AnyShape(Circle())
            let held = configuration.isPressed
            configuration.label
                .padding(.horizontal, words ? 12 : 0)
                .frame(minWidth: size, minHeight: size)
                .frame(height: size)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(Color.white)
                // Dark glass rather than clear: over a bright slide clear glass
                // is a white blur with white words on it.
                .background {
                    ZStack {
                        shape.fill(.ultraThinMaterial)
                        shape.fill(Color.black.opacity(0.38))
                        shape.fill(Color.white.opacity(held ? Theme.Press.quietHeld : hovering ? Theme.Press.quietHover : 0))
                    }
                    .environment(\.colorScheme, .dark)
                }
                .overlay(shape.stroke(Color.white.opacity(0.14), lineWidth: 1))
                .contentShape(shape)
                .fingertip(words ? Theme.fingertip : size, size)
                .scaleEffect(held && !still ? Theme.Press.scale : 1)
                .onHover { hovering = $0 }
                .animation(.easeOut(duration: 0.12), value: hovering)
                .animation(Theme.Press.animation, value: held)
        }
    }
}

extension ButtonStyle where Self == PictureButtonStyle {
    static func overPicture(_ size: CGFloat = 36, words: Bool = false) -> PictureButtonStyle {
        PictureButtonStyle(size: size, words: words)
    }
}

/// One of the paging arrows either side of the pictures (Plane.tsx
/// `PageButton`), looping, so the page indicator is what says where somebody
/// is. At full strength: at sixty per cent until the pointer came, they read
/// as arrows that could not be pressed, and a finger never came (the web's
/// went the same way, ui-polish 2026-10-29).
struct PageArrow: View {
    let symbol: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .bold))
                .foregroundStyle(Theme.foreground)
                .frame(width: 38, height: 38)
                .contentShape(Circle())
                .glassSurface(Circle(), interactive: true)
                .fingertip(38)
        }
        .buttonStyle(Pressable())
        .accessibilityLabel(label)
        .help(label)
    }
}

/// Which page is showing, "2 / 3". At the top rather than the bottom, as the
/// web learned: centred at the bottom it was under the controls on every
/// size, and the arrows loop, so nothing else says where somebody is.
struct PageIndicator: View {
    let page: Int
    let pages: Int

    var body: some View {
        Text("\(page + 1) / \(pages)")
            .font(.system(size: 11.5, weight: .semibold).monospacedDigit())
            .foregroundStyle(Theme.foreground.opacity(0.85))
            .contentTransition(.numericText(value: Double(page)))
            .padding(.horizontal, 10)
            .frame(height: 24)
            .glassSurface(Capsule())
            .animation(Theme.settle, value: page)
    }
}
