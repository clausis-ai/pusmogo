import AppKit
import Carbon
import SwiftUI

/// Raycast-style first-run explainer: one window, one idea per step, a big
/// azure stage with a live illustration, and real practice (you dictate into it).
@MainActor
enum Onboarding {
    static var isNeeded: Bool { !UserDefaults.standard.bool(forKey: "onboarded") }

    /// `PUSMOGO_ONBOARDING_STEP=<0-7>` opens the window at that step, for
    /// screenshots and for testing the try-it step with the self-test.
    static var startStep: Int? {
        ProcessInfo.processInfo.environment["PUSMOGO_ONBOARDING_STEP"].flatMap(Int.init)
    }
    private static var window: NSWindow?
    private static var closeObserver: NSObjectProtocol?

    static func show(state: AppState) {
        if window == nil {
            let w = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 720, height: 560),
                styleMask: [.titled, .closable, .fullSizeContentView],
                backing: .buffered,
                defer: false
            )
            w.titlebarAppearsTransparent = true
            w.titleVisibility = .hidden
            w.isMovableByWindowBackground = true
            w.isReleasedWhenClosed = false
            w.contentView = NSHostingView(rootView: OnboardingView(state: state, models: state.models))
            w.center()
            window = w
            closeObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.willCloseNotification, object: w, queue: .main
            ) { _ in
                MainActor.assumeIsolated {
                    NSApp.setActivationPolicy(.accessory)
                    state.micReadyForSetup = false
                }
            }
        }
        // A menu bar app has no Dock icon and no ⌘Tab entry, so a setup window
        // that falls behind System Settings while the user grants a permission
        // cannot be reached again. During setup Pusmogo is a regular app;
        // closing or finishing the window makes it a menu bar app again.
        NSApp.setActivationPolicy(.regular)
        state.micReadyForSetup = true
        bringToFront()
    }

    /// Brings the setup window back in front, e.g. when a permission was just
    /// granted in System Settings. macOS no longer lets an app take focus with
    /// `ignoringOtherApps`, so the window is also ordered front regardless.
    static func bringToFront() {
        guard let window else { return }
        if #available(macOS 14.0, *) {
            NSApp.activate()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
    }

    static func finish() {
        UserDefaults.standard.set(true, forKey: "onboarded")
        window?.close()
    }
}

private enum Step: Int, CaseIterable {
    case welcome, permissions, fnKey, tryIt, handsFree, language, huModel, done
}

struct OnboardingView: View {
    @ObservedObject var state: AppState
    @ObservedObject var models: ModelStore
    @State private var step: Step = Onboarding.startStep.flatMap(Step.init(rawValue:)) ?? .welcome
    @FocusState private var tryFieldFocused: Bool
    @State private var fnSeen = false
    @State private var tryText = ""
    @State private var handsFreeSeen = false
    @State private var languageSwapped = false
    @State private var fnUsage = FnSetting.current

