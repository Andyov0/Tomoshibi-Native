#if os(iOS)
import SwiftUI
import TomoshibiBasics
import UIKit

/// Placed under the app's root view: finds the window it is drawn in, tells
/// that window the appearance it is to take (`told`, nil for the system's),
/// and reports every change of the window's and its scene's appearance to
/// `look` (WindowLook).
///
/// The window is told through UIKit's own `overrideUserInterfaceStyle`
/// rather than SwiftUI's `preferredColorScheme`, which overrides the whole
/// scene: told this way, the scene goes on saying the system's appearance
/// during a call, and `.unspecified` hands the window back to it. And it is
/// this window and its scene that are asked, rather than the first scene
/// the application lists, which on an iPad or a phone with a second display
/// may be another.
///
/// Every report comes from UIKit's own change of traits, as it happens: the
/// photograph the system takes in the other appearance on the way into the
/// background is taken straight after that change, and the screens are in
/// that appearance in it only if they were told at once.
struct WindowLookReader: UIViewRepresentable {
    let look: WindowLook
    let told: ColorScheme?

    func makeUIView(context: Context) -> Probe {
        Probe(look: look)
    }

    func updateUIView(_ probe: Probe, context: Context) {
        probe.told = told
    }

    final class Probe: UIView {
        private let look: WindowLook
        private weak var watched: UIWindow?
        private var registrations: [(UITraitEnvironment & UITraitChangeObservable, UITraitChangeRegistration)] = []

        var told: ColorScheme? {
            didSet { if told != oldValue { tell() } }
        }

        init(look: WindowLook) {
            self.look = look
            super.init(frame: .zero)
            isUserInteractionEnabled = false
            isAccessibilityElement = false
            accessibilityElementsHidden = true
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError() }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard let window, window !== watched else { return }
            for (owner, registration) in registrations { owner.unregisterForTraitChanges(registration) }
            registrations = []
            watched = window
            let styles: [UITrait] = [UITraitUserInterfaceStyle.self]
            registrations.append((window, window.registerForTraitChanges(styles) { [weak self] (_: UIWindow, _) in
                self?.read()
            }))
            if let scene = window.windowScene {
                registrations.append((scene, scene.registerForTraitChanges(styles) { [weak self] (_: UIWindowScene, _) in
                    self?.read()
                }))
            }
            tell()
            #if DEBUG
            PhotographDebug.start(window.windowScene)
            #endif
        }

        /// The window's appearance as `told` says, and what that leaves it
        /// and its scene in, reported.
        private func tell() {
            guard let window = watched else { return }
            let style: UIUserInterfaceStyle = switch told {
            case .dark: .dark
            case .light: .light
            default: .unspecified
            }
            if window.overrideUserInterfaceStyle != style { window.overrideUserInterfaceStyle = style }
            read()
        }

        private func read() {
            guard let window = watched else { return }
            let scene = window.windowScene
            look.report(window: Self.scheme(window.traitCollection.userInterfaceStyle),
                        scene: scene.flatMap { Self.scheme($0.traitCollection.userInterfaceStyle) },
                        windowTold: window.overrideUserInterfaceStyle != .unspecified)
        }

        private static func scheme(_ style: UIUserInterfaceStyle) -> ColorScheme? {
            switch style {
            case .dark: .dark
            case .light: .light
            default: nil
            }
        }
    }
}

#if DEBUG
/// TOMOSHIBI_DEBUG_PHOTOGRAPH=<after>:<for>, honoured only when a test drives
/// the app (TOMOSHIBI_BASE set): <after> seconds after the first window is
/// shown, its scene takes the appearance that is not the system's for <for>
/// seconds and then gives it back, the screen's own appearance left as it
/// is. That is what the system does to photograph the app in the other
/// appearance on its way into the background, which a test cannot catch
/// as it happens; the screens drawn in the old appearance for that
/// photograph were what showed for a moment on coming back after the system
/// had changed (WindowLook).
@MainActor
enum PhotographDebug {
    private static var started = false

    static func start(_ scene: UIWindowScene?) {
        let environment = ProcessInfo.processInfo.environment
        guard !started, environment["TOMOSHIBI_BASE"] != nil, let scene,
              let times = environment["TOMOSHIBI_DEBUG_PHOTOGRAPH"]?.split(separator: ":").compactMap({ Double($0) }),
              times.count == 2 else { return }
        started = true
        Task { @MainActor [weak scene] in
            try? await Task.sleep(for: .seconds(times[0]))
            guard let scene else { return }
            scene.traitOverrides.userInterfaceStyle = scene.traitCollection.userInterfaceStyle == .dark ? .light : .dark
            try? await Task.sleep(for: .seconds(times[1]))
            scene.traitOverrides.remove(UITraitUserInterfaceStyle.self)
        }
    }
}
#endif
#endif
