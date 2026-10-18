import AVFoundation
import SwiftUI
import TomoshibiBasics
import TomoshibiCore

/// What a picture says about whoever is in it, beside the picture
/// (components/room/Tile.tsx's props).
struct TileMarks: Equatable {
    var label: String
    /// A name somebody proved with a passphrase: a dot beside it.
    var proven = false
    /// Wearing a name somebody else in the room proved, without proving it
    /// (name.ts `impersonating`).
    var unverified = false
    var speaking = false
    /// Their microphone is off, or they have none.
    var muted = false
    /// Turned all the way down, or stopped at the server, by whoever is
    /// looking (hearing.ts `silenced`).
    var silenced = false
    var hand = false
    /// The hand drawn at the start of the name rather than in the top left,
    /// where the room's own name and signal would cover it.
    var handBesideName = false
    /// No name drawn, for a picture that names itself.
    var unlabelled = false
}

/// The frame every picture appears in (components/room/Tile.tsx): a camera,
/// a shared screen, a person with their camera off and a share that is not on
/// the stage all sit in the same box with the same marks, so the grid never
/// jumps as cameras go on and off.
///
/// A press puts it on the stage or takes it back off, and two in quick
/// succession fill the screen with it; the room view tells the two apart, so
/// that the second press of a double one counts for the picture the first one
/// moved, wherever that picture has gone by then (`RoomView.pressed`). The
/// keyboard presses it with Return or Space, with the focus ring the web
/// draws for `focus-visible`.
struct TileFrame<Content: View, Overlay: View>: View {
    let marks: TileMarks
    var compact = false
    /// On the stage, so a press sends it back to the grid.
    var selected = false
    /// The corner symbol that says a press makes this larger; off on the
    /// stage, where the stage's own controls take that corner.
    var expandable = true
    /// Square while the picture fills the screen, whose own edges are the
    /// picture's.
    var radius: CGFloat = Theme.tileRadius
    /// Laid over another picture, as the card in a call of two is: lifted
    /// off it by a shadow and a hairline edge, since both are the same grey
    /// with nothing on them and the card otherwise vanished into the other.
    var floating = false
    /// What shows around a picture of another shape: the tile's grey in a
    /// grid, black when the picture fills the screen, as a player's bars are.
    var ground: Color = Theme.surface
    /// What the marks keep clear of at the picture's edges: the rounded
    /// corners and the home indicator, when the picture fills a phone.
    var edge = EdgeInsets()
    let press: () -> Void
    /// Return and Space: a single press, never a double one.
    let select: () -> Void
    @ViewBuilder var content: Content
    /// Drawn over the picture, handed whether the pointer is on it.
    @ViewBuilder var overlay: (_ hovering: Bool) -> Overlay

