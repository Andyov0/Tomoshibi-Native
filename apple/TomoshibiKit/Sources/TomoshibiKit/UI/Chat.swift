import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#endif

/// Everything said this call (components/room/ChatPanel.tsx): kept only for
/// the call, which the panel says while nothing has been said yet.
///
/// A run of messages from one person names them once, with the time it
/// started; addresses in a message are links; the box takes several lines and
/// sends on Return (a new line is Shift-Return), up to the web's two thousand
/// characters. While the call is reconnecting nothing can leave, and the box
/// says so rather than taking words it cannot send; a message that did not
/// leave is put back in the box, with a line saying it was not sent.
struct ChatPanel: View {
    @Bindable var model: CallModel
    let close: () -> Void
    @State private var logHeight: CGFloat = 0

    /// The tallest the conversation grows before it scrolls: the web's panel
    /// stops at twenty-six rems with its heading and its box.
    private static let tallest: CGFloat = 330

    var body: some View {
        VStack(spacing: 0) {
            PanelHeading(title: t("Messages"), symbol: "bubble.left.and.bubble.right.fill",
                         closeLabel: t("Close messages"), close: close)
            PanelRule()
            if model.messages.isEmpty {
                Empty()
                    .transition(Theme.arrive)
            } else {
                log
                    .transition(.opacity.animation(Theme.settle))
            }
            Composer(model: model)
                .padding(.horizontal, 10)
                .padding(.top, 4)
                .padding(.bottom, 10)
        }
        .glassShell(cornerRadius: 22)
        .animation(Theme.settle, value: model.messages.isEmpty)
    }

    private var log: some View {
        let lines = model.messages
        let speakers = lines.map(\.identity)
        return ScrollViewReader { reader in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(lines.enumerated()), id: \.element.id) { index, line in
                        let run = ChatRules.continues(speakers, at: index)
                        Said(line: line, run: run)
                            .padding(.top, index == 0 ? 0 : run ? 3 : 12)
                            .id(line.id)
                            .transition(.asymmetric(
                                insertion: .opacity.combined(with: .offset(y: 8)).animation(Theme.settle),
                                removal: .opacity.animation(Theme.leave)
                            ))
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { logHeight = $0 }
            }
            .scrollIndicators(.automatic)
            // The newest is where a conversation is read from.
            .defaultScrollAnchor(.bottom)
            .frame(height: min(max(logHeight, 1), Self.tallest))
            .onChange(of: lines.last?.id) { _, last in
                guard let last else { return }
                withAnimation(Theme.settle) { reader.scrollTo(last, anchor: .bottom) }
            }
            .onAppear {
                if let last = lines.last?.id { reader.scrollTo(last, anchor: .bottom) }
            }
        }
    }
}

/// One message, and above the first of a run, who said it and when.
private struct Said: View {
    let line: ChatLine
    let run: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if !run {
                HStack(alignment: .center, spacing: 8) {
                    Monogram(name: line.name, identity: line.identity, size: 20)
                    // Through the same phrase the tile uses.
                    Text(line.mine ? t("{name} (you)", ["name": line.name]) : line.name)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(Theme.foreground.opacity(0.92))
                        .lineLimit(1)
                    Text(Self.clock(line.at))
                        .font(.system(size: 10.5).monospacedDigit())
                        .foregroundStyle(Theme.muted)
                }
            }
            // Somebody's own words, so they are theirs to take away.
            LinkedText(text: line.text, size: 13)
                .foregroundStyle(Theme.foreground)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, 28)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    /// Wall-clock time to the minute, in the language in use rather than
    /// the system's, with no opinion about the twelve-hour clock: whether a
    /// time reads 14:30 or 2:30 PM is the language's habit (ChatPanel.tsx
    /// `clock`, `hour: "2-digit", minute: "2-digit"`).
    static func clock(_ at: Date) -> String {
        at.formatted(Date.FormatStyle(locale: Localization.shared.language.locale)
            .hour(.twoDigits(amPM: .abbreviated)).minute(.twoDigits))
    }
}

