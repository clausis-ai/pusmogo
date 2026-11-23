import SwiftUI
import UniformTypeIdentifiers

/// The single panel under the menu bar icon: Home · History · Dictionary · Style · Insights · Settings.
struct PanelView: View {
    @ObservedObject var state: AppState
    @State private var tab = PanelView.startTab ?? .home

    /// `PUSMOGO_PANEL=<home|history|dictionary|style|insights|settings>` opens the panel on that
    /// tab in a plain window, so it can be screenshotted (the menu bar popover
    /// cannot be opened from a script).
    static let startTab: Tab? = ProcessInfo.processInfo.environment["PUSMOGO_PANEL"].flatMap { name in
        Tab.allCases.first { "\($0)" == name }
    }

    enum Tab: String, CaseIterable, Identifiable {
        case home = "Kezdőlap", history = "Előzmények", dictionary = "Szótár", style = "Stílus", insights = "Statisztika", settings = "Beállítások"
        var id: String { rawValue }
        /// Shown as a symbol instead of its name (tooltip and VoiceOver keep the name).
        var icon: String? {
            switch self {
            case .insights: return "chart.bar.xaxis"
            case .settings: return "gearshape"
            default: return nil
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            header
            Picker("", selection: $tab) {
                ForEach(Tab.allCases) { t in
                    Group {
                        // Six text labels overflow the 360-point panel in Hungarian.
                        if let icon = t.icon {
                            Image(systemName: icon).accessibilityLabel(Text(LocalizedStringKey(t.rawValue)))
                        } else {
                            Text(LocalizedStringKey(t.rawValue))
                        }
                    }
                    .help(LocalizedStringKey(t.rawValue))
                    .tag(t)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            // The tabs fit the 360-point panel only at the small size.
            .controlSize(.small)

            Group {
                switch tab {
                case .home: HomeTab(state: state)
                case .history: HistoryTab(state: state)
                case .dictionary: DictionaryTab(state: state)
                case .style: StyleTab(state: state)
                case .insights: InsightsTab(state: state)
                case .settings: SettingsTab(state: state)
                }
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)

            Divider()
            HStack {
                Text(verbatim: "Pusmogo \(Updater.version) · vatson.ai").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                Spacer()
                Button("Kilépés") { NSApplication.shared.terminate(nil) }
                    .buttonStyle(.borderless)
                    .font(DS.Font.meta)
                    .keyboardShortcut("q")
            }
        }
        .padding(DS.Space.l)
        .frame(width: 360)
        .fixedSize(horizontal: false, vertical: true)
        .background(WindowHeightFitter())
        .task {
            // `PUSMOGO_PANEL_CYCLE=<seconds>` steps through the tabs, so tab
            // switches can be checked in the real popover: AX presses and
            // synthetic clicks close it.
            guard let secs = Self.cycleSeconds else { return }
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(secs * 1_000_000_000))
                let all = Tab.allCases
                tab = all[(all.firstIndex(of: tab)! + 1) % all.count]
                NSLog("panel: tab \(tab)")
            }
        }
    }

    static let cycleSeconds = ProcessInfo.processInfo.environment["PUSMOGO_PANEL_CYCLE"].flatMap(Double.init)

    private var header: some View {
        HStack(spacing: DS.Space.s) {
            Image(nsImage: MenuBarIcon.bars(level: nil))
                .foregroundStyle(DS.Color.accent)
            Text("Pusmogo").font(DS.Font.title)
            Spacer()
            StatusChip(phase: state.phase, models: state.models,
                       permissionsMissing: !state.micGranted || !state.accessibilityGranted)
        }
    }
}

struct StatusChip: View {
    let phase: Phase
    @ObservedObject var models: ModelStore
    /// Microphone or Accessibility not granted: dictation cannot work yet.
    var permissionsMissing = false
    var body: some View {
        let (text, color): (String, Color) = {
            switch phase {
            case .idle, .done: return idle
            case .loading where models.status != .ready: return idle
            case .loading: return (String(localized: "Modell betöltése"), .orange)
            case .listening(let locked): return (locked ? String(localized: "Kihangosítva") : String(localized: "Hallgatlak"), DS.Color.accent)
            case .thinking: return (String(localized: "Írom"), DS.Color.accent)
            case .message: return (String(localized: "Figyelem"), DS.Color.warning)
            case .info(let t): return (t, DS.Color.accent)
            }
        }()
        HStack(spacing: 5) {
            Circle().fill(color).frame(width: 6, height: 6)
            Text(text).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
        }
    }

    /// "Ready" only when there is a model to dictate with and the
    /// permissions dictation needs; a missing permission outranks the model.
    private var idle: (String, Color) {
        if permissionsMissing {
            return (String(localized: "Engedély szükséges"), DS.Color.warning)
        }
        switch models.status {
        case .ready: return (String(localized: "status.ready", defaultValue: "Kész"), .green)
        case .missing: return (String(localized: "Nincs modell"), DS.Color.warning)
        case .downloading(let p): return (String(localized: "Letöltés: \(Int(p * 100))%"), DS.Color.accent)
        case .failed(let why) where why == ModelStore.damagedOnDisk:
            return (String(localized: "Sérült modell"), DS.Color.warning)
        case .failed: return (String(localized: "Letöltési hiba"), DS.Color.warning)
        }
    }
}

struct HomeTab: View {
    @ObservedObject var state: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            if !state.micGranted || !state.accessibilityGranted {
                PermissionCard(state: state)
            }
            if let moved = state.profileMovedAside {
                ProfileResetCard(movedTo: moved) { state.profileMovedAside = nil }
            }
            if state.profileFromNewerVersion {
                NewerProfileCard()
            }
            if state.menuIconHidden {
                HiddenIconCard()
            }
            if Core.hasSpeechEngine {
                ModelCard(models: state.models)
                AccurateModelLine(state: state, models: state.accurateModel)
                AccurateModelLine(state: state, models: state.huModel)
            }
            VStack(alignment: .leading, spacing: 6) {
                ShortcutRow(keys: "fn", text: "Tartsd lenyomva és beszélj")
                ShortcutRow(keys: "fn fn", text: "Dupla koppintás: kihangosítva, fn-nel állítod le")
                ShortcutRow(keys: "fn ⇧", text: "Nyelvváltás: magyar ⇄ angol")
                ShortcutRow(keys: "esc", text: "Mégse")
                ShortcutRow(keys: "⌃ ⌘ V", text: "Utolsó szöveg beillesztése")
                ShortcutRow(keys: "⌃ ⌥ P", text: "Panel, ha az ikon nem látszik")
            }
            if let last = state.lastText {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Utoljára").font(DS.Font.label).foregroundStyle(DS.Color.quiet)
                    Text(last).font(DS.Font.body).lineLimit(3).textSelection(.enabled)
                    HStack {
                        if let t = state.lastTimings {
                            Text(t.summary).font(DS.Font.mono)
                        }
                        Spacer()
                        Button { state.insertAgain(last) } label: { Image(systemName: "text.insert") }
                            .buttonStyle(.borderless)
                            .help("Beírás oda, ahol előtte voltál")
                            .accessibilityLabel("Beírás oda, ahol előtte voltál")
                        Button {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(last, forType: .string)
                        } label: { Image(systemName: "doc.on.doc") }
                        .buttonStyle(.borderless)
                        .help("Másolás")
                        .accessibilityLabel("Másolás")
                    }
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                }
                .padding(DS.Space.m)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(Color.primary.opacity(0.04)))
            }
            HStack {
                Picker("", selection: $state.language) {
                    ForEach(DictationLanguage.allCases) { Text($0.label).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 150)
                Spacer()
                Button("Bemutató") { Onboarding.show(state: state) }
                    .buttonStyle(.link).font(DS.Font.meta)
            }
            // The engine name is an implementation detail; Home only warns
            // when this build cannot transcribe at all.
            if !Core.hasSpeechEngine {
                Text("Ez a változat beszédmotor nélkül készült")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            }
        }
        .onAppear { state.models.recheck() }
    }
}

