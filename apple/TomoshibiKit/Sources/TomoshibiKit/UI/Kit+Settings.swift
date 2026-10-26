import SwiftUI
import TomoshibiBasics
import TomoshibiCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

// More of the app's own controls, for the settings and the screens in front of
// a call: a popover that floats above a scrolling page, a choice that opens
// one, fields that light and reveal, the person's picture, a calendar. Drawn
// for the same reason as Kit.swift's (the owner's rule of 2026-10-17: no stock
// Form, Menu or Picker), and from the same glass, curves and colour.

// MARK: - A popover above the page

/// One open popover, as its anchor reports it to the page's layer.
struct FloatingPopoverItem: Identifiable {
    let id: String
    let anchor: Anchor<CGRect>
    let edge: VerticalEdge
    let alignment: HorizontalAlignment
    let width: CGFloat
    let panel: AnyView
}

struct FloatingPopoverKey: PreferenceKey {
    static let defaultValue: [FloatingPopoverItem] = []
    static func reduce(value: inout [FloatingPopoverItem], nextValue: () -> [FloatingPopoverItem]) {
        value.append(contentsOf: nextValue())
    }
}

/// Marks a control as the anchor of a popover. The panel is drawn by the
/// nearest `floatingPopoverLayer` above it rather than beside the control.
///
/// Kit's `glassPopover` hangs the panel off the control itself, which is right
/// over a call and wrong on a page that scrolls: a scroll view clips what it
/// holds, so a choice near the foot of a settings card opened into nothing,
/// and a later card drew over an earlier card's panel. Drawn at the page's
/// root it is above everything and can turn to open upwards where there is no
/// room below.
private struct FloatingAnchor<Panel: View>: ViewModifier {
    let id: String
    let popovers: Popovers
    let edge: VerticalEdge
    let alignment: HorizontalAlignment
    let width: CGFloat
    @ViewBuilder let panel: () -> Panel

    func body(content: Content) -> some View {
        // Read here, in a body, so the anchor redraws when it opens or closes.
        let open = popovers.open == id
        content.anchorPreference(key: FloatingPopoverKey.self, value: .bounds) { anchor in
            open ? [FloatingPopoverItem(id: id, anchor: anchor, edge: edge, alignment: alignment, width: width,
                                        panel: AnyView(panel()))] : []
        }
    }
}

extension View {
    func floatingPopover<Panel: View>(_ id: String, in popovers: Popovers, edge: VerticalEdge = .bottom,
                                      alignment: HorizontalAlignment = .trailing, width: CGFloat = 260,
                                      @ViewBuilder panel: @escaping () -> Panel) -> some View {
        modifier(FloatingAnchor(id: id, popovers: popovers, edge: edge, alignment: alignment, width: width, panel: panel))
    }

    /// Where a page's floating popovers are drawn, over everything on it, with
    /// the catcher under them that closes the open one on a click elsewhere.
    func floatingPopoverLayer(_ popovers: Popovers) -> some View {
        modifier(FloatingPopoverLayer(popovers: popovers))
    }
}

private struct FloatingPopoverLayer: ViewModifier {
    let popovers: Popovers

    func body(content: Content) -> some View {
        content
            .overlayPreferenceValue(FloatingPopoverKey.self) { items in
                GeometryReader { proxy in
                    ZStack(alignment: .topLeading) {
                        // Shown whenever one is open anywhere, not only here:
                        // the Mac's lobby has a layer per column, and a click in
                        // the other column closes the popover as well.
                        if popovers.open != nil {
                            Color.black.opacity(0.001)
                                .contentShape(Rectangle())
                                .onTapGesture { popovers.close() }
                        }
                        ForEach(items) { item in
                            FloatingPanel(item: item, rect: proxy[item.anchor], bounds: proxy.size)
                        }
                    }
                    .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
                }
            }
            // Drawn here and nowhere else: a page inside another page's layer
            // (the schedule inside the Mac's lobby) would otherwise have its
            // popovers drawn twice, once by each.
            .transformPreference(FloatingPopoverKey.self) { $0 = [] }
            .environment(\.popovers, popovers)
            #if os(macOS)
            .onExitCommand { popovers.close() }
            #else
            .onKeyPress(.escape) {
                guard popovers.open != nil else { return .ignored }
                popovers.close()
                return .handled
            }
            #endif
    }
}

