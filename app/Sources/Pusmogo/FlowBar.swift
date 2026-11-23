import AppKit
import SwiftUI

/// The sound bar at the bottom of the screen. A borderless, non-activating
/// panel: it never takes focus, floats over full-screen apps and every Space,
/// and only accepts clicks on its buttons: in hands-free mode (stop / cancel),
/// on the dictionary offer, and on the idle bar while the mouse is over it.
@MainActor
final class FlowBarController {
    private let panel: NSPanel
    private let state: AppState
    private static let size = CGSize(width: 360, height: 112)
    private var mouseMonitors: [Any] = []
    private var collapse: DispatchWorkItem?
    /// `PUSMOGO_PREVIEW_HOVER=1` keeps the idle bar hovered (screenshots);
    /// `track` follows the real mouse. Either way the app quits by itself.
    private let previewHover = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_HOVER"] == "1"

    init(state: AppState) {
        self.state = state
        panel = NSPanel(
            contentRect: NSRect(origin: .zero, size: Self.size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.isMovable = false
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        let host = NSHostingView(rootView: FlowBarView(state: state))
        host.frame = NSRect(origin: .zero, size: Self.size)
        panel.contentView = host
        reposition()
        NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reposition() }
        }
        // Mouse moves over other apps (global) and over the bar itself (local).
        // Mouse events need no permission; nothing is recorded.
        let moved: NSEvent.EventTypeMask = [.mouseMoved, .leftMouseDragged]
        if let g = NSEvent.addGlobalMonitorForEvents(matching: moved, handler: { [weak self] _ in
            MainActor.assumeIsolated { self?.trackMouse() }
        }) { mouseMonitors.append(g) }
        if let l = NSEvent.addLocalMonitorForEvents(matching: moved, handler: { [weak self] e in
            MainActor.assumeIsolated { self?.trackMouse() }
            return e
        }) { mouseMonitors.append(l) }
        update()
        if previewHover { state.barHovered = true }
    }

    /// Called whenever the phase changes.
    func update() {
        let visible = state.phase != .idle || state.showIdleBar || state.editSuggestion != nil
        if visible {
            reposition()
            panel.orderFrontRegardless()
        } else {
            panel.orderOut(nil)
        }
        if !hoverAllowed, state.barHovered, !previewHover { state.barHovered = false }
        if case .listening(locked: true) = state.phase {
            panel.ignoresMouseEvents = false
        } else if state.editSuggestion != nil {
            // The "Add to dictionary?" offer has buttons too.
            panel.ignoresMouseEvents = false
        } else {
            panel.ignoresMouseEvents = !(hoverAllowed && state.barHovered)
        }
    }

    /// Hover only grows the idle bar the user chose to see, with nothing else on it.
    private var hoverAllowed: Bool {
        state.phase == .idle && state.showIdleBar && state.editSuggestion == nil
    }

    /// The part of the panel that reacts to the mouse, in screen points: a
    /// generous target around the idle capsule, or the grown pill plus a margin.
    private var hotRect: NSRect {
        let f = panel.frame
        let s = state.barHovered ? FlowBarView.hoverSize(state) : CGSize(width: 90, height: 22)
        let margin: CGFloat = state.barHovered ? 10 : 0
        return NSRect(x: f.midX - s.width / 2 - margin, y: f.minY,
                      width: s.width + 2 * margin, height: s.height + margin)
    }

    private func trackMouse() {
        guard hoverAllowed, panel.isVisible, !previewHover else { return }
        let inside = hotRect.contains(NSEvent.mouseLocation)
        if inside {
            collapse?.cancel()
            collapse = nil
            if !state.barHovered { state.barHovered = true }
            panel.ignoresMouseEvents = false
        } else if state.barHovered, collapse == nil {
            // Clicks outside the pill pass through at once; the pill itself
            // waits a moment, so a mouse that slips off does not lose it.
            panel.ignoresMouseEvents = true
            let work = DispatchWorkItem { [weak self] in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.collapse = nil
                    if !self.hotRect.contains(NSEvent.mouseLocation) { self.state.barHovered = false }
                    self.update()
                }
            }
            collapse = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6, execute: work)
        } else if !state.barHovered {
            panel.ignoresMouseEvents = true
        }
    }

    /// Bottom centre of the screen the user is working on, above the Dock.
    /// It stays put while hovered, so the pill does not jump under the mouse.
    private func reposition() {
        guard !state.barHovered else { return }
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { $0.frame.contains(mouse) } ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }
        panel.setFrameOrigin(NSPoint(x: frame.midX - Self.size.width / 2, y: frame.minY + DS.Bar.bottomInset))
    }
}

struct FlowBarView: View {
    @ObservedObject var state: AppState