    var body: some View {
        VStack(spacing: 0) {
            Stage { illustration }
                .padding(.horizontal, 28)
                .padding(.top, 40)
                .frame(height: 300)

            VStack(spacing: DS.Space.s) {
                Text(title)
                    .font(.system(size: 24, weight: .semibold))
                    .multilineTextAlignment(.center)
                Text(subtitle)
                    .font(.system(size: 14))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 520)
                    .fixedSize(horizontal: false, vertical: true)
                action.padding(.top, DS.Space.s)
            }
            .padding(.top, 22)
            .padding(.horizontal, 40)

            Spacer(minLength: 0)
            footer
        }
        .frame(width: 720, height: 560)
        .background(Color(nsColor: .windowBackgroundColor))
        .animation(DS.Motion.morph, value: step)
        .onAppear {
            if let secs = ProcessInfo.processInfo.environment["PUSMOGO_ONBOARDING_CYCLE"].flatMap(Double.init) {
                // Design review: walk through every step without downloading anything.
                Timer.scheduledTimer(withTimeInterval: secs, repeats: true) { timer in
                    MainActor.assumeIsolated {
                        if step == .done { timer.invalidate() } else { go(1) }
                    }
                }
            } else {
                models.download()
            }
        }
        .onChange(of: state.fnPressed) { down in if down { fnSeen = true } }
        .onChange(of: state.phase) { phase in
            if phase == .listening(locked: true) { handsFreeSeen = true }
        }
        .onChange(of: state.language) { _ in languageSwapped = true }
        .onChange(of: state.accessibilityGranted) { granted in if granted { Onboarding.bringToFront() } }
        .onChange(of: state.micGranted) { granted in if granted { Onboarding.bringToFront() } }
        .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { _ in
            fnUsage = FnSetting.current
        }
    }

    // MARK: - Copy

    private var title: String {
        switch step {
        case .welcome: return String(localized: "Szia, ez a Pusmogo.")
        case .permissions: return String(localized: "Két engedély, és kész.")
        case .fnKey: return String(localized: "A te gombod: fn")
        case .tryIt: return String(localized: "Próbáld ki most.")
        case .handsFree: return String(localized: "Hosszabb szöveg? Kihangosítva.")
        case .language: return String(localized: "Magyar vagy angol.")
        case .huModel: return String(localized: "A saját magyar modellünk.")
        case .done: return String(localized: "A menüsorban vagyok.")
        }
    }

    private var subtitle: String {
        switch step {
        case .welcome:
            return String(localized: "Mondd, és már le is van írva. Bármelyik appban, magyarul és angolul, teljesen a gépeden. Ingyen és nyílt forráskóddal.")
        case .permissions:
            return String(localized: "A mikrofonnal hallak, a kisegítő hozzáféréssel figyelem az fn gombot és írom be a szöveget. A hangod soha nem hagyja el a Macet.")
        case .fnKey:
            return String(localized: "A billentyűzet bal alsó sarkában. Tartsd lenyomva, amíg beszélsz, engedd el, és beírom. Nyomd meg most egyszer.")
        case .tryIt:
            return String(format: String(localized: "Tartsd lenyomva az fn-t, és mondd: %@"), trySentence)
        case .handsFree:
            return String(localized: "Koppints duplán az fn-re, és beszélhetsz, ameddig akarsz, akár elengedett kézzel. Ha végeztél, egy fn. Három koppintás: mégse.")
        case .language:
            return String(localized: "Tartsd lenyomva az fn-t, és nyomd meg a Shiftet: magyar ⇄ angol. A sávon mindig látod, épp melyik.")
        case .huModel:
            return String(localized: "A Pusmogo HU csak magyarul tud, de azt nagyon. 182 óra magyar beszéden tanítottuk. Olyan gyors, mint az alap modell, és jóval kevesebbet téved.")
        case .done:
            return String(localized: "Kattints az ikonra fent: előzmények, szótár, beállítások.\nNem látod az ikont? A ⌃⌥P is megnyitja.")
        }
    }

    /// The sentences to speak are in the dictation language, not the UI
    /// language: the bar shows which one the engine expects, and fn + ⇧
    /// swaps both.
    private var trySentence: String {
        switch state.language {
        case .hungarian: return "„Szia, holnap háromkor találkozunk az irodában.”"
        case .english: return "“Hi, let's meet at the office tomorrow at three.”"
        }
    }

    private var welcomeCaption: String {
        switch state.language {
        case .hungarian: return "„Holnap háromkor találkozunk.”"
        case .english: return "“See you tomorrow at three.”"
        }
    }

    // MARK: - Illustrations

    @ViewBuilder
    private var illustration: some View {
        switch step {
        case .welcome:
            VStack(spacing: 18) {
                LiveBar(locked: false, tag: state.language.tag)
                Text(verbatim: welcomeCaption)
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(.white.opacity(0.9))
            }
        case .permissions:
            HStack(spacing: 28) {
                PermissionBadge(symbol: "mic.fill", title: String(localized: "Mikrofon"), granted: state.micGranted)
                PermissionBadge(symbol: "accessibility", title: String(localized: "Kisegítő hozzáférés"), granted: state.accessibilityGranted)
            }
        case .fnKey:
            KeyboardCorner(fnPressed: state.fnPressed)
        case .tryIt:
            VStack(spacing: 12) {
                TextEditor(text: $tryText)
                    .font(.system(size: 15))
                    .scrollContentBackground(.hidden)
                    .focused($tryFieldFocused)
                    .padding(12)
                    .frame(width: 460, height: 110)
                    .background(RoundedRectangle(cornerRadius: DS.Radius.card).fill(.white))
                    .foregroundStyle(DS.Color.ink)
                    .onAppear { tryFieldFocused = true }
                if !tryText.isEmpty, let t = state.lastTimings {
                    Label(String(format: String(localized: "Kész, %.0f ms alatt."), t.totalMs), systemImage: "checkmark.circle.fill")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(.white)
                } else {
                    LiveBar(locked: false, tag: state.language.tag).scaleEffect(0.8)
                }
            }
        case .handsFree:
            VStack(spacing: 18) {
                HStack(spacing: 10) {
                    KeyCap(label: "fn", symbol: "globe", pressed: state.fnPressed)
                    KeyCap(label: "fn", symbol: "globe", pressed: state.fnPressed)
                }
                LiveBar(locked: true, tag: state.language.tag)
            }
        case .language:
            VStack(spacing: 22) {
                HStack(spacing: 10) {
                    KeyCap(label: "fn", symbol: "globe", pressed: state.fnPressed)
                    Text("+").font(.system(size: 22, weight: .medium)).foregroundStyle(.white.opacity(0.7))
                    KeyCap(label: "shift", symbol: "shift", pressed: false, wide: true)
                }
                HStack(spacing: 8) {
                    LanguageChip(text: "Magyar", active: state.language == .hungarian)
                    Image(systemName: "arrow.left.arrow.right").foregroundStyle(.white.opacity(0.7))
                    LanguageChip(text: "English", active: state.language == .english)
                }
            }
        case .huModel:
            HuModelCard()
        case .done:
            MenuBarHint()
        }
    }

    // MARK: - Actions under the text

    @ViewBuilder
    private var action: some View {
        switch step {
        case .permissions:
            HStack(spacing: 10) {
                if !state.micGranted || !state.accessibilityGranted {
                    Button("Engedélyezés") { state.requestPermissions() }
                        .buttonStyle(PrimaryButton())
                    if !state.accessibilityGranted {
                        Button("Rendszerbeállítások") { Permissions.openAccessibilitySettings() }
                            .buttonStyle(.link)
                    }
                } else {
                    Label("Minden rendben", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                }
            }
        case .fnKey:
            VStack(spacing: 8) {
                if fnUsage != .doNothing && state.altKey == .none {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(DS.Color.warning)
                        Text("Most az fn ezt is csinálja: \(fnUsage.label). Ez ütközik a diktálással.")
                            .font(.system(size: 12))
                    }
                    HStack(spacing: 10) {
                        Button("Átállítom") { FnSetting.setDoNothing(); fnUsage = FnSetting.current }
                            .buttonStyle(PrimaryButton())
                        Button("Inkább a jobb ⌥ gombbal diktálok") { state.altKey = .rightOption }
                            .buttonStyle(.link)
                    }
                    .font(.system(size: 12))
                } else if state.altKey != .none {
                    Label("A \(SettingsTab.altKeyLabel(state.altKey)) is diktál, ugyanúgy, mint az fn.", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                }
                if fnSeen {
                    Label("Megvan, működik.", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                }
            }
        case .tryIt:
            modelProgress
        case .huModel:
            HuModelProgress(store: state.huModel, base: models)
        default:
            EmptyView()
        }
    }

    @ViewBuilder
    private var modelProgress: some View {
        switch models.status {
        case .downloading(let p):
            VStack(spacing: 4) {
                ProgressView(value: p).frame(width: 260)
                Text("A beszédmodellt töltöm (\(ModelStore.displaySize)), \(Int(p * 100))%")
                    .font(DS.Font.meta).foregroundStyle(.secondary)
            }
        case .failed(let error):
            VStack(spacing: 4) {
                Text(error).font(DS.Font.meta).foregroundStyle(.red)
                Text("Ha nélküle folytatod, a háttérben töltöm tovább.")
                    .font(DS.Font.meta).foregroundStyle(.secondary)
            }
        case .missing:
            Text("Ha nélküle folytatod, a háttérben töltöm tovább.")
                .font(DS.Font.meta).foregroundStyle(.secondary)
        case .ready:
            EmptyView()
        }
    }

    /// Where the try-it step stands. Moving on is never the main button while the
    /// model is still arriving: a user who skips a half-downloaded model meets a
    /// refused first dictation.
    private enum TryGate { case downloading, needsModel, waitingForText, done }

    private var tryGate: TryGate {
        switch models.status {
        case .downloading: return .downloading
        case .missing, .failed: return .needsModel
        case .ready: return tryText.isEmpty ? .waitingForText : .done
        }
    }

    /// Footer buttons of the try-it step (Back is drawn by the footer).
    @ViewBuilder
    private var tryItButtons: some View {
        switch tryGate {
        case .downloading:
            EmptyView()
        case .needsModel:
            Button("Folytatom nélküle") {
                models.download()
                go(1)
            }
            .buttonStyle(.borderless)
            Button(models.status == .missing ? String(localized: "Modell letöltése") : String(localized: "Újra")) {
                models.download()
            }
            .buttonStyle(PrimaryButton())
            .keyboardShortcut(.defaultAction)
        case .waitingForText:
            Button("Kihagyom") { go(1) }.buttonStyle(.borderless)
        case .done:
            Button("Tovább") { go(1) }
                .buttonStyle(PrimaryButton())
                .keyboardShortcut(.defaultAction)
        }
    }

    // MARK: - Footer

    private var canContinue: Bool {
        switch step {
        case .permissions: return state.micGranted && state.accessibilityGranted
        default: return true
        }
    }

    private var footer: some View {
        HStack {
            HStack(spacing: 6) {
                ForEach(Step.allCases, id: \.rawValue) { s in
                    Capsule()
                        .fill(s == step ? DS.Color.accent : Color.primary.opacity(0.15))
                        .frame(width: s == step ? 18 : 6, height: 6)
                }
            }
            Spacer()
            if step != .welcome {
                Button("Vissza") { go(-1) }.buttonStyle(.borderless)
            }
            if step == .tryIt {
                tryItButtons
            } else if step == .done {
                Button("Kész") { Onboarding.finish() }
                    .buttonStyle(PrimaryButton())
                    .keyboardShortcut(.defaultAction)
            } else {
                Button(continueLabel) { go(1) }
                    .buttonStyle(PrimaryButton(enabled: canContinue))
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canContinue && step == .permissions && false)
            }
        }
        // Same height with or without the primary button, so the try-it step's
        // footer does not jump when its buttons change.
        .frame(minHeight: 30)
        .padding(.horizontal, 28)
        .padding(.vertical, 18)
        .overlay(alignment: .top) { Divider() }
    }

    private var continueLabel: String {
        switch step {
        case .welcome: return String(localized: "Kezdjük")
        case .permissions where !canContinue: return String(localized: "Később")
        case .handsFree where !handsFreeSeen: return String(localized: "Kihagyom")
        case .language where !languageSwapped: return String(localized: "Kihagyom")
        default: return String(localized: "Tovább")
        }
    }

    private func go(_ delta: Int) {
        if let next = Step(rawValue: step.rawValue + delta) { step = next }
    }
}