/// The panel, placed beside its anchor and kept inside the page.
private struct FloatingPanel: View {
    let item: FloatingPopoverItem
    let rect: CGRect
    let bounds: CGSize
    private let gap: CGFloat = 8
    private let margin: CGFloat = 10

    var body: some View {
        let width = min(item.width, bounds.width - margin * 2)
        let x: CGFloat = {
            let wanted = switch item.alignment {
            case .leading: rect.minX
            case .center: rect.midX - width / 2
            default: rect.maxX - width
            }
            return min(max(margin, wanted), max(margin, bounds.width - width - margin))
        }()
        let downward = opensDownward
        item.panel
            .frame(width: width)
            .glassPane(cornerRadius: 18)
            .fixedSize(horizontal: false, vertical: true)
            .alignmentGuide(.leading) { _ in -x }
            .alignmentGuide(.top) { d in
                let below = rect.maxY + gap
                let above = rect.minY - gap - d.height
                // The preferred side, unless it would run off the page and the
                // other side has the room.
                let fitsBelow = below + d.height <= bounds.height - margin
                let fitsAbove = above >= margin
                let y: CGFloat = if item.edge == .bottom {
                    fitsBelow || !fitsAbove ? below : above
                } else {
                    fitsAbove || !fitsBelow ? above : below
                }
                return -min(max(margin, y), max(margin, bounds.height - d.height - margin))
            }
            .transition(.asymmetric(
                insertion: .opacity.combined(with: .scale(scale: 0.94, anchor: downward ? .top : .bottom)).animation(Theme.settle),
                removal: .opacity.combined(with: .scale(scale: 0.97, anchor: downward ? .top : .bottom)).animation(Theme.leave)
            ))
    }

    /// Which way the scale of its arrival grows from. Measured roughly here,
    /// before the panel's height is known; the placement itself is exact.
    private var opensDownward: Bool {
        item.edge == .bottom ? rect.maxY < bounds.height * 0.62 : rect.minY < bounds.height * 0.3
    }
}

extension Popovers {
    /// TOMOSHIBI_DEBUG_POPOVER=<id>, in a debug build, opens that popover a
    /// moment after the screen arrives, for a test to see it without a
    /// pointer (the room's bar reads the same switch).
    func openFromDebug() {
        #if DEBUG
        guard let id = ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_POPOVER"] else { return }
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(1.2))
            show(id)
        }
        #endif
    }
}

// MARK: - Choosing one of several

/// One choice of several, as a capsule saying the current one; pressed, a
/// popover of the others. What a pull-down menu was for.
struct GlassChoice<Value: Hashable>: View {
    struct Option {
        let value: Value
        let label: String
        var detail: String?
        var symbol: String?
        var disabled = false
    }

    let id: String
    let options: [Option]
    @Binding var selection: Value
    var symbol: String?
    var width: CGFloat = 260
    var height: CGFloat = 30
    var heading: String?
    @Environment(\.popovers) private var popovers

    var body: some View {
        if let popovers {
            Button { popovers.toggle(id) } label: {
                HStack(spacing: 6) {
                    if let symbol { Image(systemName: symbol).font(.system(size: 11.5, weight: .semibold)) }
                    Text(current?.label ?? "—").lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(Theme.muted)
                }
                .font(.system(size: 12.5, weight: .medium))
            }
            .buttonStyle(.glassCapsule(height))
            .fixedSize()
            .floatingPopover(id, in: popovers, width: width) {
                MenuCard {
                    if let heading { MenuHeading(title: heading) }
                    ForEach(options.indices, id: \.self) { index in
                        let option = options[index]
                        MenuItem(title: option.label, symbol: option.symbol, detail: option.detail,
                                 checked: option.value == selection) {
                            withAnimation(Theme.settle) { selection = option.value }
                            popovers.close()
                        }
                        .disabled(option.disabled)
                    }
                }
            }
        }
    }

    private var current: Option? { options.first { $0.value == selection } }
}

// MARK: - Fields

/// A field sunk into the app's glass: a symbol that lights while there is
/// something in it, and for a secret an eye that shows it once there is
/// something to show (Identity.tsx: "an eye beside an empty field is a
/// control offering to reveal nothing").
struct GlassField: View {
    let prompt: String
    @Binding var text: String
    var symbol: String?
    /// The symbol wears the person's colour once the field holds something:
    /// the web lights the key beside a passphrase because a name nobody can
    /// take is the one thing on that screen worth lighting.
    var lights = false
    var secret = false
    var mono = false
    var readOnly = false
    var height: CGFloat = 44
    var onSubmit: () -> Void = {}
    @State private var shown = false