struct ShortcutRow: View {
    let keys: String
    let text: LocalizedStringKey
    var body: some View {
        HStack(spacing: DS.Space.s) {
            HStack(spacing: 3) {
                ForEach(Array(keys.split(separator: " ").map(String.init).enumerated()), id: \.offset) { _, k in
                    Text(k)
                        .font(.system(size: 11, weight: .medium))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(Color.primary.opacity(0.07)))
                        .overlay(RoundedRectangle(cornerRadius: DS.Radius.control).strokeBorder(Color.primary.opacity(0.08), lineWidth: 0.5))
                }
            }
            .frame(width: 92, alignment: .leading)
            // Keep the wrapped height: when the cards above make Home tall,
            // SwiftUI would otherwise squeeze a long label to one cut line.
            Text(text).font(DS.Font.body)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

struct PermissionCard: View {
    @ObservedObject var state: AppState
    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            Text(state.micGranted || state.accessibilityGranted
                 ? LocalizedStringKey("Egy engedély kell")
                 : LocalizedStringKey("Két engedély kell"))
                .font(DS.Font.label)
            if !state.micGranted {
                PermissionRow(title: "Mikrofon", detail: "hogy halljalak", action: Permissions.openMicrophoneSettings)
            }
            if !state.accessibilityGranted {
                PermissionRow(title: "Kisegítő hozzáférés", detail: "a gyorsbillentyűhöz és a beillesztéshez", action: Permissions.openAccessibilitySettings)
            }
        }
        .padding(DS.Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.accent.opacity(0.08)))
    }
}

/// Shown once after a damaged profile was replaced by a fresh one: the old
/// file is kept, never deleted, so the user (or support) can still look at it.
struct ProfileResetCard: View {
    let movedTo: String
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            Text("Az előzmények fájlja megsérült").font(DS.Font.label)
            Text("Új, üres profillal indultam, ezért az előzmények, a szótár és a gyorsszövegek most üresek. A régi fájlt nem töröltem, félretettem.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Button("Megmutatás a Finderben") {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: movedTo)])
                }
                .buttonStyle(.link)
                Spacer()
                Button("Rendben", action: dismiss).buttonStyle(.borderless)
            }
            .font(DS.Font.meta)
        }
        .padding(DS.Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.warning.opacity(0.10)))
    }
}

/// Shown while the profile was written by a newer Pusmogo (an older copy
/// started after a newer one). The file is never touched; updating the app
/// brings History, Dictionary and Snippets back.
struct NewerProfileCard: View {
    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            Text("Ezt az előzményt egy újabb Pusmogo írta").font(DS.Font.label)
            Text(NewerProfileCard.detail)
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(DS.Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.warning.opacity(0.10)))
    }

    static let detail = String(localized: "Frissítsd az appot, és visszakapod az előzményeket, a szótárat és a gyorsszövegeket. Addig a diktálás működik, de semmit nem mentek el, és a fájlhoz nem nyúlok.")
}

/// Home's help while macOS hides the menu bar icon (a full menu bar on a Mac
/// with a notch): how to make room, and how to reach the panel meanwhile.
struct HiddenIconCard: View {
    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            Text("A menüikon nem látszik").font(DS.Font.label)
            Text("Túl sok ikon van a menüsoron, ezért a macOS elrejtette. Tartsd lenyomva a ⌘ billentyűt, és húzz le a menüsorról néhány ikont, amire nincs szükséged, vagy lépj ki az appjukból. Addig a ⌃⌥P nyitja meg ezt a panelt.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(DS.Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.accent.opacity(0.08)))
    }
}

/// Home's notice while the speech model is not installed: dictation is refused
/// until it is, so the download (or its failure) belongs on the first screen,
/// not only in Settings. Shows nothing once the model is ready.
struct ModelCard: View {
    @ObservedObject var models: ModelStore

    var body: some View {
        if models.status != .ready {
            VStack(alignment: .leading, spacing: DS.Space.s) {
                switch models.status {
                case .downloading(let p):
                    Text("A beszédmodell töltődik").font(DS.Font.label)
                    ProgressView(value: p)
                    Text("\(Int(p * 100))% · Utána diktálhatsz.")
                        .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                case .failed(let why):
                    // A damaged file was found on disk; anything else is a download
                    // that did not finish (network, checksum, full disk).
                    Text(why == ModelStore.damagedOnDisk
                         ? LocalizedStringKey("A beszédmodell sérült")
                         : LocalizedStringKey("A beszédmodell letöltése nem sikerült"))
                        .font(DS.Font.label)
                    HStack(alignment: .firstTextBaseline) {
                        Text(why).font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer()
                        Button("Újra") { models.download() }.controlSize(.small)
                    }
                default:
                    Text("A beszédmodell nincs letöltve").font(DS.Font.label)
                    HStack(alignment: .firstTextBaseline) {
                        Text("Diktálás előtt egyszer le kell tölteni.")
                            .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer()
                        Button("Letöltés (\(ModelStore.displaySize))") { models.download() }
                            .controlSize(.small)
                    }
                }
            }
            .padding(DS.Space.m)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.accent.opacity(0.08)))
        }
    }
}

/// The second model the language wants (Pusmogo HU for Hungarian, or large-v3
/// after a "most accurate" choice) is fetched in the background once turbo is in
/// place. While it downloads, or after it failed, one quiet line says so (with
/// Retry); dictation keeps using turbo meanwhile. Nothing shows once it is
/// installed, or when the language does not want it, or while turbo itself is not
/// ready (its own card covers that).
struct AccurateModelLine: View {
    @ObservedObject var state: AppState
    @ObservedObject var models: ModelStore

    var body: some View {
        if state.models.status == .ready,
           Core.SpeechModel.wanted(state.language.code) == models.model {
            switch models.status {
            case .downloading(let p):
                VStack(alignment: .leading, spacing: DS.Space.xs) {
                    Text(models.model == .pusmogoHu
                         ? String(localized: "A Pusmogo HU magyar modell töltődik: \(Int(p * 100))%. Addig az alap modellel diktálsz.")
                         : String(localized: "Pontosabb magyar modell töltődik: \(Int(p * 100))%. Addig a gyorssal diktálsz."))
                        .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                        .fixedSize(horizontal: false, vertical: true)
                    ProgressView(value: p).controlSize(.small)
                }
            case .failed(let why):
                HStack(alignment: .firstTextBaseline) {
                    Text(models.model == .pusmogoHu
                         ? String(localized: "A Pusmogo HU modell nem töltődött le. \(why)")
                         : String(localized: "A pontosabb modell nem töltődött le. \(why)"))
                        .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer()
                    Button("Újra") { models.download() }.controlSize(.small)
                }
            default:
                EmptyView()
            }
        }
    }
}

/// "Legpontosabb / Gyors" per language. The picker shows the model the language
/// will use (the automatic default until the user picks one); picking writes an
/// explicit choice, which always wins. On a Mac with 8 GB or less, one quiet line
/// says why Hungarian starts on the fast model.
struct ModelChoiceRows: View {
    @ObservedObject var state: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            ForEach(DictationLanguage.allCases) { language in
                TrailingPickerRow(LocalizedStringKey(language.label)) {
                    Picker(language.label, selection: binding(language)) {
                        if language == .hungarian {
                            Text("Pusmogo HU").tag(Core.SpeechModel.pusmogoHu)
                        }
                        Text("Legpontosabb").tag(Core.SpeechModel.largeV3)
                        Text("Gyors").tag(Core.SpeechModel.turbo)
                    }
                }
            }
            Text("A Pusmogo HU a saját magyar modellünk: gyors, és magyarul kevesebbet téved a legpontosabbnál is. A legpontosabb lassabb, és 2 GB memóriát használ.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func binding(_ language: DictationLanguage) -> Binding<Core.SpeechModel> {
        Binding(get: { Core.SpeechModel.wanted(language.code) },
                set: { state.setModel($0, for: language) })
    }
}

