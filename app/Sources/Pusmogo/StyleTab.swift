import SwiftUI

/// The Style tab: one writing style per kind of app, each with an example the
/// core renders live (`Core.applyStyle`, the same call every dictation goes
/// through), and the user's own app → category mapping.
///
/// Storage is shared with `AppState.captureTarget()`: UserDefaults `style.<0-3>`
/// (0 formal, 1 casual, 2 very casual; code, category 4, has one fixed style) and `styleOverrides`, one
/// `bundle.id<TAB>category` line per app (names as in core `style.rs`).
struct StyleTab: View {
    @ObservedObject var state: AppState
    @AppStorage("styleOverrides") private var overrides = ""

    static let maxHeight: CGFloat = 440

    /// Categories in the order shown; the number is the core's category id.
    static let categories: [(id: Int32, name: String, label: String)] = [
        (1, "personal", String(localized: "Személyes üzenetek")),
        (2, "work", String(localized: "Munkahelyi üzenetek")),
        (3, "email", String(localized: "E-mail")),
        (4, "code", String(localized: "Kód")),
        (0, "other", String(localized: "Minden más")),
    ]

    static let styles: [(id: Int, label: String)] = [
        (0, String(localized: "Formális")),
        (1, String(localized: "Laza")),
        (2, String(localized: "Nagyon laza")),
    ]

    var body: some View {
        FittedScroll(maxHeight: Self.maxHeight) {
            VStack(alignment: .leading, spacing: DS.Space.m) {
                Text("A szöveg formája attól függ, melyik appba diktálsz. A szavakat sosem írja át.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Self.categories, id: \.id) { cat in
                    StyleCategoryRow(category: cat.id, label: cat.label, language: state.language)
                }
                Divider()
                mapping
            }
            .padding(.trailing, DS.Space.s)
        }
        .controlSize(.small)
        .font(DS.Font.body)
    }

    // MARK: - User mapping

    private var rows: [(bundle: String, category: String)] {
        overrides.split(separator: "\n").compactMap { line in
            let parts = line.split(separator: "\t", maxSplits: 1).map(String.init)
            guard parts.count == 2, !parts[0].isEmpty else { return nil }
            return (parts[0], parts[1])
        }
    }

    private func save(_ rows: [(bundle: String, category: String)]) {
        overrides = rows.map { "\($0.bundle)\t\($0.category)" }.joined(separator: "\n")
    }

    private func setCategory(_ category: String, for bundle: String) {
        var all = rows.filter { $0.bundle != bundle }
        all.append((bundle, category))
        save(all.sorted { $0.bundle < $1.bundle })
    }

    private var mapping: some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            Text("Saját beosztás").font(DS.Font.label)
            if rows.isEmpty {
                Text("A gyakori appokat magától felismeri. Itt átsorolhatsz egy appot másik csoportba.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(rows, id: \.bundle) { row in
                HStack(spacing: DS.Space.s) {
                    Text(Self.appName(row.bundle)).lineLimit(1).truncationMode(.middle)
                        .help(row.bundle)
                    Spacer(minLength: DS.Space.s)
                    Picker("Csoport", selection: Binding(get: { row.category },
                                                         set: { setCategory($0, for: row.bundle) })) {
                        ForEach(Self.categories, id: \.name) { Text($0.label).tag($0.name) }
                    }
                    .pickerStyle(.menu).labelsHidden().fixedSize()
                    Button {
                        save(rows.filter { $0.bundle != row.bundle })
                    } label: {
                        Image(systemName: "minus.circle")
                    }
                    .buttonStyle(.borderless)
                    .help("Eltávolítás")
                }
            }
            if let app = state.previousAppIdentity, !rows.contains(where: { $0.bundle == app.bundleID }) {
                Button(String(localized: "\(app.name) hozzáadása")) {
                    let current = Core.styleCategory(bundleID: app.bundleID, windowTitle: "", overrides: "")
                    let name = Self.categories.first { $0.id == current }?.name ?? "other"
                    setCategory(name, for: app.bundleID)
                }
                .buttonStyle(.link).font(DS.Font.meta)
            }
        }
    }

    /// The app's name from its bundle id, or the id when it is not installed.
    static func appName(_ bundleID: String) -> String {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else { return bundleID }
        return FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
    }
}

/// One category: its style menu and the example in that style.
struct StyleCategoryRow: View {
    let category: Int32
    let label: String
    let language: DictationLanguage
    @AppStorage private var style: Int

    init(category: Int32, label: String, language: DictationLanguage) {
        self.category = category
        self.label = label
        self.language = language
        // Defaults as in the core (`Category::default_style`): personal casual.
        _style = AppStorage(wrappedValue: category == 1 ? 1 : 0, "style.\(category)")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            HStack(spacing: DS.Space.s) {
                Text(label).font(DS.Font.label)
                Spacer(minLength: DS.Space.s)
                if category == Self.code {
                    // The code style is fixed (core `code.rs`): no menu.
                    Text("Kódhoz igazítva").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                } else {
                    Picker(label, selection: $style) {
                        ForEach(StyleTab.styles, id: \.id) { Text($0.label).tag($0.id) }
                    }
                    .pickerStyle(.menu).labelsHidden().fixedSize()
                }
            }
            Text(example)
                .font(DS.Font.meta)
                .foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(DS.Space.s)
                .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(Color.primary.opacity(0.04)))
        }
    }

    static let code: Int32 = 4

    /// Sample cleaned text as the core would receive it, styled by the core.
    private var example: String {
        Core.applyStyle(Self.sample(category, language), category: category,
                        style: Int32(min(2, max(0, style))), language: language.code)
    }

    static func sample(_ category: Int32, _ language: DictationLanguage) -> String {
        let hu = language == .hungarian
        switch category {
        case 1: return hu ? "Hét körül ott leszek." : "I'll be there around seven."
        case 2: return hu ? "A jelentést ma délután küldöm." : "I'll send the report this afternoon."
        case 3: return hu
            ? "Kedves Anna, köszönöm a gyors választ. A szerződést holnap visszaküldöm. Üdvözlettel, Péter."
            : "Hi Alex, thanks for the quick reply. I'll send the contract back tomorrow. Best regards, Sam."
        case 4: return hu
            ? "Írd át a „Kész” feliratot, aztán futtasd a teszteket."
            : "Rename the variable to camel case user id."
        default: return hu ? "Holnap reggel átnézem a tervet." : "I'll review the plan tomorrow morning."
        }
    }
}