    @State private var hovering = false
    @FocusState private var focused: Bool

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            RoundedRectangle(cornerRadius: radius).fill(ground)
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .clipShape(RoundedRectangle(cornerRadius: radius))
            if !marks.unlabelled {
                NameLabel(marks: marks, compact: compact)
                    .padding(compact ? 4 : 8)
                    .padding(edge)
                    .transition(Theme.arrive)
            }
            if marks.hand && !marks.handBesideName {
                HandMark(compact: compact)
                    .padding(compact ? 5 : 8)
                    .padding(edge)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .transition(.asymmetric(
                        insertion: .scale(scale: 0.5).combined(with: .opacity).animation(Theme.settle),
                        removal: .scale(scale: 0.8).combined(with: .opacity).animation(Theme.leave)))
            }
            if expandable {
                // Over the picture rather than beside it: anything taking room
                // would reflow the grid as the pointer crossed it (Tile.tsx).
                Image(systemName: selected ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                    .font(.system(size: compact ? 9.5 : 11, weight: .bold))
                    .foregroundStyle(Theme.foreground)
                    .frame(width: compact ? 20 : 26, height: compact ? 20 : 26)
                    .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(.black.opacity(0.5)))
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .padding(compact ? 5 : 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .opacity(hovering || StageDebug.hover ? 1 : 0)
                    .scaleEffect(hovering || StageDebug.hover ? 1 : 0.9, anchor: .topTrailing)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
            overlay(hovering || StageDebug.hover)
        }
        .overlay {
            RoundedRectangle(cornerRadius: radius)
                .strokeBorder(Color.white.opacity(0.14), lineWidth: 1)
                .opacity(floating ? 1 : 0)
                .allowsHitTesting(false)
        }
        .shadow(color: .black.opacity(floating ? 0.45 : 0), radius: 18, y: 8)
        .overlay {
            // The tally around a speaker, held through a pause (useSteady).
            RoundedRectangle(cornerRadius: radius)
                .strokeBorder(Theme.tally, lineWidth: 2)
                .opacity(marks.speaking ? 1 : 0)
                .allowsHitTesting(false)
        }
        .overlay {
            RoundedRectangle(cornerRadius: radius + 2)
                .strokeBorder(Theme.foreground.opacity(0.75), lineWidth: 2)
                .padding(-3)
                .opacity(focused ? 1 : 0)
                .allowsHitTesting(false)
        }
        .contentShape(RoundedRectangle(cornerRadius: radius))
        .onTapGesture(perform: press)
        .onHover { hovering = $0 }
        // Focusable as a button is, by the keyboard and not by a click: a
        // ring left behind by every click is the fault the web's
        // `focus-visible` exists to avoid.
        .focusable(interactions: .activate)
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(keys: [.return, .space]) { _ in
            select()
            return .handled
        }
        .animation(.easeOut(duration: 0.14), value: hovering)
        .animation(Theme.settle, value: marks)
        .animation(Theme.settle, value: focused)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(selected ? t("Show everybody") : t("Show {name} larger", ["name": marks.label]))
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction(.default, select)
    }
}

/// The name at the foot of a picture, and what is true of it: in the
/// person's colour while they speak, a dot for a proven name, a badge for one
/// worn by somebody else, and one mark for having no sound from here, where
/// the reader's own doing wins: a microphone somebody else turned off is
/// nothing to act on, a person talking into silence that you caused is
/// (Tile.tsx).
private struct NameLabel: View {
    let marks: TileMarks
    let compact: Bool

    var body: some View {
        HStack(spacing: compact ? 3 : 5) {
            if marks.hand && marks.handBesideName {
                HandMark(compact: true, small: true)
                    .padding(.leading, compact ? -3 : -4)
                    .transition(.scale.combined(with: .opacity))
            }
            Text(marks.label)
                .font(.system(size: compact ? 10.5 : 12.5, weight: .medium))
                .foregroundStyle(marks.speaking ? Theme.tally : Theme.foreground)
                .lineLimit(1)
                .truncationMode(.tail)
            if marks.proven {
                Circle()
                    .fill(Theme.muted)
                    .frame(width: 3.5, height: 3.5)
                    .padding(.horizontal, 1)
                    .help(t("Only this person can use this name"))
                    .accessibilityLabel(t("Only this person can use this name"))
                    .transition(.opacity)
            }
            if marks.unverified {
                Text(t("unverified"))
                    .font(.system(size: compact ? 8.5 : 9.5, weight: .semibold))
                    .foregroundStyle(Theme.dangerForeground)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1.5)
                    .background(Capsule().fill(Theme.danger))
                    .help(t("Someone else has proved this name"))
                    .accessibilityLabel(t("Someone else has proved this name"))
                    .transition(.scale.combined(with: .opacity))
            }
            if marks.silenced {
                Image(systemName: "speaker.slash.fill")
                    .font(.system(size: compact ? 9 : 10.5, weight: .semibold))
                    .foregroundStyle(Theme.danger)
                    .help(t("Muted by you"))
                    .accessibilityLabel(t("Muted by you"))
                    .transition(.scale.combined(with: .opacity))
            } else if marks.muted {
                Image(systemName: "mic.slash.fill")
                    .font(.system(size: compact ? 9 : 10.5, weight: .semibold))
                    .foregroundStyle(Theme.muted)
                    .accessibilityLabel(t("microphone off"))
                    .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.horizontal, compact ? 5 : 7)
        .padding(.vertical, compact ? 2 : 3.5)
        // Over the picture, a dimming rather than glass: one piece of glass
        // on every tile was worked out again from the moving video beneath it
        // on every frame, which is what Apple's guidance says to limit, and a
        // name on a picture is content. The dark fill is what the guidance
        // gives for legibility over rich content.
        .background(Capsule().fill(Color.black.opacity(0.5)))
        .overlay(Capsule().stroke(Color.white.opacity(0.1), lineWidth: 0.5))
        .animation(Theme.settle, value: marks)
    }
}