struct PermissionRow: View {
    let title: LocalizedStringKey
    let detail: LocalizedStringKey
    let action: () -> Void
    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(DS.Font.body)
                Text(detail).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            }
            Spacer()
            Button("Megnyitás", action: action).controlSize(.small)
        }
    }
}

struct HistoryTab: View {
    @ObservedObject var state: AppState
    @State private var query = ""
    /// Results from the profile while a search is typed; nil shows the recent list.
    @State private var results: [HistoryEntry]?
    @State private var confirmClear = false

    private var shown: [HistoryEntry] { results ?? state.history }
    private static let startQuery = ProcessInfo.processInfo.environment["PUSMOGO_HISTORY_QUERY"]

    var body: some View {
        if state.profileFromNewerVersion && state.history.isEmpty && results == nil {
            NewerProfileCard()
        } else if state.history.isEmpty && results == nil {
            Text("Még nincs diktálás. Tartsd lenyomva az fn billentyűt bármelyik appban.")
                .font(DS.Font.body).foregroundStyle(DS.Color.quiet)
        } else {
            VStack(alignment: .leading, spacing: DS.Space.s) {
                HStack(spacing: DS.Space.xs) {
                    HistorySearchField(text: $query, placeholder: String(localized: "Keresés"))
                        .onChange(of: query) { _ in search() }
                        .onAppear {
                            // `PUSMOGO_HISTORY_QUERY=<text>` (with `PUSMOGO_PANEL=history`)
                            // starts with a typed search, so search results can be
                            // screenshotted: AX text entry does not reach the field's delegate.
                            if query.isEmpty, let q = Self.startQuery { query = q; search() }
                        }
                    Button("Összes törlése") { confirmClear = true }
                        .disabled(state.history.isEmpty)
                }
                .controlSize(.small)
                if shown.isEmpty {
                    Text("Nincs találat.").font(DS.Font.body).foregroundStyle(DS.Color.quiet)
                } else {
                    FittedScroll(maxHeight: 280) {
                        VStack(alignment: .leading, spacing: DS.Space.s) {
                            ForEach(shown) { HistoryRow(entry: $0, insert: state.insertAgain, delete: delete) }
                            if shown.count >= AppState.historyLimit {
                                Group {
                                    if results == nil {
                                        Text("A legutóbbi 200 diktálás látszik. A keresés a régebbieket is megtalálja.")
                                    } else {
                                        Text("A legutóbbi 200 találat látszik. Pontosíts a keresésen a régebbiekért.")
                                    }
                                }
                                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                                .padding(.top, DS.Space.xs)
                            }
                        }
                    }
                }
            }
            .confirmationDialog("Törlöd az összes diktálást?", isPresented: $confirmClear) {
                Button("Összes törlése", role: .destructive) {
                    state.clearHistory()
                    query = ""
                    results = nil
                }
                Button("Mégse", role: .cancel) {}
            } message: {
                Text("Az előzmények csak ezen a gépen vannak, és nem hozhatók vissza.")
            }
        }
    }

    private func search() {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { results = nil; return }
        state.searchHistory(q) { found in
            // A slower, older search must not overwrite a newer one.
            if query.trimmingCharacters(in: .whitespaces) == q { results = found }
        }
    }

    private func delete(_ entry: HistoryEntry) {
        let wasFull = (results?.count ?? 0) >= AppState.historyLimit
        state.deleteHistoryEntry(entry)
        results?.removeAll { $0.id == entry.id || (entry.storedId != nil && $0.storedId == entry.storedId) }
        // A full result list refills from the profile, like the recent list.
        if wasFull { search() }
    }
}

extension Locale {
    /// The language the UI is actually shown in, with the user's region.
    /// `Locale.current` keeps the system language, so on a system language
    /// we have no strings for (German, say) the Hungarian fallback UI would
    /// show English month names. SwiftUI's `Text(_:format:)` takes the locale from
    /// the environment, not the style. `Bundle.preferredLocalizations` is no help
    /// there either (it answers "en"), so ask the strings table itself.
    static let appUI: Locale = {
        let lang = String(localized: "locale.ui", defaultValue: "hu")
        var c = Locale.Components(locale: .current)
        c.languageComponents = Locale.Language.Components(identifier: lang)
        return Locale(components: c)
    }()
}

struct HistoryRow: View {
    let entry: HistoryEntry
    let insert: (String) -> Void
    let delete: (HistoryEntry) -> Void
    @State private var expanded = false

    /// Lines a row shows before "more", so one long dictation cannot fill the list.
    static let collapsedLines = 4

    /// Whether the text can run past `collapsedLines` in the panel's width
    /// (about 50 characters a line at the body size). A cheap estimate, so a
    /// text just under the limit may show "more" that reveals nothing new.
    private var isLong: Bool {
        let lines = entry.text.split(separator: "\n", omittingEmptySubsequences: false)
        let wrapped = lines.reduce(0) { $0 + max(1, ($1.count + 49) / 50) }
        return wrapped > Self.collapsedLines
    }

    /// "Today 14:05", "Yesterday 09:12", "Oct 4, 14:05"; older than this
    /// year adds the year, so a "Keep history: Forever" list never reads as
    /// out of order when it crosses New Year.
    private var dateLabel: Text {
        let cal = Calendar.current
        let time = entry.date.formatted(.dateTime.hour().minute().locale(.appUI))
        if cal.isDateInToday(entry.date) { return Text("Ma \(time)") }
        if cal.isDateInYesterday(entry.date) { return Text("Tegnap \(time)") }
        let style: Date.FormatStyle = cal.isDate(entry.date, equalTo: Date(), toGranularity: .year)
            ? .dateTime.month(.abbreviated).day().hour().minute()
            : .dateTime.year().month(.abbreviated).day().hour().minute()
        return Text(entry.date.formatted(style.locale(.appUI)))
    }

    /// Milliseconds below a second, seconds with one decimal above it
    /// ("45.2 s" rather than "45 210 ms").
    private var latencyLabel: Text {
        let ms = entry.timings.totalMs
        if ms < 1000 { return Text("· \(Int(ms.rounded())) ms") }
        let seconds = (ms / 1000).formatted(.number.precision(.fractionLength(1)).locale(.appUI))
        return Text("· \(seconds) mp")
    }

    /// Date, app and latency as one run of text joined by single spaces;
    /// separate views in the HStack added its 8 pt gap on top of the space
    /// before each "·", so the dots sat far from the date.
    private var metaLabel: Text {
        var label = dateLabel
        if !entry.app.isEmpty { label = label + Text(" ") + Text("· \(entry.app)") }
        return label + Text(" ") + latencyLabel
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(entry.text).font(DS.Font.body).textSelection(.enabled)
                .lineLimit(expanded ? nil : Self.collapsedLines)
            if let spoken = entry.spoken, !spoken.isEmpty {
                // The self-correction pass dropped words: show what was said.
                Text("Elhangzott: \(spoken)")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .textSelection(.enabled)
                    .lineLimit(expanded ? nil : 2)
                    .help("Ezt mondtad. Az önjavítás előtti részt kihagytuk a beírt szövegből.")
            }
            if isLong {
                Button(expanded ? "Kevesebb" : "Több") { expanded.toggle() }
                    .buttonStyle(.link)
                    .font(DS.Font.meta)
            }
            HStack {
                metaLabel
                Spacer()
                Button { insert(entry.text) } label: { Image(systemName: "text.insert") }
                    .buttonStyle(.borderless)
                    .help("Beírás oda, ahol előtte voltál")
                    .accessibilityLabel("Beírás oda, ahol előtte voltál")
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(entry.text, forType: .string)
                } label: { Image(systemName: "doc.on.doc") }
                .buttonStyle(.borderless)
                .help("Másolás")
                .accessibilityLabel("Másolás")
                Button { delete(entry) } label: { Image(systemName: "trash") }
                    .buttonStyle(.borderless)
                    .help("Törlés")
                    .accessibilityLabel("Törlés")
            }
            .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
        }
        .padding(DS.Space.s)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(Color.primary.opacity(0.04)))
    }
}