    var body: some View {
        VStack(spacing: DS.Space.s) {
            Spacer(minLength: 0)
            if showsLiveText {
                LiveCaption(text: state.liveText)
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            } else if hovered, let tip = tooltip {
                tip.transition(.opacity.combined(with: .move(edge: .bottom)))
            } else if showsOffer, let s = state.editSuggestion {
                DictionaryOffer(suggestion: s,
                                accept: { s.learned ? state.undoLearnedSuggestion() : state.acceptEditSuggestion() },
                                dismiss: { state.dismissEditSuggestion() })
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
            pill
        }
        .frame(width: 360, height: 112)
        .animation(DS.Motion.fade, value: showsLiveText)
        .animation(DS.Motion.fade, value: state.editSuggestion)
        .animation(DS.Motion.morph, value: hovered)
        .animation(DS.Motion.fade, value: hoveredButton)
    }

    // MARK: Hovered idle bar

    @State private var hoveredButton: HoverButton?
    private enum HoverButton: String { case dictate, notes, panel }
    /// `PUSMOGO_PREVIEW_HOVER_BUTTON=dictate|notes|panel` shows that button's
    /// tooltip in a hover preview, where no mouse is over the pill.
    private static let previewButton = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_HOVER_BUTTON"]
        .flatMap(HoverButton.init(rawValue:))

    /// The notetaker is planned; `PUSMOGO_NOTETAKER=1` shows its button, disabled.
    static let showsNotetaker = ProcessInfo.processInfo.environment["PUSMOGO_NOTETAKER"] == "1"
    private static let buttonSize: CGFloat = 24
    /// The grown pill: microphone, the notetaker when shown, and the panel
    /// button while macOS hides the menu bar icon.
    static func hoverSize(_ state: AppState) -> CGSize {
        let n = CGFloat(1 + (showsNotetaker ? 1 : 0) + (state.menuIconHidden ? 1 : 0))
        return CGSize(width: n * buttonSize + (n - 1) * DS.Space.s + 2 * 6, height: 36)
    }

    private var hovered: Bool { state.phase == .idle && state.barHovered }

    /// The label above the pill: the button under the mouse, or the only one.
    private var tooltip: BarTooltip? {
        let alone = !Self.showsNotetaker && !state.menuIconHidden
        switch hoveredButton ?? Self.previewButton ?? (alone ? .dictate : nil) {
        case .dictate: return BarTooltip(text: "Diktálás", key: "fn")
        case .notes: return BarTooltip(text: "Jegyzetelő · hamarosan", key: "⌥M")
        case .panel: return BarTooltip(text: "Rejtett menüikon · panel", key: "⌃⌥P")
        case nil: return nil
        }
    }

    private var hoverButtons: some View {
        HStack(spacing: DS.Space.s) {
            Button { state.startFromBar() } label: {
                Image(systemName: "mic.fill")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: Self.buttonSize, height: Self.buttonSize)
                    .background(Circle().fill(DS.Color.accent))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Diktálás"))
            .onHover { hoveredButton = $0 ? .dictate : (hoveredButton == .dictate ? nil : hoveredButton) }
            if Self.showsNotetaker {
                Image(systemName: "record.circle")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(DS.Color.barInk.opacity(0.4))
                    .frame(width: Self.buttonSize, height: Self.buttonSize)
                    .background(Circle().fill(Color.white.opacity(0.08)))
                    .accessibilityLabel(Text("Jegyzetelő · hamarosan"))
                    .onHover { hoveredButton = $0 ? .notes : (hoveredButton == .notes ? nil : hoveredButton) }
            }
            if state.menuIconHidden {
                // The icon is out of reach, so the bar is the way into History,
                // Dictionary and Settings; Home then says how to make room.
                Button { state.openPanelFromBar() } label: {
                    Image(systemName: "menubar.rectangle")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(DS.Color.barInk)
                        .frame(width: Self.buttonSize, height: Self.buttonSize)
                        .background(Circle().fill(Color.white.opacity(0.12)))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Rejtett menüikon · panel"))
                .onHover { hoveredButton = $0 ? .panel : (hoveredButton == .panel ? nil : hoveredButton) }
            }
        }
    }

    /// The offer waits while the bar is busy with a dictation.
    private var showsOffer: Bool {
        switch state.phase {
        case .listening, .thinking, .loading: return false
        default: return true
        }
    }

    private var showsLiveText: Bool {
        guard !state.liveText.isEmpty else { return false }
        switch state.phase {
        case .listening, .thinking: return true
        default: return false
        }
    }

    private var size: CGSize {
        switch state.phase {
        case .idle: return state.barHovered ? Self.hoverSize(state) : DS.Bar.idle
        case .listening(let locked): return locked ? DS.Bar.locked : DS.Bar.listening
        case .thinking where state.redoLanguage != nil: return DS.Bar.thinkingTagged
        case .thinking, .loading: return DS.Bar.thinking
        case .done: return DS.Bar.done
        case .message(let text): return CGSize(width: min(330, 52 + CGFloat(text.count) * 6.9), height: 32)
        case .info(let text): return CGSize(width: max(64, 34 + CGFloat(text.count) * 7.4), height: 32)
        }
    }

    private var pill: some View {
        content
            .frame(width: size.width, height: size.height)
            .flowBarSurface()
            .animation(DS.Motion.morph, value: state.phase)
            .animation(DS.Motion.morph, value: state.barHovered)
    }

    @ViewBuilder
    private var content: some View {
        switch state.phase {
        case .idle where state.barHovered:
            hoverButtons
                .transition(.opacity)
        case .idle:
            Color.clear
        case .listening(let locked):
            HStack(spacing: DS.Space.s) {
                if locked {
                    BarButton(symbol: "xmark", help: "Mégse (Esc)") { state.cancel() }
                }
                Text(state.language.tag)
                    .font(.system(size: 9.5, weight: .bold, design: .rounded))
                    .foregroundStyle(DS.Color.barInk.opacity(0.55))
                    .contentTransition(.numericText())
                    .animation(DS.Motion.fade, value: state.language)
                Waveform(levels: state.levels, tint: locked ? DS.Color.accent : DS.Color.barInk)
                if locked {
                    BarButton(symbol: "stop.fill", help: "Kész (fn)", filled: true) { state.stop() }
                }
            }
            .padding(.horizontal, locked ? 6 : 14)
            .transition(.opacity)
        case .thinking where state.redoLanguage != nil:
            // Mid-dictation language swap: the clip is redone in this language.
            HStack(spacing: DS.Space.s) {
                Text(state.redoLanguage?.tag ?? "")
                    .font(.system(size: 9.5, weight: .bold, design: .rounded))
                    .foregroundStyle(DS.Color.barInk.opacity(0.55))
                ThinkingDots()
            }
            .transition(.opacity)
        case .thinking, .loading:
            ThinkingDots()
                .transition(.opacity)
        case .done:
            Image(systemName: "checkmark")
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(DS.Color.barInk)
                .transition(.scale.combined(with: .opacity))
        case .info(let text):
            Text(text)
                .font(DS.Font.label)
                .foregroundStyle(DS.Color.barInk)
                .transition(.opacity)
        case .message(let text):
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(DS.Color.warning)
                Text(text).lineLimit(1).foregroundStyle(DS.Color.barInk)
            }
            .font(DS.Font.label)
            .padding(.horizontal, 12)
            .transition(.opacity)
        }
    }
}

/// What a button on the hovered idle bar does, with its key, above the pill.
struct BarTooltip: View {
    let text: LocalizedStringKey
    let key: String