/// Nothing has been said yet. Carries the one fact somebody needs before they
/// type: this is not written down anywhere (ChatPanel.tsx `Empty`).
///
/// The symbol sits on a flat plate, the same faint white as the share card's
/// (ShareCardFace). It sat in a glow of the person's colour, which is kept
/// for what is saying something, and an empty list says nothing; the glow
/// went with the other gradients (2026-10-20).
private struct Empty: View {
    var body: some View {
        VStack(spacing: 12) {
            ZStack {
                Circle()
                    .fill(Color.white.opacity(0.08))
                    .frame(width: 56, height: 56)
                Image(systemName: "bubble.left.and.text.bubble.right")
                    .font(.system(size: 26, weight: .light))
                    .foregroundStyle(Theme.foreground.opacity(0.5))
            }
            Text(t("Messages disappear when the call ends."))
                .font(.system(size: 12))
                .foregroundStyle(Theme.muted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 200)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 26)
        .accessibilityElement(children: .combine)
    }
}

/// The box: as many lines as a message needs, up to five, then it scrolls.
private struct Composer: View {
    @Bindable var model: CallModel
    @State private var focused = false
    @State private var height: CGFloat = 24
    #if os(iOS)
    @FocusState private var focus: Bool
    #endif

    private var offline: Bool { model.state != .connected }
    private var sendable: Bool {
        !offline && !model.sending && !model.chatDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .bottom, spacing: 8) {
                field
                Button(action: send) {
                    ZStack {
                        if model.sending {
                            MediaSpinner(size: 14, tint: Theme.tallyForeground)
                                .transition(.opacity)
                        } else {
                            Image(systemName: "arrow.up")
                                .font(.system(size: 12.5, weight: .bold))
                                .foregroundStyle(sendable ? Theme.tallyForeground : Theme.muted)
                                .transition(.opacity)
                        }
                    }
                    .frame(width: 28, height: 28)
                    .background(Circle().fill(sendable || model.sending ? Theme.tally : Color.white.opacity(0.08)))
                    .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .disabled(!sendable)
                .accessibilityLabel(t("Send"))
                .help(t("Send"))
                .padding(.bottom, 1)
            }
            .padding(.leading, 12)
            .padding(.trailing, 5)
            .padding(.vertical, 5)
            .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.22)))
            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(focused && !offline ? Theme.tally.opacity(0.55) : Color.white.opacity(0.09), lineWidth: 1))
            .opacity(offline ? 0.6 : 1)

            if model.chatFailed || nearLimit {
                HStack {
                    if model.chatFailed {
                        Label(t("That could not be sent."), systemImage: "exclamationmark.circle.fill")
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.danger)
                            .transition(Theme.arrive)
                    }
                    Spacer(minLength: 0)
                    if nearLimit {
                        // How much is left, once there is little: the box
                        // otherwise stops taking characters without a word.
                        Text(verbatim: "\(model.chatDraft.utf16.count)/\(ChatRules.longest)")
                            .font(.system(size: 10.5).monospacedDigit())
                            .foregroundStyle(model.chatDraft.utf16.count >= ChatRules.longest ? Theme.tally : Theme.muted)
                            .transition(.opacity)
                    }
                }
                .padding(.horizontal, 8)
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: model.chatFailed)
        .animation(Theme.settle, value: nearLimit)
        .animation(Theme.settle, value: sendable)
        .animation(Theme.settle, value: model.sending)
        .animation(Theme.settle, value: focused)
        .animation(Theme.settle, value: height)
    }

    private var nearLimit: Bool { model.chatDraft.utf16.count > ChatRules.longest - 200 }

    private var placeholder: String { offline ? t("Reconnecting…") : t("Say something") }

    @ViewBuilder
    private var field: some View {
        #if os(macOS)
        ComposerField(text: $model.chatDraft, height: $height, focused: $focused, enabled: !offline, submit: send)
            .frame(height: height)
            .overlay(alignment: .topLeading) {
                if model.chatDraft.isEmpty {
                    Text(placeholder)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.muted)
                        .padding(.top, 4)
                        .allowsHitTesting(false)
                        .transition(.opacity.animation(.easeOut(duration: 0.12)))
                }
            }
            .accessibilityLabel(t("Say something"))
        #else
        TextField("", text: Binding(get: { model.chatDraft }, set: { model.chatDraft = ChatRules.clamp($0) }),
                  prompt: Text(placeholder).foregroundStyle(Theme.muted), axis: .vertical)
            .lineLimit(1...5)
            .font(.system(size: 15))
            .foregroundStyle(Theme.foreground)
            .tint(Theme.tally)
            .textFieldStyle(.plain)
            .disabled(offline)
            .focused($focus)
            .padding(.vertical, 4)
            // A hardware keyboard sends on Return, as on the Mac; the screen's
            // own keyboard makes a new line, and the arrow sends.
            .onKeyPress(.return, phases: .down) { press in
                if press.modifiers.contains(.shift) { return .ignored }
                send()
                return .handled
            }
            .onChange(of: focus) { _, now in focused = now }
            .task {
                // After the panel's own arrival, or the keyboard rises through it.
                try? await Task.sleep(for: .milliseconds(350))
                focus = true
            }
            .accessibilityLabel(t("Say something"))
        #endif
    }

    private func send() {
        guard sendable else { return }
        Task { await model.sendDraft() }
    }
}