struct DictionaryTab: View {
    @ObservedObject var state: AppState
    @State private var heard = ""
    @State private var written = ""
    @State private var rejected = false
    /// The heard text of the row being edited, so a changed heard text
    /// replaces that row instead of adding a second one.
    @State private var editing: String?
    /// Filters the list by heard or written text; shown once the list is long.
    @State private var query = ""

    /// Below this many entries the whole list fits in a glance and a filter is noise.
    static let filterThreshold = 15
    private static let startQuery = ProcessInfo.processInfo.environment["PUSMOGO_DICTIONARY_QUERY"]

    private var showsFilter: Bool { state.dictionary.count > Self.filterThreshold }

    /// Newest first, so an added word shows up at the top (the core keeps
    /// the oldest-first order for the prompt), filtered by heard or written
    /// text ignoring case and accents ("kovacs" finds "Kovács").
    private var shown: [Core.DictionaryEntry] {
        let newestFirst = Array(state.dictionary.reversed())
        let q = query.trimmingCharacters(in: .whitespaces)
        guard showsFilter, !q.isEmpty else { return newestFirst }
        let opts: String.CompareOptions = [.caseInsensitive, .diacriticInsensitive]
        return newestFirst.filter {
            $0.from.range(of: q, options: opts) != nil || $0.to.range(of: q, options: opts) != nil
        }
    }

    private var canAdd: Bool {
        !heard.trimmingCharacters(in: .whitespaces).isEmpty && !written.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// Words (spelling rules for the engine) or snippets (a spoken trigger
    /// types a saved text); both are things the user teaches Pusmogo.
    enum Section: String, CaseIterable, Identifiable {
        case words = "Szavak", snippets = "Gyorsszövegek"
        var id: String { rawValue }
    }
    /// `PUSMOGO_DICTIONARY_SECTION=snippets` opens the snippets list (screenshots).
    @State private var section: Section =
        ProcessInfo.processInfo.environment["PUSMOGO_DICTIONARY_SECTION"] == "snippets" ? .snippets : .words

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Picker("", selection: $section) {
                ForEach(Section.allCases) { Text(LocalizedStringKey($0.rawValue)).tag($0) }
            }
            .pickerStyle(.segmented).labelsHidden().controlSize(.small)
            switch section {
            case .words: words
            case .snippets: SnippetsSection(state: state)
            }
        }
    }

    private var words: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Text("Nevek, márkák és szakszavak, amiket mindig így írok le. A következő diktálástól érvényes.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: DS.Space.xs) {
                TextField("Amit hallok", text: $heard)
                Image(systemName: "arrow.right").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                TextField("Ahogy írjam", text: $written)
                    .onSubmit(add)
                if editing == nil {
                    Button("Hozzáadás", action: add).disabled(!canAdd)
                } else {
                    Button("Mentés", action: add).disabled(!canAdd)
                    Button("Mégse", action: cancelEdit)
                }
            }
            .textFieldStyle(.roundedBorder)
            .controlSize(.small)
            if rejected {
                Text("Ezt nem tudtam elmenteni.").font(DS.Font.meta).foregroundStyle(DS.Color.warning)
            }
            if let n = state.dictionaryLeftOut, n > 0 {
                Text("A lista túl hosszú: a \(n) legrégebbi szót a beszédmodell már nem kapja meg előre. A cseréjük továbbra is működik.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if state.dictionary.isEmpty {
                Text("Még üres. Például: „pushmogó” → „Pusmogo”.")
                    .font(DS.Font.body).foregroundStyle(DS.Color.quiet)
            } else {
                if showsFilter {
                    HistorySearchField(text: $query, placeholder: String(localized: "Keresés"))
                        .controlSize(.small)
                        .onAppear {
                            // `PUSMOGO_DICTIONARY_QUERY=<text>` (with `PUSMOGO_PANEL=dictionary`)
                            // starts with a typed filter for screenshots: AX text entry
                            // does not reach the field's delegate.
                            if query.isEmpty, let q = Self.startQuery { query = q }
                        }
                }
                if shown.isEmpty {
                    Text("Nincs találat.").font(DS.Font.body).foregroundStyle(DS.Color.quiet)
                } else {
                    FittedScroll(maxHeight: 240) {
                        VStack(alignment: .leading, spacing: DS.Space.xs) {
                            ForEach(shown) { entry in
                                DictionaryRow(entry: entry, state: state, editing: editing == entry.from) { edit(entry) }
                            }
                        }
                    }
                }
            }
        }
    }

    private func add() {
        guard canAdd else { return }
        let old = editing
        let key = heard.trimmingCharacters(in: .whitespaces).lowercased()
        state.addDictionaryEntry(from: heard, to: written) { ok in
            rejected = !ok
            guard ok else { return }
            // Same key: the core already updated the row in place.
            if let old, old != key { state.removeDictionaryEntry(from: old) }
            // A filter could hide the row just saved; show the whole list.
            query = ""
            cancelEdit()
        }
    }

    private func edit(_ entry: Core.DictionaryEntry) {
        editing = entry.from
        heard = entry.from
        written = entry.to
        rejected = false
    }

    private func cancelEdit() {
        editing = nil
        heard = ""
        written = ""
    }
}

/// Snippets: saying only the trigger types the saved text verbatim
/// (matching ignores case, accents and punctuation; core `snippets.rs`).
struct SnippetsSection: View {
    @ObservedObject var state: AppState
    @State private var trigger = ""
    @State private var text = ""
    @State private var rejected = false
    /// The id of the snippet being edited; Save changes that one.
    @State private var editing: Int64?

    private var canSave: Bool {
        !trigger.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Text("Ha csak a kulcsszót mondod, a mentett szöveget írom be, betűre pontosan.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: DS.Space.xs) {
                TextField("Kulcsszó, amit mondasz", text: $trigger)
                TextField("Szöveg, amit beírok", text: $text, axis: .vertical)
                    .lineLimit(2...5)
                HStack(spacing: DS.Space.xs) {
                    Spacer(minLength: 0)
                    if editing != nil { Button("Mégse", action: cancelEdit) }
                    if editing == nil {
                        Button("Hozzáadás", action: save).disabled(!canSave)
                    } else {
                        Button("Mentés", action: save).disabled(!canSave)
                    }
                }
            }
            .textFieldStyle(.roundedBorder)
            .controlSize(.small)
            if rejected {
                Text("Ezt nem tudtam elmenteni: ez a kulcsszó már egy másik gyorsszövegé.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if state.snippets.isEmpty {
                Text("Még üres. Például: „címem” → a teljes címed.")
                    .font(DS.Font.body).foregroundStyle(DS.Color.quiet)
            } else {
                FittedScroll(maxHeight: 240) {
                    VStack(alignment: .leading, spacing: DS.Space.xs) {
                        // Newest first, like the word list.
                        ForEach(state.snippets.reversed()) { snippet in
                            SnippetRow(snippet: snippet, editing: editing == snippet.id,
                                       onEdit: { edit(snippet) },
                                       onDelete: { delete(snippet) })
                        }
                    }
                }
            }
            SnippetTransferRow(state: state)
        }
    }