/// A raised hand, top left, away from the name and from the stage's own
/// controls (Reacting.tsx). Announced as a status when it goes up, because a
/// hand nobody sees is a hand that was not put up, and a reader has no
/// picture to glance at; it is not a button, and it was labelled with the
/// button's words before.
private struct HandMark: View {
    let compact: Bool
    /// Inside the name's pill.
    var small = false

    var body: some View {
        let side: CGFloat = small ? 18 : compact ? 22 : 28
        Text("\u{270B}")
            .font(.system(size: small ? 10 : compact ? 12 : 15))
            .frame(width: side, height: side)
            .background(Circle().fill(Theme.tally))
            .shadow(color: .black.opacity(small ? 0 : 0.3), radius: 6, y: 2)
            .accessibilityAddTraits(.updatesFrequently)
            .onAppear { AccessibilityNotification.Announcement("\u{270B}").post() }
    }
}

/// Holds a flag on for a moment after it goes off (hooks/useSteady.ts):
/// speaking arrives as a flag a pause between two sentences turns off, and
/// drawn straight it makes the tally flicker through a single remark. Rising
/// is immediate; only the fall waits.
struct Steady<Inner: View>: View {
    let value: Bool
    var hold: Duration = .milliseconds(700)
    @ViewBuilder var inner: (_ held: Bool) -> Inner
    @State private var held = false

    var body: some View {
        inner(held || value)
            .task(id: value) {
                if value {
                    held = true
                } else {
                    try? await Task.sleep(for: hold)
                    if !Task.isCancelled { held = false }
                }
            }
    }
}

/// A picture that belongs to somebody else: their camera, or their screen on
/// the stage (components/room/SurfaceTile.tsx). A camera fills its tile,
/// because a face is fine cropped at the edges and wrong letterboxed; a
/// screen is shown whole, because cropping hides whatever was at the edge.
struct SurfacePicture: View {
    let surface: Surface
    let call: Call
    var compact = false

    var body: some View {
        ZStack {
            if let sid = surface.sid {
                VideoSurface(call: call, sid: sid, fill: surface.kind == .camera)
                    .transition(.opacity.animation(Theme.settle))
            } else {
                Avatar(name: surface.person.name, identity: surface.person.id)
                    .transition(.opacity.animation(Theme.settle))
            }
            // What they just reacted with, rising off every picture of theirs
            // as the web draws it on each (Room.tsx, Reacting.tsx).
            FloatingReactions(call: call, identity: surface.person.id, compact: compact)
        }
        .animation(Theme.settle, value: surface.sid)
    }
}

/// This participant's own camera, or their initials while it is off.
struct SelfPicture: View {
    @Bindable var model: CallModel
    var compact = false

    var body: some View {
        ZStack {
            if model.cameraOn {
                SelfView(session: model.camera.session, capture: model.camera).transition(.opacity.animation(Theme.settle))
            } else {
                Avatar(name: model.localName, identity: model.localIdentity)
                    .transition(.opacity.animation(Theme.settle))
            }
            // What this participant sent, over their own picture as everybody
            // else sees it there (the web draws reactions on every tile, its
            // own included).
            FloatingReactions(call: model.call, identity: model.localIdentity, compact: compact)
        }
        .animation(Theme.settle, value: model.cameraOn)
    }
}

/// Stands in for a screen share that is not on the stage
/// (components/room/ShareCard.tsx). A desktop scaled into a grid cell is
/// unreadable, so subscribing to it would spend the bandwidth and deliver
/// nothing: the card says who is sharing, and the whole card takes the press
/// that makes it big, rather than a button on a surface that already answers
/// one.
struct ShareCardFace: View {
    let name: String
    var compact = false