// MARK: - Pieces

/// Pusmogo HU as its Hugging Face model page shows it: repository name, what
/// it is, a few tags, and a link to the page (model card per docs/data-card.md).
private struct HuModelCard: View {
    static let page = URL(string: "https://huggingface.co/vatsonai/pusmogo-hu-001")!

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                Image(systemName: "cube.fill").font(.system(size: 20)).foregroundStyle(DS.Color.accent)
                Text(verbatim: "vatsonai / pusmogo-hu-001")
                    .font(.system(size: 18, weight: .semibold, design: .monospaced))
                    .foregroundStyle(DS.Color.ink)
            }
            Text("Whisper large-v3-turbo, csak magyarra tanítva. Ingyenes, a gépeden fut.")
                .font(.system(size: 13)).foregroundStyle(DS.Color.ink.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 6) {
                ForEach([String(localized: "Magyar"), String(localized: "Beszédfelismerés"), "whisper.cpp", ModelStore.displaySize], id: \.self) { tag in
                    Text(verbatim: tag)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(DS.Color.ink.opacity(0.75))
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(Color.black.opacity(0.06)))
                }
            }
            Link(destination: Self.page) {
                Label("Megnézem a Hugging Face-en", systemImage: "arrow.up.right.square")
                    .font(.system(size: 13, weight: .medium))
            }
            .foregroundStyle(DS.Color.accent)
        }
        .padding(22)
        .frame(width: 440, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: DS.Radius.card).fill(.white))
    }
}