/// Text with its addresses made into links (Linked.tsx): in the signal
/// colour, underlined faintly, opened by the system in the browser, because
/// the call keeps running here.
struct LinkedText: View {
    let text: String
    var size: CGFloat = 13

    var body: some View {
        Text(Self.attributed(text))
            .font(.system(size: size))
            .tint(Theme.tally)
            #if DEBUG
            .environment(\.openURL, OpenURLAction { url in Self.opened(url) })
            #endif
    }

    #if DEBUG
    /// TOMOSHIBI_DEBUG_LINKS=<file>, in a debug build: a link pressed is
    /// written to <file> instead of being opened, so that a script can press
    /// one without a browser opening on the machine it runs on.
    private static func opened(_ url: URL) -> OpenURLAction.Result {
        guard let path = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_LINKS"], !path.isEmpty else { return .systemAction }
        let data = Data((url.absoluteString + "\n").utf8)
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
        return .handled
    }
    #endif

    @MainActor
    static func attributed(_ text: String) -> AttributedString {
        var out = AttributedString()
        for piece in Linked.pieces(text) {
            switch piece {
            case let .text(words):
                out.append(AttributedString(words))
            case let .link(href):
                var link = AttributedString(href)
                if let url = URL(string: href) {
                    link.link = url
                    link.foregroundColor = Theme.tally
                    link.underlineStyle = Text.LineStyle(pattern: .solid, color: Theme.tally.opacity(0.45))
                }
                out.append(link)
            }
        }
        return out
    }
}

#if os(macOS)
/// The box itself on the Mac: a text view rather than SwiftUI's field, which
/// on the Mac takes Return as the end of editing and gives no way to tell a
/// plain Return from a shifted one. Return sends; Shift-Return and
/// Option-Return make a new line; Return while an input method is composing
/// (pinyin, kana) finishes the word as it always does and sends nothing.
private struct ComposerField: NSViewRepresentable {
    @Binding var text: String
    @Binding var height: CGFloat
    @Binding var focused: Bool
    var enabled: Bool
    let submit: () -> Void

    static let font = NSFont.systemFont(ofSize: 13)
    static let lines: CGFloat = 5

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.hasHorizontalScroller = false

        let view = BoxView()
        view.coordinator = context.coordinator
        view.delegate = context.coordinator
        view.drawsBackground = false
        view.isRichText = false
        view.importsGraphics = false
        view.allowsUndo = true
        view.font = Self.font
        view.textColor = NSColor(Theme.foreground)
        view.insertionPointColor = NSColor(Theme.tally)
        view.textContainerInset = NSSize(width: 0, height: 4)
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = true
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.autoresizingMask = [.width]
        // What is typed is sent as typed: an address with its quotes turned
        // curly, or two hyphens turned into a dash, is not the one copied.
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.string = text
        scroll.documentView = view
        context.coordinator.view = view
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let view = context.coordinator.view else { return }
        if view.string != text, !view.hasMarkedText() {
            view.string = text
            view.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
        }
        view.isEditable = enabled
        view.textColor = NSColor(enabled ? Theme.foreground : Theme.muted)
        context.coordinator.measure()
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerField
        weak var view: BoxView?