    private func save() {
        guard canSave else { return }
        state.saveSnippet(id: editing, trigger: trigger, text: text) { ok in
            rejected = !ok
            if ok { cancelEdit() }
        }
    }

    private func edit(_ snippet: Core.Snippet) {
        editing = snippet.id
        trigger = snippet.trigger
        text = snippet.text
        rejected = false
    }

    private func delete(_ snippet: Core.Snippet) {
        if editing == snippet.id { cancelEdit() }
        state.deleteSnippet(id: snippet.id)
    }

    private func cancelEdit() {
        editing = nil
        trigger = ""
        text = ""
    }
}

/// Export and import of the snippets alone, as a JSON list of
/// `{"trigger", "text"}`; import adds to the list (a trigger already saved
/// gets the file's text) instead of replacing it.
struct SnippetTransferRow: View {
    @ObservedObject var state: AppState
    @State private var message: String?
    @State private var failed = false

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            HStack(spacing: DS.Space.s) {
                Text("Fájlba vagy fájlból").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                Spacer(minLength: DS.Space.s)
                Button("Exportálás…", action: export).disabled(state.snippets.isEmpty)
                Button("Importálás…", action: importFile)
            }
            .controlSize(.small)
            if let message {
                Text(message).font(DS.Font.meta)
                    .foregroundStyle(failed ? DS.Color.warning : DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func export() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = String(localized: "Pusmogo gyorsszövegek") + ".json"
        panel.allowedContentTypes = [.json]
        panel.canCreateDirectories = true
        panel.directoryURL = ProfileTransferRow.testDirectory
        NSApp.activate(ignoringOtherApps: true)
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            state.exportSnippets(to: url) { ok in
                failed = !ok
                message = ok ? String(localized: "Elmentve: \(url.lastPathComponent)")
                    : String(localized: "Nem sikerült elmenteni.")
            }
        }
    }

    private func importFile() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.json]
        panel.directoryURL = ProfileTransferRow.testDirectory
        NSApp.activate(ignoringOtherApps: true)
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            state.importSnippets(from: url) { result in
                failed = result == nil
                guard let result else {
                    message = String(localized: "Ez nem gyorsszöveg-lista, semmi sem változott.")
                    return
                }
                message = result.skipped == 0
                    ? String(localized: "Betöltve: \(result.added) gyorsszöveg")
                    : String(localized: "Betöltve: \(result.added) gyorsszöveg, \(result.skipped) üres kihagyva")
            }
        }
    }
}

struct SnippetRow: View {
    let snippet: Core.Snippet
    var editing = false
    var onEdit: () -> Void
    var onDelete: () -> Void
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: DS.Space.s) {
            VStack(alignment: .leading, spacing: 2) {
                Text(snippet.trigger).font(DS.Font.body)
                    .lineLimit(2).fixedSize(horizontal: false, vertical: true)
                Text(snippet.text).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .lineLimit(3).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Button(action: onEdit) { Image(systemName: "pencil") }
                .buttonStyle(.borderless)
                .foregroundStyle(editing ? DS.Color.accent : DS.Color.quiet)
                .help("Szerkesztés")
                .accessibilityLabel("Szerkesztés")
            Button(action: onDelete) { Image(systemName: "trash") }
                .buttonStyle(.borderless)
                .foregroundStyle(DS.Color.quiet)
                .help("Törlés")
                .accessibilityLabel("Törlés")
        }
        .padding(.horizontal, DS.Space.s).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(Color.primary.opacity(0.04)))
        .help(Text(verbatim: "\(snippet.trigger) → \(snippet.text)"))
    }
}

struct DictionaryRow: View {
    let entry: Core.DictionaryEntry
    @ObservedObject var state: AppState
    var editing = false
    var onEdit: () -> Void = {}
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: DS.Space.s) {
                // Long entries wrap (up to three lines each) instead of being
                // cut to one line; the tooltip always holds the full pair.
                // A fixed heard column keeps the arrows and written texts in one line.
                Text(entry.from).font(DS.Font.body).foregroundStyle(DS.Color.quiet)
                    .lineLimit(3).fixedSize(horizontal: false, vertical: true)
                    .frame(width: Self.heardWidth, alignment: .leading)
                Image(systemName: "arrow.right").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                Text(entry.to).font(DS.Font.body)
                    .lineLimit(3).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Button(action: onEdit) { Image(systemName: "pencil") }
                    .buttonStyle(.borderless)
                    .foregroundStyle(editing ? DS.Color.accent : DS.Color.quiet)
                    .help("Szerkesztés")
                    .accessibilityLabel("Szerkesztés")
                Button { state.removeDictionaryEntry(from: entry.from) } label: { Image(systemName: "trash") }
                    .buttonStyle(.borderless)
                    .foregroundStyle(DS.Color.quiet)
                    .help("Törlés")
                    .accessibilityLabel("Törlés")
            }
            if Self.replacesEverydayWord(from: entry.from, to: entry.to) {
                // A heard phrase (with a space) is several words, not one.
                Group {
                    if entry.from.trimmingCharacters(in: .whitespaces).contains(" ") {
                        Text("Ezek hétköznapi szavak is: minden „\(entry.from)” helyére „\(entry.to)” kerül.")
                    } else {
                        Text("Ez hétköznapi szó is: minden „\(entry.from)” helyére „\(entry.to)” kerül.")
                    }
                }
                    .font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, DS.Space.s).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(Color.primary.opacity(0.04)))
        .help(Text(verbatim: "\(entry.from) → \(entry.to)"))
    }
}

extension DictionaryRow {
    /// Width of the heard column; longer heard texts wrap inside it.
    static let heardWidth: CGFloat = 110
    private static var everydayCache: [String: Bool] = [:]

    /// True when the entry rewrites a word the system spell checker knows in
    /// Hungarian or English ("bolt" → "Bolt"): the replacement is
    /// case-insensitive on whole words, so it also hits the everyday meaning.
    /// Entries that keep the heard form as it is only steer the engine and
    /// never warn. Spell checking is a macOS API, so this stays in Swift.
    static func replacesEverydayWord(from: String, to: String) -> Bool {
        let heard = from.trimmingCharacters(in: .whitespaces).lowercased()
        guard !heard.isEmpty, heard != to.trimmingCharacters(in: .whitespaces) else { return false }
        if let known = everydayCache[heard] { return known }
        let checker = NSSpellChecker.shared
        let known = ["hu", "en"].contains { lang in
            checker.availableLanguages.contains(lang)
                && checker.checkSpelling(of: heard, startingAt: 0, language: lang, wrap: false,
                                         inSpellDocumentWithTag: 0, wordCount: nil).location == NSNotFound
        }
        everydayCache[heard] = known
        return known
    }
}

struct SettingsTab: View {
    @ObservedObject var state: AppState
    @State private var loginItem = LoginItem.state
    @State private var loginError: String?

    /// Settings is taller than the other tabs (~650 pt, more with the login,
    /// Bluetooth or download rows), so it scrolls inside a fixed height: on a
    /// 13-inch screen the panel would otherwise run off the bottom and hide Quit.
    static let maxHeight: CGFloat = 440