    var body: some View {
        HStack(spacing: 10) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(lights && !text.isEmpty ? Theme.tallyInk : Theme.muted)
                    .frame(width: 18)
                    .animation(Theme.settle, value: text.isEmpty)
            }
            Group {
                if readOnly {
                    Text(text).foregroundStyle(Theme.muted).frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                } else if secret && !shown {
                    SecureField("", text: $text, prompt: Text(prompt).foregroundStyle(Theme.muted))
                } else {
                    TextField("", text: $text, prompt: Text(prompt).foregroundStyle(Theme.muted))
                }
            }
            .textFieldStyle(.plain)
            .font(mono ? .system(size: 14, design: .monospaced) : .system(size: 14))
            .foregroundStyle(Theme.foreground)
            .onSubmit(onSubmit)
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            #endif
            if secret && !text.isEmpty {
                Button { shown.toggle() } label: {
                    Image(systemName: shown ? "eye.slash" : "eye")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.muted)
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(shown ? t("Hide passphrase") : t("Show passphrase"))
                .transition(Theme.arrive)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: height)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.sunk(0.24)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Theme.ink.opacity(0.10)))
        .animation(Theme.settle, value: text.isEmpty)
    }
}

// MARK: - Sealing a call

/// Sealed.tsx: a switch that opens onto one field and one sentence. The word
/// and the room name make the key; everybody who types the same word hears
/// each other and the relay does not. The title says "This call is sealed"
/// once a word is typed, in the person's colour, which is the only promise on
/// the screen about who can read the call.
struct SealControl: View {
    @Binding var on: Bool
    @Binding var word: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: sealed ? "lock.fill" : "lock")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(sealed ? Theme.tallyInk : Theme.muted)
                    .frame(width: 18)
                    .contentTransition(.symbolEffect(.replace))
                Text(sealed ? t("This call is sealed") : t("Seal this call"))
                    .font(.system(size: 14, weight: sealed ? .semibold : .regular))
                    .foregroundStyle(sealed ? Theme.tallyInk : Theme.foreground)
                    .contentTransition(.opacity)
                Spacer(minLength: 8)
                Toggle("", isOn: $on).labelsHidden().toggleStyle(.glassSwitch)
                    .accessibilityLabel(t("Seal this call"))
            }
            if on {
                VStack(alignment: .leading, spacing: 8) {
                    GlassField(prompt: t("Sealing word"), text: $word, symbol: "key", lights: true, secret: true, height: 40)
                    Text(t("Everybody who types the same word can hear each other, and the server carrying the call cannot. Tell people the word some other way."))
                        .font(.system(size: 11.5))
                        .foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .transition(Theme.arrive)
            }
        }
        .animation(Theme.settle, value: on)
        .animation(Theme.settle, value: sealed)
        .onChange(of: on) { if !on { word = "" } }
    }

    private var sealed: Bool { on && !word.isEmpty }
}

// MARK: - Copying a link

/// Copies a link and says so for a moment, as the web's copy buttons do.
struct CopyLinkButton: View {
    let link: String
    var size: CGFloat = 32
    @State private var copied = false

    var body: some View {
        Button {
            CopiedText.put(link)
            copied = true
            Task {
                try? await Task.sleep(for: .seconds(1.6))
                copied = false
            }
        } label: {
            Image(systemName: copied ? "checkmark" : "doc.on.doc")
                .font(.system(size: size * 0.38, weight: .semibold))
                .foregroundStyle(copied ? Theme.tallyInk : Theme.foreground)
                .contentTransition(.symbolEffect(.replace))
        }
        .buttonStyle(.glassCircle(size))
        .accessibilityLabel(copied ? t("Link copied") : t("Copy link"))
        .help(copied ? t("Link copied") : t("Copy link"))
        .animation(Theme.settle, value: copied)
    }
}

enum CopiedText {
    static func put(_ text: String) {
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        #endif
    }
}

// MARK: - The person's picture

/// The account's picture where it has one, the initial in a circle where it
/// has none or the picture will not load (Lobby.tsx `Corner`, Account.tsx
/// `Face`).
struct AccountAvatar: View {
    let name: String
    let address: URL?
    var size: CGFloat = 32