        init(_ parent: ComposerField) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let view else { return }
            if parent.text != view.string { parent.text = view.string }
            measure()
        }

        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            if textView.hasMarkedText() { return false }
            if NSApp.currentEvent?.modifierFlags.contains(.shift) == true {
                textView.insertNewlineIgnoringFieldEditor(nil)
                return true
            }
            parent.submit()
            return true
        }

        /// The web's `maxLength`: a paste that would pass two thousand
        /// characters goes in cut to fit, as a browser's does.
        func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString: String?) -> Bool {
            guard let replacement = replacementString else { return true }
            let length = (textView.string as NSString).length
            let after = length - range.length + (replacement as NSString).length
            guard after > ChatRules.longest else { return true }
            let room = ChatRules.longest - (length - range.length)
            if room > 0 {
                let fitted = ChatRules.clamp(replacement, to: room)
                if !fitted.isEmpty { textView.insertText(fitted, replacementRange: range) }
            }
            NSSound.beep()
            return false
        }

        /// One line to five, then it scrolls.
        func measure() {
            guard let view, let manager = view.layoutManager, let container = view.textContainer else { return }
            manager.ensureLayout(for: container)
            let line = manager.defaultLineHeight(for: ComposerField.font)
            let used = max(manager.usedRect(for: container).height, line)
            let inset = view.textContainerInset.height * 2
            let wanted = min(used, line * ComposerField.lines) + inset
            let rounded = wanted.rounded(.up)
            if abs(rounded - parent.height) > 0.5 {
                DispatchQueue.main.async { [parent] in parent.height = rounded }
            }
        }

        func focusChanged(_ now: Bool) {
            DispatchQueue.main.async { [parent] in
                if parent.focused != now { parent.focused = now }
            }
        }
    }

    /// Takes the keyboard the moment it is on screen: the panel was opened in
    /// order to type (ChatPanel.tsx focuses its box on mount).
    final class BoxView: NSTextView {
        weak var coordinator: Coordinator?
        private var focusedOnce = false

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window, !focusedOnce else { return }
            focusedOnce = true
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                window.makeFirstResponder(self)
            }
        }

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became { coordinator?.focusChanged(true) }
            return became
        }

        override func resignFirstResponder() -> Bool {
            let resigned = super.resignFirstResponder()
            if resigned { coordinator?.focusChanged(false) }
            return resigned
        }
    }
}
#endif

#if DEBUG
extension CallModel {
    /// TOMOSHIBI_DEBUG_CHAT=<file>, in a debug build: the messages box driven
    /// from a script, because the box cannot be typed into from outside on
    /// the machine these were checked on. Each line appended to <file>:
    ///
    ///     type <words>       put <words> in the box ("\n" is a new line)
    ///     send               press Return
    ///     fail on|off        make sends fail, to see the words come back
    ///     state              append the box and the messages to <file>.out
    func driveChatForDebug() {
        guard let path = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_CHAT"], !path.isEmpty else { return }
        Task { [weak self] in
            var offset = 0
            while let self, !Task.isCancelled, self.endedReason == nil {
                if let data = FileManager.default.contents(atPath: path), data.count > offset {
                    let fresh = String(decoding: data[offset...], as: UTF8.self)
                    if let end = fresh.lastIndex(of: "\n") {
                        offset += fresh[...end].utf8.count
                        for line in fresh[..<end].split(separator: "\n") {
                            await self.performChatLine(String(line), out: path + ".out")
                        }
                    }
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    private func performChatLine(_ line: String, out: String) async {
        let verb = line.prefix { $0 != " " }
        let rest = line.dropFirst(verb.count).drop { $0 == " " }
        var said = "done"
        switch verb {
        case "type": chatDraft = ChatRules.clamp(rest.replacingOccurrences(of: "\\n", with: "\n"))
        case "send": await sendDraft()
        case "fail": rehearseFailedSends = rest == "on"
        case "state":
            said = "draft=\(chatDraft.debugDescription) failed=\(chatFailed) sending=\(sending) "
                + "messages=\(messages.map { "\($0.mine ? "me" : $0.name): \($0.text)" })"
        default: said = "unknown: \(verb)"
        }
        let data = Data("> \(line)\n\(said)\n".utf8)
        if let handle = FileHandle(forWritingAtPath: out) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: out, contents: data)
        }
    }
}
#endif