    var body: some View {
        ZStack {
            LinearGradient(colors: [Theme.surfaceHigh, Theme.surface], startPoint: .top, endPoint: .bottom)
            RadialGradient(colors: [Theme.tally.opacity(0.16), .clear], center: .center, startRadius: 0, endRadius: 160)
            VStack(spacing: compact ? 4 : 10) {
                Image(systemName: "rectangle.inset.filled.and.person.filled")
                    .font(.system(size: compact ? 15 : 22, weight: .medium))
                    .foregroundStyle(Theme.foreground.opacity(0.9))
                    .frame(width: compact ? 30 : 48, height: compact ? 30 : 48)
                    .background(Circle().fill(Color.white.opacity(0.08)))
                Text(t("{name} is sharing", ["name": name]))
                    .font(.system(size: compact ? 11 : 13.5, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(compact ? 1 : 2)
                    .multilineTextAlignment(.center)
                if !compact {
                    HStack(spacing: 5) {
                        Image(systemName: "play.fill").font(.system(size: 9, weight: .bold))
                        Text(t("Watch"))
                    }
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Theme.tallyForeground)
                    .padding(.horizontal, 12)
                    .frame(height: 26)
                    .background(Capsule().fill(Theme.tally))
                }
            }
            .padding(.horizontal, 12)
        }
    }
}

/// This participant's own shared screen. There is no preview of the capture
/// to draw, so it says so, with the way to stop: a picture of your own screen
/// inside your own screen is a hall of mirrors anyway, and what the sharer
/// needs from this place is to know it is going out.
struct MyShareFace: View {
    var compact = false
    var stop: (() -> Void)?

    var body: some View {
        ZStack {
            LinearGradient(colors: [Theme.surfaceHigh, Theme.surface], startPoint: .topLeading, endPoint: .bottomTrailing)
            RadialGradient(colors: [Theme.tally.opacity(0.22), .clear], center: .center, startRadius: 0, endRadius: 220)
            VStack(spacing: compact ? 4 : 12) {
                ZStack {
                    Circle().fill(Theme.tally.opacity(0.18)).frame(width: compact ? 34 : 64, height: compact ? 34 : 64)
                    Image(systemName: "rectangle.on.rectangle")
                        .font(.system(size: compact ? 14 : 24, weight: .semibold))
                        .foregroundStyle(Theme.tally)
                }
                Text(t("You are sharing your screen"))
                    .font(.system(size: compact ? 11 : 14, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(compact ? 1 : 2)
                    .multilineTextAlignment(.center)
                if let stop, !compact {
                    Button(action: stop) {
                        Text(t("Stop sharing"))
                    }
                    .buttonStyle(.glassCapsule(30))
                }
            }
            .padding(.horizontal, 12)
        }
    }
}

/// What can be done with whatever is on the stage (StageControls.tsx): the
/// same person's other picture, when they have one, and filling the screen.
/// Hidden until the pointer is on the stage, because a shared screen is being
/// read and anything left on top of it competes with what people came to
/// see. Its buttons take their own presses: on the web a press on one carried
/// on to the picture beneath, and "Their screen" pinned the other picture
/// only for the picture's own press to pin this one straight back.
struct StageControls: View {
    /// The other picture's kind, when there is one.
    let other: Surface.Kind?
    let filled: Bool
    let switchTo: () -> Void
    let fill: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            if let other {
                Button(action: switchTo) {
                    HStack(spacing: 6) {
                        Image(systemName: other == .screen ? "rectangle.on.rectangle" : "person.fill")
                            .font(.system(size: 11.5, weight: .semibold))
                        Text(other == .screen ? t("Their screen") : t("Their camera"))
                    }
                }
                .buttonStyle(.overPicture(32, words: true))
                .transition(Theme.arrive)
            }
            Button(action: fill) {
                Image(systemName: filled ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                    .font(.system(size: 12.5, weight: .bold))
            }
            .buttonStyle(.overPicture(32))
            .accessibilityLabel(filled ? t("Leave fullscreen") : t("Fill the screen"))
            .help(filled ? t("Leave fullscreen") : t("Fill the screen"))
        }
        .animation(Theme.settle, value: other)
    }
}

/// A person with no picture: their initials on a colour of their own, steady
/// for the same identity, as the web's marble avatar is steady for its seed.
struct Avatar: View {
    let name: String
    let identity: String