    var body: some View {
        ZStack {
            Initial(name: name, size: size)
            if let address {
                AsyncImage(url: address, transaction: Transaction(animation: Theme.settle)) { phase in
                    if case let .success(image) = phase {
                        image.resizable().scaledToFill()
                            .transition(.opacity)
                    }
                }
                .frame(width: size, height: size)
                .clipShape(Circle())
            }
        }
        .frame(width: size, height: size)
        .overlay(Circle().stroke(Theme.ink.opacity(0.12), lineWidth: 1))
        .accessibilityHidden(true)
    }
}

extension AppModel {
    /// Where the signed-in account's picture is, resolved against the
    /// deployment: the server says `/api/avatar/<trip>`, a path on itself.
    /// Carries a counter that a new picture moves on, so the image the system
    /// already holds under that address is not shown again (Account.tsx
    /// appends the time for the same reason).
    var avatarAddress: URL? {
        guard let avatar = account?.avatar, !avatar.isEmpty else { return nil }
        if avatar.hasPrefix("http://") || avatar.hasPrefix("https://") { return URL(string: avatar) }
        guard let base = deployment?.baseUrl() else { return nil }
        let root = base.hasSuffix("/") ? String(base.dropLast()) : base
        let path = avatar.hasPrefix("/") ? avatar : "/" + avatar
        return URL(string: root + path + (pictureVersion > 0 ? "?\(pictureVersion)" : ""))
    }
}

// MARK: - Small labels

/// A capsule naming one thing an account has, or has not been given
/// (Account.tsx `Ability`): drawn dashed when it was withheld, so a missing
/// button in a call can be seen to have been a decision.
struct Chip: View {
    let title: String
    var symbol: String?
    var withheld = false
    var lit = false
    var mono = false

    var body: some View {
        HStack(spacing: 5) {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(lit ? Theme.tallyInk : withheld ? Theme.faint(0.7) : Theme.muted)
            }
            Text(title)
                .font(mono ? .system(size: 11.5, design: .monospaced) : .system(size: 12))
                .foregroundStyle(withheld ? Theme.muted : Theme.foreground)
                .lineLimit(1)
        }
        .padding(.horizontal, 9).padding(.vertical, 5)
        .background(Capsule().fill(withheld ? Color.clear : lit ? Theme.tally.opacity(0.14) : Theme.ink.opacity(0.08)))
        .overlay(Capsule().strokeBorder(withheld ? Theme.ink.opacity(0.18) : lit ? Theme.tally.opacity(0.4) : Theme.ink.opacity(0.08),
                                         style: StrokeStyle(lineWidth: 1, dash: withheld ? [3, 3] : [])))
    }
}

/// Things that wrap onto as many lines as they need.
struct FlowRow: Layout {
    var spacing: CGFloat = 6

    /// A thing wider than the line is given the line, never more: one too
    /// wide for a phone held upright pushed the whole page past the screen's
    /// edge.
    private func size(of subview: LayoutSubview, within width: CGFloat) -> CGSize {
        let ideal = subview.sizeThatFits(.unspecified)
        guard ideal.width > width else { return ideal }
        return subview.sizeThatFits(ProposedViewSize(width: width, height: nil))
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0, widest: CGFloat = 0
        for subview in subviews {
            let size = size(of: subview, within: width)
            if x > 0, x + size.width > width {
                y += line + spacing
                x = 0
                line = 0
            }
            x += size.width + spacing
            line = max(line, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: min(widest, width), height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, line: CGFloat = 0
        for subview in subviews {
            let size = size(of: subview, within: bounds.width)
            if x > bounds.minX, x + size.width > bounds.maxX {
                y += line + spacing
                x = bounds.minX
                line = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }
}

// MARK: - Days and times

/// A month of days to choose one from, drawn rather than the system's
/// calendar: the chosen day in the person's colour, today ringed, days that
/// cannot be chosen dimmed.
struct MonthCalendar: View {
    @Binding var day: Date
    var earliest: Date?
    var latest: Date?
    @State private var month: Date
    @State private var forward = true
    @Environment(\.locale) private var locale

    init(day: Binding<Date>, earliest: Date? = nil, latest: Date? = nil) {
        _day = day
        self.earliest = earliest
        self.latest = latest
        _month = State(initialValue: Calendar.current.dateInterval(of: .month, for: day.wrappedValue)?.start ?? day.wrappedValue)
    }

    private var calendar: Calendar {
        var calendar = Calendar.current
        calendar.locale = locale
        return calendar
    }

    var body: some View {
        VStack(spacing: 10) {
            HStack {
                Text(month.formatted(Date.FormatStyle(locale: locale).year().month(.wide)))
                    .font(.system(size: 14, weight: .semibold))
                    .contentTransition(.numericText(countsDown: !forward))
                Spacer()
                step("chevron.left", by: -1)
                step("chevron.right", by: 1)
            }
            let symbols = weekdaySymbols
            HStack(spacing: 0) {
                ForEach(symbols.indices, id: \.self) { index in
                    Text(symbols[index]).font(.system(size: 10.5, weight: .semibold)).foregroundStyle(Theme.muted)
                        .frame(maxWidth: .infinity)
                }
            }
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 2), count: 7), spacing: 4) {
                ForEach(cells, id: \.self) { cell in
                    if let cell { dayCell(cell) } else { Color.clear.frame(height: 30) }
                }
            }
            .id(month)
            .transition(.asymmetric(
                insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity).animation(Theme.settle),
                removal: .opacity.animation(Theme.leave)
            ))
        }
        .clipped()
    }