/// Download state of Pusmogo HU on the setup step that shows it. It follows the
/// base model (`AppState.fetchAccurateModelIfWanted`); the button starts it at
/// once, for example when the dictation language is English.
private struct HuModelProgress: View {
    @ObservedObject var store: ModelStore
    @ObservedObject var base: ModelStore

    var body: some View {
        switch store.status {
        case .downloading(let p):
            VStack(spacing: 4) {
                ProgressView(value: p).frame(width: 260)
                Text("A Pusmogo HU töltődik (\(ModelStore.displaySize)), \(Int(p * 100))%")
                    .font(DS.Font.meta).foregroundStyle(.secondary)
            }
        case .ready:
            Label("Kész. A magyar diktálás ezt használja.", systemImage: "checkmark.circle.fill")
                .foregroundStyle(.green)
        case .failed(let error):
            VStack(spacing: 6) {
                Text(error).font(DS.Font.meta).foregroundStyle(.red)
                Button("Újra") { store.download() }.controlSize(.small)
            }
        case .missing:
            if base.status == .ready {
                Button("Letöltés (\(ModelStore.displaySize))") { store.download() }
                    .buttonStyle(PrimaryButton())
            } else {
                Text("Az alap modell után töltöm le (\(ModelStore.displaySize)).")
                    .font(DS.Font.meta).foregroundStyle(.secondary)
            }
        }
    }
}