    var body: some View {
        GeometryReader { geometry in
            let side = min(geometry.size.width, geometry.size.height) * 0.36
            ZStack {
                Circle().fill(color)
                Text(initials)
                    .font(.system(size: side * 0.38, weight: .semibold))
                    .foregroundStyle(Theme.foreground)
            }
            .frame(width: side, height: side)
            .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
        }
    }

    private var initials: String {
        let words = name.split(separator: " ")
        let letters = words.prefix(2).compactMap(\.first)
        return letters.isEmpty ? "?" : String(letters).uppercased()
    }

    /// Warm hues only, close to the palette in web/src/live/avatar.ts.
    private var color: Color {
        let palette: [Color] = [
            Color(red: 0.55, green: 0.33, blue: 0.24), Color(red: 0.42, green: 0.36, blue: 0.27),
            Color(red: 0.33, green: 0.38, blue: 0.33), Color(red: 0.48, green: 0.30, blue: 0.33),
            Color(red: 0.36, green: 0.33, blue: 0.40), Color(red: 0.50, green: 0.42, blue: 0.25),
        ]
        var hash: UInt64 = 1469598103934665603
        for byte in identity.utf8 { hash = (hash ^ UInt64(byte)) &* 1099511628211 }
        return palette[Int(hash % UInt64(palette.count))]
    }
}

/// This participant's own camera, drawn straight from the capture session,
/// or, while the background is blurred, from the frames as they are sent: the
/// session's own preview shows the room the call no longer sees.
struct SelfView: View {
    let session: AVCaptureSession
    var capture: CameraCapture?
    @State private var blurred = false

    var body: some View {
        Group {
            if let capture, blurred {
                BlurredPreview(capture: capture).transition(.opacity)
            } else {
                PreviewRepresentable(session: session).transition(.opacity)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: Theme.tileRadius))
        .animation(Theme.settle, value: blurred)
        .onAppear { blurred = capture?.blurring ?? false }
        .onReceive(NotificationCenter.default.publisher(for: CameraCapture.blurChanged)) { note in
            guard let capture, note.object as? CameraCapture === capture else { return }
            blurred = capture.blurring
        }
    }
}

/// The blurred frames, drawn as a remote picture is (VideoRenderer), filling
/// the tile and mirrored as the session's preview mirrors a camera that faces
/// its owner. The mirroring is the view's: the renderer sets its own layer's
/// transform with every frame.
private struct BlurredPreview: View {
    let capture: CameraCapture

    var body: some View {
        BlurredRepresentable(capture: capture)
    }
}

#if os(macOS)
private struct BlurredRepresentable: NSViewRepresentable {
    let capture: CameraCapture
    func makeNSView(context: Context) -> BlurredView { BlurredView(capture: capture) }
    func updateNSView(_ view: BlurredView, context: Context) { view.mirror() }
    static func dismantleNSView(_ view: BlurredView, coordinator: ()) { view.stop() }
}

final class BlurredView: NSView {
    private let capture: CameraCapture
    private let renderer = VideoRenderer()
    private var token: UUID?
    private let holder = CALayer()

    init(capture: CameraCapture) {
        self.capture = capture
        super.init(frame: .zero)
        wantsLayer = true
        layer = CALayer()
        renderer.layer.videoGravity = .resizeAspectFill
        holder.addSublayer(renderer.layer)
        layer?.addSublayer(holder)
        let renderer = self.renderer
        token = capture.showFrames { renderer.show($0, rotation: 0) }
        mirror()
    }

    required init?(coder: NSCoder) { fatalError("not from a nib") }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        holder.frame = bounds
        renderer.layer.frame = holder.bounds
        CATransaction.commit()
        mirror()
    }