    private func step(_ symbol: String, by months: Int) -> some View {
        let target = calendar.date(byAdding: .month, value: months, to: month) ?? month
        let allowed = months < 0 ? earliest.map { target >= startOfMonth($0) } ?? true : latest.map { target <= $0 } ?? true
        return Button {
            forward = months > 0
            withAnimation(Theme.settle) { month = target }
        } label: {
            Image(systemName: symbol).font(.system(size: 11, weight: .bold))
        }
        .buttonStyle(.glassCircle(28))
        .disabled(!allowed)
    }

    private func startOfMonth(_ date: Date) -> Date {
        calendar.dateInterval(of: .month, for: date)?.start ?? date
    }

    private var weekdaySymbols: [String] {
        let symbols = calendar.veryShortStandaloneWeekdaySymbols
        let first = calendar.firstWeekday - 1
        return Array(symbols[first...] + symbols[..<first])
    }

    /// The month's days with blanks before the first, so it starts under its
    /// own weekday.
    private var cells: [Date?] {
        guard let range = calendar.range(of: .day, in: .month, for: month) else { return [] }
        let weekday = calendar.component(.weekday, from: month)
        let lead = (weekday - calendar.firstWeekday + 7) % 7
        var out: [Date?] = Array(repeating: nil, count: lead)
        for offset in 0..<range.count {
            out.append(calendar.date(byAdding: .day, value: offset, to: month))
        }
        return out
    }