/// The azure stage with a soft inner glow.
private struct Stage<Content: View>: View {
    @ViewBuilder let content: Content
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: DS.Radius.stage)
                .fill(DS.Color.accent)
            RadialGradient(colors: [.white.opacity(0.22), .clear], center: .center, startRadius: 10, endRadius: 320)
                .clipShape(RoundedRectangle(cornerRadius: DS.Radius.stage))
            content
        }
    }
}

/// A self-animating copy of the real sound bar for illustrations.
private struct LiveBar: View {
    let locked: Bool
    let tag: String

    static func levels(at t: Double) -> [Float] {
        (0..<17).map { i -> Float in
            let a: Double = sin(t * 3.1 + Double(i) * 0.55)
            let b: Double = sin(t * 1.3 + Double(i) * 0.21)
            return Float(0.25 + 0.6 * abs(a * b))
        }
    }

    var body: some View {
        TimelineView(.animation) { tl in
            let t = tl.date.timeIntervalSinceReferenceDate
            let levels = Self.levels(at: t)
            HStack(spacing: 8) {
                if locked { BarButton(symbol: "xmark", help: "") {} }
                Text(tag).font(.system(size: 9.5, weight: .bold, design: .rounded)).foregroundStyle(.white.opacity(0.55))
                Waveform(levels: levels, tint: locked ? DS.Color.accent : .white)
                if locked { BarButton(symbol: "stop.fill", help: "", filled: true) {} }
            }
            .padding(.horizontal, locked ? 6 : 14)
            .frame(width: locked ? DS.Bar.locked.width : DS.Bar.listening.width, height: 32)
            .background(Capsule().fill(DS.Color.barFill))
            .overlay(Capsule().strokeBorder(DS.Color.barStroke, lineWidth: 0.5))
            .scaleEffect(1.5)
        }
    }
}