    var body: some View {
        FittedScroll(maxHeight: Self.maxHeight) {
            content.padding(.trailing, DS.Space.s)
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: DS.Space.m) {
            Toggle("Indítás bejelentkezéskor", isOn: launchAtLogin)
            if loginItem == .needsApproval {
                Button("Engedélyezd a Rendszerbeállítások → Bejelentkezési elemek között…") { LoginItem.openSettings() }
                    .buttonStyle(.link).font(DS.Font.meta)
            }
            if let loginError {
                Text(loginError).font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
            }
            TrailingPickerRow("Mikrofon") {
                Picker("Mikrofon", selection: $state.micUID) {
                    Text("Automatikus").tag("")
                    ForEach(state.micDevices) { Text($0.name).tag($0.uid) }
                    // A saved microphone that is unplugged keeps its row.
                    if !state.micUID.isEmpty, !state.micDevices.contains(where: { $0.uid == state.micUID }) {
                        Text("Nem elérhető mikrofon").tag(state.micUID)
                    }
                }
            }
            if Updater.shared.isOn {
                HStack(spacing: DS.Space.s) {
                    Toggle("Frissítések automatikus keresése", isOn: Binding(get: { Updater.shared.automatic }, set: { Updater.shared.automatic = $0 }))
                    Spacer(minLength: DS.Space.s)
                    Button("Keresés most") { Updater.shared.check() }.controlSize(.small)
                }
            }
            TrailingPickerRow("Mikrofon készenlétben") {
                Picker("Mikrofon készenlétben", selection: $state.micReady) {
                    Text("Automatikus").tag(AppState.MicReady.auto)
                    Text("Mindig").tag(AppState.MicReady.always)
                    Text("Soha").tag(AppState.MicReady.never)
                }
            }
            Text("A lassan induló mikrofon (például USB vagy monitor) a diktálások között is fut, így az első szó sem vész el. Közben látszik a macOS mikrofonjelzője; a hang csak a memóriában van, és felülíródik.")
                .font(DS.Font.meta).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if state.micIsBluetooth {
                Text("Bluetooth-mikrofonnal gyengébb a felismerés, és diktálás közben a zene is telefonminőségű. Jobb egy beépített vagy vezetékes mikrofon.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
            } else if let name = state.micInsteadOfBluetooth {
                Text("Most ezt használja: \(name). A fejhallgató mikrofonja elrontaná a zenét.")
                    .font(DS.Font.meta).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Toggle("Hangjelzések", isOn: $state.soundsOn)
            Toggle("Zene és videó szüneteltetése diktálás közben", isOn: $state.pauseMedia)
            Toggle("Kis sáv a képernyő alján, amikor nem diktálsz", isOn: $state.showIdleBar)
            VStack(alignment: .leading, spacing: DS.Space.xs) {
                Toggle("Korai élő felirat", isOn: $state.captionPreviews)
                Text("A felirat kb. 4 másodperc után megjelenik, nem csak 10 felett. Kicsit több memóriát használ.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: DS.Space.xs) {
                Toggle("Szövegkörnyezet használata", isOn: $state.contextHints)
                Text("A kurzor előtti szövegből tanul neveket és szakszavakat. Csak a gépeden, nem menti el.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: DS.Space.xs) {
                Toggle("Önjavítás felismerése", isOn: $state.selfCorrections)
                Text("Ha menet közben javítasz („háromkor, nem, négykor”), csak a javítást írja be.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
            TrailingPickerRow("Második diktálóbillentyű") {
                Picker("Második diktálóbillentyű", selection: $state.altKey) {
                    ForEach(KeyboardTap.AltKey.allCases, id: \.self) { Text(Self.altKeyLabel($0)).tag($0) }
                }
            }
            Text("fn nélküli billentyűzethez: nyomva tartva ugyanúgy diktál, mint az fn.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            TrailingPickerRow("Előzmények megőrzése") {
                Picker("Előzmények megőrzése", selection: historyDays) {
                    ForEach(retentionChoices, id: \.days) { Text($0.label).tag($0.days) }
                }
            }
            ProfileTransferRow(state: state)
            aiApps
            debugRecord
            testSetRow
            ModelStatusRow(state: state, models: state.models)
            shortcuts
            Button("Az fn (🌐) billentyű beállítása…") { Permissions.openKeyboardSettings() }
                .buttonStyle(.link)
            Text("Állítsd erre: „A 🌐 billentyű megnyomásakor: Ne történjen semmi”, különben az fn az emojikat is megnyitja.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
        .toggleStyle(TrailingSwitchStyle())
        .controlSize(.small)
        .font(DS.Font.body)
        .onAppear {
            state.refreshMicDevices()
            loginItem = LoginItem.state
            NSLog("settings: login item \(loginItem)")
        }
    }

    /// Opt-in local record of recent dictations, for reporting a misheard one.
    private var debugRecord: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            HStack(spacing: DS.Space.s) {
                Text("Utolsó 20 diktálás megőrzése")
                Spacer(minLength: DS.Space.s)
                if state.keepDebugAudio {
                    Button("Mappa") { state.openDebugFolder() }
                        .buttonStyle(.link).font(DS.Font.meta)
                }
                Toggle("Utolsó 20 diktálás megőrzése", isOn: $state.keepDebugAudio)
                    .toggleStyle(.switch).labelsHidden()
            }
            Text("Hang és szöveg, hogy egy félrehallott diktálást vissza lehessen játszani. Csak ezen a Macen marad.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// What AI apps on this Mac may read through the bundled MCP server
    /// (`pusmogo-mcp`). Read-only, local, both off by default.
    private var aiApps: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            Text("AI-alkalmazások").font(DS.Font.label)
            Toggle("Előzmények megosztása", isOn: Binding(
                get: { state.shareHistory }, set: { state.setShareHistory($0) }))
            Toggle("Szótár és gyorsszövegek megosztása", isOn: Binding(
                get: { state.shareDictionary }, set: { state.setShareDictionary($0) }))
            Text("A gépeden futó AI-alkalmazások a Pusmogo MCP-szerverén át olvashatják ezeket, módosítani nem tudják. Semmi nem kerül fel a netre.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
            McpClientsView().padding(.top, DS.Space.xs)
        }
    }

    /// Opens the test-set recording window (`TestSetWindow`): the user reads the
    /// script aloud into a local folder that `pusmogo-bench` can score.
    private var testSetRow: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            HStack(spacing: DS.Space.s) {
                Text("Tesztfelvétel")
                Spacer(minLength: DS.Space.s)
                Button("Megnyitás") {
                    TestSetWindow.show(micUID: state.micUID.isEmpty ? nil : state.micUID)
                }
                .controlSize(.small)
            }
            Text("Olvass fel rövid mondatokat, hogy mérni lehessen, mennyire ért a Pusmogo. Csak ezen a Macen marad.")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var launchAtLogin: Binding<Bool> {
        Binding(get: { loginItem == .on || loginItem == .needsApproval }, set: { on in
            switch LoginItem.set(on) {
            case .success(let s): loginItem = s; loginError = nil
            case .failure: loginItem = LoginItem.state; loginError = String(localized: "A macOS nem engedte. Próbáld a Rendszerbeállításokban.")
            }
        })
    }

    static func altKeyLabel(_ key: KeyboardTap.AltKey) -> String {
        switch key {
        case .none: return String(localized: "Nincs")
        case .rightOption: return String(localized: "Jobb ⌥")
        case .rightCommand: return String(localized: "Jobb ⌘")
        case .rightControl: return String(localized: "Jobb ⌃")
        }
    }

    static let shortcutRows: [(keys: String, action: String)] = [
        (String(localized: "fn nyomva tartva"), String(localized: "diktálás, felengedésre beírja")),
        (String(localized: "fn kétszer"), String(localized: "kihangosítva; fn újra: kész")),
        ("fn + ⇧", String(localized: "magyar ⇄ angol")),
        ("Esc", String(localized: "megszakítás")),
        ("⌃⌘V / ⌃⌘C", String(localized: "utolsó szöveg beillesztése / másolása")),
        ("⌃⌥P", String(localized: "ez a panel, ha a menüikon nem látszik")),
    ]

    /// The fixed list, plus the second push-to-talk key when one is set.
    private var shortcutRows: [(keys: String, action: String)] {
        guard state.altKey != .none else { return Self.shortcutRows }
        var rows = Self.shortcutRows
        rows.insert((String(localized: "\(Self.altKeyLabel(state.altKey)) nyomva"), String(localized: "ugyanaz, mint az fn")), at: 1)
        return rows
    }

    private var shortcuts: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            Text("Billentyűk").font(DS.Font.label)
            ForEach(shortcutRows, id: \.keys) { row in
                HStack(alignment: .firstTextBaseline, spacing: DS.Space.s) {
                    Text(row.keys).font(DS.Font.mono).frame(width: 110, alignment: .leading)
                    Text(row.action).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                }
            }
        }
    }

    static let retentionChoices: [(days: Int, label: String)] = [
        (0, String(localized: "Mindig")), (90, String(localized: "90 napig")), (30, String(localized: "30 napig")),
        (7, String(localized: "7 napig")), (1, String(localized: "1 napig")),
    ]

    /// A value set outside this menu (an imported profile) still shows.
    private var retentionChoices: [(days: Int, label: String)] {
        let d = state.historyDays
        if Self.retentionChoices.contains(where: { $0.days == d }) { return Self.retentionChoices }
        return Self.retentionChoices + [(d, String(localized: "\(d) napig"))]
    }

    private var historyDays: Binding<Int> {
        Binding(get: { state.historyDays }, set: { state.setHistoryDays($0) })
    }
}

/// Which speech model is installed, how big it is, and whether it is in memory
/// right now (it is freed after idle minutes and reloads when you start speaking).
/// Export and import of the whole profile (history, dictionary, settings)
/// as one file, for a backup or a move to another Mac.
/// A switch at the trailing edge with its label at the leading edge, so every
/// switch in Settings lines up in one column with the Export/Import buttons
/// instead of sitting right after labels of different lengths.
struct TrailingSwitchStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: DS.Space.s) {
            configuration.label
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: DS.Space.s)
            Toggle(isOn: configuration.$isOn) { configuration.label }
                .toggleStyle(.switch)
                .labelsHidden()
        }
    }
}

/// A Settings row with its menu picker on the right edge, in the same column
/// as the switches and the Export/Import buttons. The picker keeps its natural
/// width when it fits; a long choice (a "… Heritage Edition USB-C" microphone)
/// fills the space left by the label and truncates instead of widening the
/// tab past the panel.
struct TrailingPickerRow<P: View>: View {
    let label: LocalizedStringKey
    let picker: P

    init(_ label: LocalizedStringKey, @ViewBuilder picker: () -> P) {
        self.label = label
        self.picker = picker()
    }

    var body: some View {
        HStack(spacing: DS.Space.s) {
            Text(label).fixedSize()
            Spacer(minLength: DS.Space.s)
            ViewThatFits(in: .horizontal) {
                styled.fixedSize()
                styled
            }
        }
    }

    private var styled: some View {
        picker.pickerStyle(.menu).labelsHidden()
    }
}

struct ProfileTransferRow: View {
    @ObservedObject var state: AppState
    @State private var confirmImport = false
    @State private var message: String?
    @State private var failed = false

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            HStack(spacing: DS.Space.s) {
                Text("Előzmények és szótár")
                Spacer(minLength: DS.Space.s)
                Button("Exportálás…", action: export)
                Button("Importálás…") { confirmImport = true }
            }
            if let message {
                Text(message).font(DS.Font.meta)
                    .foregroundStyle(failed ? DS.Color.warning : DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .confirmationDialog("Lecseréled az előzményeidet, a szótáradat és a beállításaidat?",
                            isPresented: $confirmImport) {
            Button("Importálás…", action: importFile)
            Button("Mégse", role: .cancel) {}
        } message: {
            Text("A mostaniak helyére az exportált fájl tartalma kerül.")
        }
    }

    private func export() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = String(localized: "Pusmogo profil") + ".sqlite"
        panel.canCreateDirectories = true
        panel.directoryURL = Self.testDirectory
        NSApp.activate(ignoringOtherApps: true)
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            state.exportProfile(to: url) { ok in
                failed = !ok
                message = ok ? String(localized: "Elmentve: \(url.lastPathComponent)")
                    : String(localized: "Nem sikerült elmenteni.")
            }
        }
    }

    /// `PUSMOGO_TRANSFER_DIR=<dir or file>` opens both panels there (a file
    /// is preselected in the open panel), so a test can finish them through
    /// Accessibility. Nil keeps the macOS default.
    static var testDirectory: URL? {
        ProcessInfo.processInfo.environment["PUSMOGO_TRANSFER_DIR"].map { URL(fileURLWithPath: $0) }
    }

    /// Export names its file `.sqlite`; `.sqlite3` and `.db` stay selectable
    /// for a copy renamed by hand. macOS has no declared type for these
    /// extensions, so they are matched by extension. The core still checks
    /// the file's tables and schema version before anything changes.
    static let profileTypes: [UTType] = ["sqlite", "sqlite3", "db"]
        .compactMap { UTType(filenameExtension: $0) }

    private func importFile() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = Self.profileTypes
        panel.directoryURL = Self.testDirectory
        NSApp.activate(ignoringOtherApps: true)
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            state.importProfile(from: url) { ok in
                failed = !ok
                message = ok ? String(localized: "Betöltve: \(url.lastPathComponent)")
                    : String(localized: "Ez nem Pusmogo-profil, semmi sem változott.")
            }
        }
    }
}