    private func dayCell(_ date: Date) -> some View {
        let chosen = calendar.isDate(date, inSameDayAs: day)
        let today = calendar.isDateInToday(date)
        let allowed = (earliest.map { date >= calendar.startOfDay(for: $0) } ?? true)
            && (latest.map { date <= $0 } ?? true)
        return Button {
            // The time of day is kept; only the day changes.
            let parts = calendar.dateComponents([.hour, .minute], from: day)
            withAnimation(Theme.settle) {
                day = calendar.date(bySettingHour: parts.hour ?? 0, minute: parts.minute ?? 0, second: 0, of: date) ?? date
            }
        } label: {
            Text("\(calendar.component(.day, from: date))")
                .font(.system(size: 12.5, weight: chosen || today ? .semibold : .regular).monospacedDigit())
                .foregroundStyle(chosen ? Theme.tallyForeground : allowed ? Theme.foreground : Theme.faint(0.45))
                .frame(maxWidth: .infinity, minHeight: 30)
                .background {
                    if chosen {
                        Circle().fill(Theme.tally).frame(width: 30, height: 30).transition(.scale.combined(with: .opacity))
                    } else if today {
                        Circle().stroke(Theme.tally.opacity(0.7), lineWidth: 1).frame(width: 30, height: 30)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!allowed)
        .accessibilityLabel(date.formatted(date: .complete, time: .omitted))
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

/// An hour and a quarter of it, as two small grids in a popover each.
struct ClockChoice: View {
    let id: String
    @Binding var hour: Int
    @Binding var minute: Int
    var minutes: [Int] = [0, 15, 30, 45]
    @Environment(\.popovers) private var popovers

    var body: some View {
        if let popovers {
            HStack(spacing: 4) {
                part(id + ".hour", value: hour, label: t("Hour"), popovers: popovers) {
                    ClockGrid(values: Array(0..<24), columns: 6, chosen: hour) { hour = $0; popovers.close() }
                }
                Text(":").font(.system(size: 15, weight: .semibold)).foregroundStyle(Theme.muted)
                part(id + ".minute", value: minute, label: t("Minute"), popovers: popovers) {
                    ClockGrid(values: minutes, columns: 4, chosen: minute) { minute = $0; popovers.close() }
                }
            }
        }
    }

    private func part<Panel: View>(_ id: String, value: Int, label: String, popovers: Popovers,
                                   @ViewBuilder panel: @escaping () -> Panel) -> some View {
        ClockCapsule(value: value, label: label) { popovers.toggle(id) }
            .floatingPopover(id, in: popovers, alignment: .center, width: 236) {
                VStack(alignment: .leading, spacing: 6) {
                    MenuHeading(title: label)
                    panel()
                }
                .padding(8)
            }
    }
}

/// One half of a time, as the capsule that opens its grid.
private struct ClockCapsule: View {
    let value: Int
    let label: String
    var lit = false
    let press: () -> Void

    var body: some View {
        Button(action: press) {
            Text(String(format: "%02d", value)).font(.system(size: 14, weight: .semibold).monospacedDigit())
                .foregroundStyle(lit ? Theme.tallyInk : Theme.foreground)
                .frame(minWidth: 26)
        }
        .buttonStyle(.glassCapsule(32))
        .accessibilityLabel(label)
        .accessibilityAddTraits(lit ? .isSelected : [])
        .animation(Theme.settle, value: lit)
    }
}

/// The hours or the quarters to choose one from.
private struct ClockGrid: View {
    let values: [Int]
    let columns: Int
    let chosen: Int
    let pick: (Int) -> Void

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 4), count: columns), spacing: 4) {
            ForEach(values, id: \.self) { value in
                Button {
                    withAnimation(Theme.settle) { pick(value) }
                } label: {
                    Text(String(format: "%02d", value))
                        .font(.system(size: 13, weight: value == chosen ? .bold : .regular).monospacedDigit())
                        .foregroundStyle(value == chosen ? Theme.tallyForeground : Theme.foreground)
                        .frame(maxWidth: .infinity, minHeight: 30)
                        .background(RoundedRectangle(cornerRadius: 9, style: .continuous)
                            .fill(value == chosen ? Theme.tally : Theme.ink.opacity(0.06)))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// A moment that may be left open: a capsule saying it, or saying that it is
/// not set, opening onto a calendar and a clock (Account.tsx's `Works from`
/// and `Works until`, which the web leaves to the browser's own picker).
struct MomentChoice: View {
    let id: String
    @Binding var moment: Date?
    var earliest: Date? = Date()
    @Environment(\.popovers) private var popovers
    @Environment(\.locale) private var locale

    var body: some View {
        if let popovers {
            HStack(spacing: 6) {
                Button { popovers.toggle(id) } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "calendar").font(.system(size: 11.5, weight: .semibold))
                        Text(moment.map { LobbyModel.said($0) } ?? "—").lineLimit(1)
                    }
                    .font(.system(size: 12.5, weight: .medium))
                }
                .buttonStyle(.glassCapsule(30))
                .fixedSize()
                if moment != nil {
                    Button {
                        withAnimation(Theme.settle) { moment = nil }
                    } label: {
                        Image(systemName: "xmark").font(.system(size: 10, weight: .bold))
                    }
                    .buttonStyle(.glassCircle(26))
                    .accessibilityLabel(t("Remove"))
                    .transition(Theme.arrive)
                }
            }
            .animation(Theme.settle, value: moment == nil)
            // Hung from the leading edge: the capsule grows from a dash to a
            // whole date the moment a time is chosen, and a panel hung from
            // its trailing edge jumped sideways under the pointer midway
            // through choosing the time.
            .floatingPopover(id, in: popovers, alignment: .leading, width: 290) {
                MomentPanel(id: id, moment: $moment, earliest: earliest)
                    .environment(\.locale, locale)
                    .environment(\.popovers, popovers)
            }
        }
    }
}

/// The calendar, and under it the time, whose hour and quarter are chosen in
/// the panel itself, in the calendar's place while they are.
///
/// Not in popovers of their own, as the clock beside a schedule is: the app
/// keeps one popover open at a time, so pressing the hour inside this one
/// closed it, took the hour's anchor with it, and left nothing on screen but
/// the invisible catcher a popover lays over the page. The time of a guest
/// link could not be set at all, and every link opened on the next hour.
private struct MomentPanel: View {
    let id: String
    @Binding var moment: Date?
    let earliest: Date?
    @State private var part: Part?

    enum Part { case hour, minute }

    var body: some View {
        let day = Binding<Date>(get: { moment ?? Self.nextHour }, set: { moment = $0 })
        let calendar = Calendar.current
        let hour = calendar.component(.hour, from: day.wrappedValue)
        let minute = calendar.component(.minute, from: day.wrappedValue) / 15 * 15
        VStack(spacing: 12) {
            // The calendar keeps its room while a grid is over it, so the
            // panel does not change size under the pointer.
            ZStack(alignment: .top) {
                MonthCalendar(day: day, earliest: earliest)
                    .opacity(part == nil ? 1 : 0)
                    .allowsHitTesting(part == nil)
                    .accessibilityHidden(part != nil)
                if let part {
                    VStack(alignment: .leading, spacing: 6) {
                        MenuHeading(title: part == .hour ? t("Hour") : t("Minute"))
                        if part == .hour {
                            ClockGrid(values: Array(0..<24), columns: 6, chosen: hour) { picked in
                                day.wrappedValue = calendar.date(bySettingHour: picked, minute: minute, second: 0, of: day.wrappedValue) ?? day.wrappedValue
                                self.part = nil
                            }
                        } else {
                            ClockGrid(values: [0, 15, 30, 45], columns: 4, chosen: minute) { picked in
                                day.wrappedValue = calendar.date(bySettingHour: hour, minute: picked, second: 0, of: day.wrappedValue) ?? day.wrappedValue
                                self.part = nil
                            }
                        }
                    }
                    .transition(Theme.arrive)
                }
            }
            HStack {
                Text(t("Time")).font(.system(size: 12)).foregroundStyle(Theme.muted)
                Spacer()
                HStack(spacing: 4) {
                    ClockCapsule(value: hour, label: t("Hour"), lit: part == .hour) { part = part == .hour ? nil : .hour }
                    Text(":").font(.system(size: 15, weight: .semibold)).foregroundStyle(Theme.muted)
                    ClockCapsule(value: minute, label: t("Minute"), lit: part == .minute) { part = part == .minute ? nil : .minute }
                }
            }
        }
        // Nothing is set by opening it: an end left open is a link that works
        // until it is stopped, and a moment written merely by looking would
        // be a link that stops when nobody asked it to.
        .padding(14)
        .animation(Theme.settle, value: part)
        .onAppear {
            #if DEBUG
            // TOMOSHIBI_DEBUG_MOMENT=hour|minute: that half's grid already
            // showing, for a test to see it without a pointer.
            switch ProcessInfo.processInfo.environment["TOMOSHIBI_DEBUG_MOMENT"] {
            case "hour": part = .hour
            case "minute": part = .minute
            default: break
            }
            #endif
        }
    }

    static var nextHour: Date {
        let now = Date().timeIntervalSinceReferenceDate
        return Date(timeIntervalSinceReferenceDate: ((now / 3600).rounded(.down) + 1) * 3600)
    }
}

// MARK: - Pages of settings

/// The heading a page of settings opens with: its symbol in a lit tile, its
/// name large, and a line saying what it is for.
struct SettingsPageHeading: View {
    let title: String
    let symbol: String
    var detail: String?

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            SectionTile(symbol: symbol, lit: true, size: 44)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.system(size: 24, weight: .bold))
                if let detail {
                    Text(detail).font(.system(size: 12.5)).foregroundStyle(Theme.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

/// A section's symbol in a rounded tile, lit in the person's colour where it
/// is the one chosen: the colour itself, flat, as a prominent button wears
/// it. It shaded from 95 to 72 per cent down the tile until the owner asked
/// for fewer gradients (2026-10-20).
struct SectionTile: View {
    let symbol: String
    var lit = false
    var size: CGFloat = 28

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: size * 0.44, weight: .semibold))
            .foregroundStyle(lit ? Theme.tallyForeground : Theme.foreground)
            .frame(width: size, height: size)
            .background {
                RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                    .fill(lit ? Theme.tally : Theme.ink.opacity(0.10))
            }
            .overlay(RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                .stroke(lit ? Color.white.opacity(0.22) : Theme.ink.opacity(0.06)))
            .animation(Theme.settle, value: lit)
    }
}