struct KeyCap: View {
    let label: String
    var symbol: String? = nil
    let pressed: Bool
    var wide = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let symbol { Image(systemName: symbol).font(.system(size: 13, weight: .medium)) }
            Spacer(minLength: 0)
            Text(label).font(.system(size: 13, weight: .medium))
                .lineLimit(1)
                .minimumScaleFactor(0.7)
        }
        .padding(10)
        .frame(width: wide ? 110 : 62, height: 62, alignment: .leading)
        .foregroundStyle(pressed ? .white : DS.Color.ink)
        .background(
            RoundedRectangle(cornerRadius: DS.Radius.small)
                .fill(pressed ? DS.Color.ink : .white)
                .shadow(color: .black.opacity(pressed ? 0 : 0.25), radius: 0, y: pressed ? 0 : 3)
        )
        .offset(y: pressed ? 3 : 0)
        .animation(.spring(response: 0.15, dampingFraction: 0.7), value: pressed)
    }
}

/// The bottom-left corner of a Mac keyboard with fn highlighted.
private struct KeyboardCorner: View {
    let fnPressed: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                KeyCap(label: "shift", symbol: "shift", pressed: false, wide: true)
                KeyCap(label: "Z", pressed: false)
                KeyCap(label: "X", pressed: false)
            }
            .opacity(0.45)
            HStack(spacing: 8) {
                KeyCap(label: "fn", symbol: "globe", pressed: fnPressed)
                    .overlay(
                        RoundedRectangle(cornerRadius: DS.Radius.card)
                            .strokeBorder(.white, lineWidth: 1)
                            .padding(-4)
                            .opacity(fnPressed ? 0 : 1)
                    )
                KeyCap(label: "control", symbol: "control", pressed: false).opacity(0.45)
                KeyCap(label: "option", symbol: "option", pressed: false).opacity(0.45)
                KeyCap(label: "command", symbol: "command", pressed: false, wide: true).opacity(0.45)
            }
        }
    }
}

private struct PermissionBadge: View {
    let symbol: String
    let title: String
    let granted: Bool
    var body: some View {
        VStack(spacing: 10) {
            ZStack(alignment: .bottomTrailing) {
                Image(systemName: symbol)
                    .font(.system(size: 34, weight: .medium))
                    .foregroundStyle(DS.Color.accent)
                    .frame(width: 84, height: 84)
                    .background(RoundedRectangle(cornerRadius: DS.Radius.card).fill(.white))
                Image(systemName: granted ? "checkmark.circle.fill" : "circle.dashed")
                    .font(.system(size: 22))
                    .foregroundStyle(granted ? .green : .white.opacity(0.8))
                    .background(Circle().fill(granted ? .white : .clear).padding(3))
                    .offset(x: 8, y: 8)
            }
            Text(title).font(.system(size: 13, weight: .medium)).foregroundStyle(.white)
        }
    }
}