    var body: some View {
        HStack(spacing: 6) {
            Text(text).foregroundStyle(DS.Color.barInk)
            Text(key)
                .font(.system(size: 10, weight: .semibold, design: .rounded))
                .foregroundStyle(DS.Color.barInk.opacity(0.8))
                .padding(.horizontal, 5)
                .frame(minWidth: 18, minHeight: 17)
                .background(RoundedRectangle(cornerRadius: DS.Radius.key, style: .continuous)
                    .strokeBorder(DS.Color.barInk.opacity(0.35), lineWidth: 0.75))
        }
        .font(DS.Font.label)
        .padding(.horizontal, 10)
        .frame(height: 28)
        .flowBarSurface()
    }
}

/// The words transcribed so far, above the bar during a long dictation. Two
/// lines at most, cut at the start so the newest words stay in view.
struct LiveCaption: View {
    let text: String

    /// Width of the text inside the caption: `maxWidth` minus the padding.
    private static let textWidth: CGFloat = 340 - 2 * 12
    private static let lines = 2
    /// AppKit twin of `DS.Font.label`, used only to measure.
    private static let font = NSFont.systemFont(ofSize: 12, weight: .medium)

    /// The newest words that fit in two lines, starting at a word, with "…"
    /// in front when anything was cut. SwiftUI's head truncation over two
    /// lines cuts in the middle and hides the newest words, so trim here,
    /// measured with the real font: a fixed character count leaves half a
    /// line empty for narrow letters and overflows for wide ones.
    static func tail(_ text: String) -> String {
        if fits(text) { return text }
        // Word starts after the first character, oldest first. The shortest
        // candidates fit, so search for the earliest start that still fits.
        let starts = text.indices.filter { $0 > text.startIndex && text[text.index(before: $0)] == " " }
        var lo = 0, hi = starts.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if fits("…" + text[starts[mid]...]) { hi = mid } else { lo = mid + 1 }
        }
        if lo < starts.count { return "…" + text[starts[lo]...] }
        // One unbroken word longer than two lines: keep its end.
        var cut = text.endIndex
        while cut > text.startIndex, fits("…" + text[text.index(before: cut)...]) { cut = text.index(before: cut) }
        return "…" + text[cut...]
    }