struct ModelStatusRow: View {
    @ObservedObject var state: AppState
    @ObservedObject var models: ModelStore

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            Text("Beszédmodell").font(DS.Font.label)
            switch models.status {
            case .ready:
                HStack(alignment: .firstTextBaseline, spacing: DS.Space.s) {
                    Text(Self.modelName(model)).font(DS.Font.mono)
                    Text(Self.sizeOnDisk(model)).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    Spacer()
                    Button("Mappa") { NSWorkspace.shared.open(Core.modelsDirectory) }
                        .buttonStyle(.link).font(DS.Font.meta)
                }
                Text(memoryLine).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            case .downloading(let p):
                ProgressView(value: p)
                Text("Letöltés: \(Int(p * 100))%").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            case .missing:
                Button("Modell letöltése (\(ModelStore.displaySize))") { models.download() }
            case .failed(let why):
                Text(why).font(DS.Font.meta).foregroundStyle(DS.Color.warning)
                    .fixedSize(horizontal: false, vertical: true)
                Button("Újra") { models.download() }
            }
            AccurateModelLine(state: state, models: state.accurateModel)
            AccurateModelLine(state: state, models: state.huModel)
            if Core.hasSpeechEngine { ModelChoiceRows(state: state) }
        }
        .onAppear { models.recheck(); NSLog("settings: model \(models.status) loaded=\(state.engineLoaded) engine=\(state.engineName)") }
    }

    private var memoryLine: String {
        if !Core.hasSpeechEngine { return String(localized: "Ez a változat beszédmotor nélkül készült") }
        return state.engineLoaded
            ? String(localized: "Betöltve, azonnal kész")
            : String(localized: "Pihen, a memória szabad. Diktáláskor betöltöm.")
    }

    /// "ggml-large-v3-turbo-q5_0.bin" → "large-v3-turbo q5_0".
    /// The model the current language uses (the one in memory once loaded).
    private var model: Core.SpeechModel {
        state.loadedModel ?? Core.SpeechModel.pick(state.language.code) ?? .turbo
    }

    static func modelName(_ model: Core.SpeechModel) -> String {
        var name = URL(fileURLWithPath: model.path).deletingPathExtension().lastPathComponent
        if name.hasPrefix("ggml-") { name.removeFirst(5) }
        if let i = name.range(of: "-q", options: .backwards) { name.replaceSubrange(i, with: " q") }
        return name
    }

    static func sizeOnDisk(_ model: Core.SpeechModel) -> String {
        let bytes = (try? FileManager.default.attributesOfItem(atPath: model.path)[.size] as? Int64) ?? 0
        return ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}