    func mirror() {
        let faces = capture.device.map(CameraCapture.facesUser) ?? true
        holder.setAffineTransform(faces ? CGAffineTransform(scaleX: -1, y: 1) : .identity)
    }

    func stop() {
        if let token { capture.stopShowing(token) }
        token = nil
    }
}
#else
private struct BlurredRepresentable: UIViewRepresentable {
    let capture: CameraCapture
    func makeUIView(context: Context) -> BlurredView { BlurredView(capture: capture) }
    func updateUIView(_ view: BlurredView, context: Context) { view.mirror() }
    static func dismantleUIView(_ view: BlurredView, coordinator: ()) { view.stop() }
}

final class BlurredView: UIView {
    private let capture: CameraCapture
    private let renderer = VideoRenderer()
    private var token: UUID?

    init(capture: CameraCapture) {
        self.capture = capture
        super.init(frame: .zero)
        renderer.layer.videoGravity = .resizeAspectFill
        layer.addSublayer(renderer.layer)
        let renderer = self.renderer
        token = capture.showFrames { renderer.show($0, rotation: 0) }
        mirror()
    }

    required init?(coder: NSCoder) { fatalError("not from a storyboard") }

    override func layoutSubviews() {
        super.layoutSubviews()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        renderer.layer.frame = bounds
        CATransaction.commit()
    }

    func mirror() {
        let faces = capture.device.map(CameraCapture.facesUser) ?? true
        transform = faces ? CGAffineTransform(scaleX: -1, y: 1) : .identity
    }

    func stop() {
        if let token { capture.stopShowing(token) }
        token = nil
    }
}
#endif

/// Keeps a preview mirrored only while the camera feeding it faces its owner
/// (CameraCapture.facesUser, live/facing.ts, hooks/useMirrored.ts). It was
/// mirrored always, so Desk View, which films the desk from above, showed
/// every page and every word on it backwards. Read again whenever a camera
/// starts feeding the session, because a switch of camera keeps the same
/// session running and nothing about the session says the camera changed.
private final class MirrorFollower {
    private weak var layer: AVCaptureVideoPreviewLayer?
    private var observers: [NSObjectProtocol] = []

    init(layer: AVCaptureVideoPreviewLayer, session: AVCaptureSession) {
        self.layer = layer
        let center = NotificationCenter.default
        for name in [CameraCapture.changedCamera, AVCaptureSession.didStartRunningNotification] {
            observers.append(center.addObserver(forName: name, object: session, queue: .main) { [weak self] _ in
                self?.apply()
            })
        }
        apply()
    }

    deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
    }

    func apply() {
        guard let layer, let session = layer.session, let connection = layer.connection,
              connection.isVideoMirroringSupported else { return }
        connection.automaticallyAdjustsVideoMirroring = false
        connection.isVideoMirrored = CameraCapture.mirrors(session)
    }
}

#if os(macOS)
private struct PreviewRepresentable: NSViewRepresentable {
    let session: AVCaptureSession
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        let layer = AVCaptureVideoPreviewLayer(session: session)
        layer.videoGravity = .resizeAspectFill
        // Mirrored where the camera faces its owner, as every camera preview
        // is: a person expects to see what a mirror shows them. What is sent
        // is not mirrored.
        context.coordinator.follower = MirrorFollower(layer: layer, session: session)
        view.layer = layer
        view.wantsLayer = true
        return view
    }
    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.follower?.apply()
    }
    func makeCoordinator() -> Coordinator { Coordinator() }
    final class Coordinator {
        var follower: MirrorFollower?
    }
}
#else
private struct PreviewRepresentable: UIViewRepresentable {
    let session: AVCaptureSession
    func makeUIView(context: Context) -> PreviewView {
        let view = PreviewView()
        view.preview.session = session
        view.preview.videoGravity = .resizeAspectFill
        context.coordinator.follower = MirrorFollower(layer: view.preview, session: session)
        return view
    }
    func updateUIView(_ view: PreviewView, context: Context) {
        context.coordinator.follower?.apply()
    }
    func makeCoordinator() -> Coordinator { Coordinator() }
    final class Coordinator {
        var follower: MirrorFollower?
    }

    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var preview: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
    }
}
#endif