    private static func fits(_ s: String) -> Bool {
        let attributed = NSAttributedString(string: s, attributes: [.font: font])
        let rect = attributed.boundingRect(
            with: NSSize(width: textWidth, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading])
        let lineHeight = NSLayoutManager().defaultLineHeight(for: font)
        return rect.height <= lineHeight * CGFloat(lines) + 0.5
    }

    var body: some View {
        Text(Self.tail(text))
            .font(DS.Font.label)
            .foregroundStyle(DS.Color.barInk.opacity(0.9))
            .lineLimit(2)
            .truncationMode(.head)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .frame(maxWidth: 340)
            .fixedSize(horizontal: false, vertical: true)
            .flowBarSurface(cornerRadius: DS.Radius.card)
            .contentTransition(.opacity)
    }
}

/// "Add to dictionary?" after the user corrected a dictated word (C10):
/// the two spellings, an add button and a dismiss button.
struct DictionaryOffer: View {
    let suggestion: EditWatcher.Suggestion
    let accept: () -> Void
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: DS.Space.s) {
            Image(systemName: suggestion.learned ? "checkmark.circle" : "character.book.closed")
                .foregroundStyle(DS.Color.barInk.opacity(0.7))
            // Each spelling truncates on its own and they share the room,
            // so the arrow between them always shows.
            HStack(spacing: 4) {
                Text(suggestion.wrong).strikethrough()
                    .foregroundColor(DS.Color.barInk.opacity(0.55))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text("→").foregroundColor(DS.Color.barInk.opacity(0.55))
                    .fixedSize()
                Text(suggestion.right)
                    .foregroundColor(DS.Color.barInk)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Button(action: accept) {
                Text(suggestion.learned ? LocalizedStringKey("Visszavonás") : LocalizedStringKey("Szótárba"))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 10)
                    .frame(height: 22)
                    .background(Capsule().fill(DS.Color.accent))
            }
            .buttonStyle(.plain)
            .help(suggestion.learned ? LocalizedStringKey("Kétszer javítottad így, ezért felvettem a szótárba. Visszavonás: kiveszem.")
                                     : LocalizedStringKey("Felveszem a szótárba, legközelebb így írom"))
            BarButton(symbol: "xmark", help: "Elvetés", action: dismiss)
        }
        .font(DS.Font.label)
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .frame(height: 34)
        .flowBarSurface()  // a 34 pt capsule
        .padding(.horizontal, 10)  // long words truncate inside the bar's width
    }
}

/// Live waveform: the most recent mic levels, newest on the right, shaped by a
/// soft bell so the centre moves most (calm at the edges, alive in the middle).
struct Waveform: View {
    let levels: [Float]
    var tint: Color = .white
    private let bars = 17

    var body: some View {
        Canvas { ctx, size in
            let gap: CGFloat = 2.5
            let w = (size.width - gap * CGFloat(bars - 1)) / CGFloat(bars)
            let recent = Array(levels.suffix(bars))
            let pad = bars - recent.count
            for i in 0..<bars {
                let raw = i < pad ? 0 : CGFloat(recent[i - pad])
                let x = CGFloat(i) / CGFloat(bars - 1) - 0.5
                let bell = 0.35 + 0.65 * exp(-x * x / 0.08)
                let h = max(3, min(size.height, 3 + raw * bell * (size.height - 3)))
                let rect = CGRect(x: CGFloat(i) * (w + gap), y: (size.height - h) / 2, width: w, height: h)
                ctx.fill(Path(roundedRect: rect, cornerRadius: w / 2), with: .color(tint))
            }
        }
        .frame(height: 18)
        .animation(.linear(duration: 0.06), value: levels)
    }
}

/// Three dots travelling in a wave while the text is being written.
struct ThinkingDots: View {
    var body: some View {
        TimelineView(.animation) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            HStack(spacing: 5) {
                ForEach(0..<3, id: \.self) { i in
                    let phase = sin(t * 7 - Double(i) * 0.9)
                    Circle()
                        .fill(DS.Color.barInk.opacity(0.45 + 0.55 * max(0, phase)))
                        .frame(width: 5, height: 5)
                        .offset(y: -2.5 * max(0, phase))
                }
            }
        }
    }
}

struct BarButton: View {
    let symbol: String
    let help: LocalizedStringKey
    var filled = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(filled ? .white : DS.Color.barInk.opacity(0.8))
                .frame(width: 22, height: 22)
                .background(Circle().fill(filled ? DS.Color.accent : Color.white.opacity(0.12)))
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(Text(help))  // an icon alone gives VoiceOver nothing to read
    }
}