private struct LanguageChip: View {
    let text: String
    let active: Bool
    var body: some View {
        Text(text)
            .font(.system(size: 14, weight: .semibold))
            .padding(.horizontal, 16).padding(.vertical, 8)
            .foregroundStyle(active ? DS.Color.accent : .white)
            .background(Capsule().fill(active ? .white : .white.opacity(0.15)))
            .animation(DS.Motion.morph, value: active)
    }
}

/// A mock menu bar with the Pusmogo icon and an arrow to it.
private struct MenuBarHint: View {
    var body: some View {
        VStack(alignment: .trailing, spacing: 6) {
            HStack(spacing: 16) {
                Image(systemName: "wifi")
                Image(systemName: "battery.75percent")
                Image(nsImage: MenuBarIcon.bars(level: nil))
                    .padding(5)
                    .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(.white.opacity(0.25)))
                Text("15:05")
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 16).padding(.vertical, 8)
            .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(.black.opacity(0.25)))
            Image(systemName: "arrow.up")
                .font(.system(size: 22, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.trailing, 74)
        }
        .scaleEffect(1.3)
    }
}

struct PrimaryButton: ButtonStyle {
    var enabled = true
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(enabled ? .white : .primary)
            .padding(.horizontal, 16).padding(.vertical, 7)
            .background(RoundedRectangle(cornerRadius: DS.Radius.control)
                .fill(enabled ? DS.Color.accent : Color.primary.opacity(0.08)))
            .opacity(configuration.isPressed ? 0.85 : 1)
    }
}

/// macOS "Press 🌐 key to" setting (com.apple.HIToolbox AppleFnUsageType).
enum FnSetting: Int {
    case doNothing = 0, changeInputSource = 1, emoji = 2, dictation = 3

    /// The stored value, or macOS's default when the user never changed it:
    /// with more than one keyboard layout the 🌐 key changes the input
    /// source (seen live: a double fn press swapped U.S. and Hungarian),
    /// otherwise it shows the emoji picker.
    static var current: FnSetting {
        if let v = CFPreferencesCopyAppValue("AppleFnUsageType" as CFString, "com.apple.HIToolbox" as CFString) as? Int {
            return FnSetting(rawValue: v) ?? .emoji
        }
        return selectableKeyboardLayouts > 1 ? .changeInputSource : .emoji
    }

    /// Sets "Press 🌐 key to" to "Do Nothing", after the user clicks Átállítom in
    /// setup. The old value is not needed back. Every running app caches the
    /// setting, so writing only the preference left a double fn switching the
    /// keyboard layout until the next login (seen live); HIToolbox's own setter
    /// also tells the running apps. It is private, so the plain write stays as
    /// the fallback.
    static func setDoNothing() {
        let was = current
        if let update = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "TISUpdateFnUsageType") {
            unsafeBitCast(update, to: (@convention(c) (UInt32) -> Void).self)(UInt32(doNothing.rawValue))
        } else {
            let app = "com.apple.HIToolbox" as CFString
            CFPreferencesSetAppValue("AppleFnUsageType" as CFString, 0 as CFNumber, app)
            CFPreferencesAppSynchronize(app)
        }
        NSLog("setup: fn key set to do nothing (was \(was))")
    }

    private static var selectableKeyboardLayouts: Int {
        let filter = [kTISPropertyInputSourceCategory as String: kTISCategoryKeyboardInputSource as String,
                      kTISPropertyInputSourceIsSelectCapable as String: true] as CFDictionary
        guard let list = TISCreateInputSourceList(filter, false)?.takeRetainedValue() else { return 1 }
        return CFArrayGetCount(list)
    }

    var label: String {
        switch self {
        case .doNothing: return String(localized: "semmi")
        case .changeInputSource: return String(localized: "beviteli forrás váltása")
        case .emoji: return String(localized: "emojik megjelenítése")
        case .dictation: return String(localized: "Apple diktálás indítása")
        }
    }
}