/// The panel in an ordinary window: opened by ⌃⌥P (for when the menu bar
/// icon is hidden behind the notch) and by the `PUSMOGO_PANEL` test hook.
/// A second ⌃⌥P brings the same window back to the front.
@MainActor
enum PanelWindow {
    private static var window: NSWindow?

    static func show(state: AppState) {
        // Until setup is finished, the menu bar icon and ⌃⌥P lead back to it.
        if Onboarding.isNeeded && !AppState.oneShotTest && PanelView.startTab == nil {
            return Onboarding.show(state: state)
        }
        let w = window ?? {
            let w = EscClosingWindow(contentRect: NSRect(x: 0, y: 0, width: 360, height: 480),
                                     styleMask: [.titled, .closable], backing: .buffered, defer: false)
            w.title = "Pusmogo"
            w.isReleasedWhenClosed = false
            w.contentView = NSHostingView(rootView: PanelView(state: state))
            w.center()
            window = w
            return w
        }()
        // A one-shot test opens it for a screenshot only: never take focus
        // from the app the user is typing in.
        if AppState.oneShotTest { w.orderFrontRegardless(); return }
        if let front = NSWorkspace.shared.frontmostApplication, front != NSRunningApplication.current {
            previousApp = front
        }
        NSApp.activate(ignoringOtherApps: true)
        w.makeKeyAndOrderFront(nil)
    }

    /// The app the user was in before the window opened.
    private static var previousApp: NSRunningApplication?

    /// Closing the window would leave Pusmogo active with nothing on screen,
    /// so typing would go nowhere until the user clicks their app: hand focus
    /// back to that app, unless another of our windows (onboarding, the test
    /// recording window) is still open.
    static func windowClosed(_ closed: NSWindow) {
        defer { previousApp = nil }
        guard NSApp.isActive, let app = previousApp, !app.isTerminated else { return }
        let otherWindow = NSApp.windows.contains { $0 !== closed && $0.isVisible && $0.canBecomeMain }
        if !otherWindow { app.activate(options: []) }
    }
}

/// The History search box. A SwiftUI `TextField` hands Esc to AppKit's word
/// completion, which left the text in place and could show "no matches"; here
/// Esc clears a typed search, and on an empty field it closes the panel.
struct HistorySearchField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.placeholderString = placeholder
        field.controlSize = .small
        field.font = .systemFont(ofSize: NSFont.systemFontSize(for: .small))
        field.sendsSearchStringImmediately = true
        field.delegate = context.coordinator
        // The clear button sends the action, not a text change.
        field.target = context.coordinator
        field.action = #selector(Coordinator.searched(_:))
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.text = $text
        if field.stringValue != text { field.stringValue = text }
    }

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var text: Binding<String>
        init(text: Binding<String>) { self.text = text }

        @objc func searched(_ field: NSSearchField) {
            if text.wrappedValue != field.stringValue { text.wrappedValue = field.stringValue }
        }

        func controlTextDidChange(_ note: Notification) {
            guard let field = note.object as? NSSearchField else { return }
            text.wrappedValue = field.stringValue
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.cancelOperation(_:)) else { return false }
            if control.stringValue.isEmpty {
                control.window?.cancelOperation(nil)
            } else {
                control.stringValue = ""
                text.wrappedValue = ""
            }
            return true
        }
    }
}

/// Esc closes the panel window, as it closes the menu bar panel.
final class EscClosingWindow: NSWindow {
    override func cancelOperation(_ sender: Any?) { performClose(sender) }
    override func close() {
        super.close()
        PanelWindow.windowClosed(self)
    }
}

/// Whether macOS hid the menu bar icon. With too many status items on a Mac
/// with a notch, the ones that don't fit land under the notch or off screen.
@MainActor
enum StatusItemVisibility {
    static func isHidden() -> Bool {
        guard let item = NSApp.windows.first(where: { String(describing: type(of: $0)).contains("StatusBarWindow") })
        else {
            NSLog("menu bar: no icon window among \(NSApp.windows.map { "\(type(of: $0)) \($0.frame)" })")
            return false
        }
        let frame = item.frame
        NSLog("menu bar: icon window \(frame), occlusion visible \(item.occlusionState.contains(.visible))")
        guard let screen = item.screen ?? NSScreen.screens.first(where: { $0.frame.intersects(frame) }) else {
            NSLog("menu bar: icon window \(frame) is on no screen")
            return true
        }
        if !screen.frame.contains(frame) {
            NSLog("menu bar: icon window \(frame) is outside \(screen.frame)")
            return true
        }
        if let left = screen.auxiliaryTopLeftArea, let right = screen.auxiliaryTopRightArea {
            let notch = NSRect(x: left.maxX, y: left.minY, width: right.minX - left.maxX, height: left.height)
            if notch.width > 0 && notch.intersects(frame) {
                NSLog("menu bar: icon window \(frame) is under the notch \(notch)")
                return true
            }
        }
        return false
    }
}

/// A vertical scroll view as tall as its content, up to `maxHeight`.
///
/// A bare `ScrollView` has no ideal height, so the menu bar popover
/// (`MenuBarExtra(.window)`), which sizes itself to its content's ideal size,
/// collapsed it to nothing: Settings showed an empty band and long History or
/// Dictionary lists lost their rows. Measuring the content gives the popover a
/// definite height on every tab, and the plain ⌃⌥P window the same layout.
struct FittedScroll<Content: View>: View {
    let maxHeight: CGFloat
    @ViewBuilder let content: () -> Content
    @State private var contentHeight: CGFloat = 0

    var body: some View {
        ScrollView {
            content()
                .background(GeometryReader { proxy in
                    Color.clear.preference(key: FittedScrollHeight.self, value: proxy.size.height)
                })
        }
        .onPreferenceChange(FittedScrollHeight.self) { contentHeight = $0 }
        .frame(height: min(contentHeight, maxHeight))
    }
}

private struct FittedScrollHeight: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// Keeps the hosting window exactly as tall as the panel.
///
/// The menu bar popover (`MenuBarExtra(.window)`) grows with its content but
/// never shrinks: after a tall tab (Settings, Home with the last dictation) a
/// shorter one sat centred in the old height, with an empty band of the
/// popover's own background above and below it. This view is the panel's
/// background, so its frame is the panel's size; when that differs from the
/// window's content height, the window is resized with its top edge kept
/// under the menu bar.
private struct WindowHeightFitter: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { FitterView() }
    func updateNSView(_ view: NSView, context: Context) {}

    final class FitterView: NSView {
        override func setFrameSize(_ newSize: NSSize) {
            super.setFrameSize(newSize)
            DispatchQueue.main.async { [weak self] in self?.fitWindow() }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            DispatchQueue.main.async { [weak self] in self?.fitWindow() }
        }

        private func fitWindow() {
            let height = frame.height
            guard height > 0, let window else { return }
            let content = window.contentRect(forFrameRect: window.frame)
            guard abs(content.height - height) > 0.5 else { return }
            let fitted = window.frameRect(forContentRect: NSRect(x: content.minX, y: content.maxY - height,
                                                                 width: content.width, height: height))
            window.setFrame(fitted, display: true)
        }
    }
}
