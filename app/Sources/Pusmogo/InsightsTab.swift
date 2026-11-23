import AppKit
import SwiftUI

/// The Insights tab: numbers the core computes from the local profile
/// (`Core.insights`), plus a playful title picked by the core's rules.
/// Read fresh each time the tab opens; nothing leaves the Mac.
struct InsightsTab: View {
    @ObservedObject var state: AppState
    @State private var insights: Core.Insights?
    @State private var loaded = false

    static let maxHeight: CGFloat = 440

    var body: some View {
        FittedScroll(maxHeight: Self.maxHeight) {
            VStack(alignment: .leading, spacing: DS.Space.m) {
                if let i = insights, i.dictations > 0 {
                    titleCard(i)
                    tiles(i)
                    details(i)
                } else if loaded {
                    Text("Még nincs mit mutatni. Diktálj néhányat, és itt látod, hogyan használod a Pusmogót.")
                        .font(DS.Font.body).foregroundStyle(DS.Color.quiet)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Text("Minden adat ezen a Macen marad.")
                    .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            }
            .padding(.trailing, DS.Space.s)
        }
        .font(DS.Font.body)
        .onAppear {
            state.loadInsights { value in
                insights = value
                loaded = true
            }
        }
    }

    // MARK: - Parts

    private func titleCard(_ i: Core.Insights) -> some View {
        let t = Self.title(i.title)
        return VStack(alignment: .leading, spacing: DS.Space.xs) {
            Text("A hangprofilod").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            Text(t.name).font(DS.Font.title)
            Text(t.blurb).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(DS.Space.m)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.accent.opacity(0.08)))
    }

    private func tiles(_ i: Core.Insights) -> some View {
        Grid(horizontalSpacing: DS.Space.s, verticalSpacing: DS.Space.s) {
            GridRow {
                tile(i.words.formatted(), String(localized: "szó összesen"))
                tile(i.wordsPerMinute.map { $0.formatted() } ?? "–", String(localized: "szó percenként"))
            }
            GridRow {
                tile(i.currentStreak.formatted(), String(localized: "nap egymás után"))
                tile(i.longestStreak.formatted(), String(localized: "nap, a leghosszabb sorozat"))
            }
        }
    }

    private func tile(_ value: String, _ caption: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value).font(.system(size: 20, weight: .semibold)).monospacedDigit()
            Text(caption).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .lineLimit(1).minimumScaleFactor(0.85)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(DS.Space.s)
        .background(RoundedRectangle(cornerRadius: DS.Radius.small).fill(DS.Color.quiet.opacity(0.08)))
    }

    private func details(_ i: Core.Insights) -> some View {
        VStack(alignment: .leading, spacing: DS.Space.s) {
            row(String(localized: "Diktálások"), i.dictations.formatted())
            row(String(localized: "Leggyakoribb szó"), i.topWord.map { "\($0.text) · \($0.count)×" })
            row(String(localized: "Legtöbbet javított szó"), i.topCorrection.map { "\($0.text) · \($0.count)×" })
            row(String(localized: "Legaktívabb órád"), i.peakHour.map(Self.hourRange))
            row(String(localized: "Ahová a legtöbbet diktálsz"), i.topApp.map { Self.appName($0.text) })
        }
    }

    private func row(_ label: String, _ value: String?) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label).foregroundStyle(DS.Color.quiet)
            Spacer(minLength: DS.Space.s)
            Text(value ?? "–").lineLimit(1).truncationMode(.middle)
        }
    }

    // MARK: - Wording

    static func hourRange(_ h: Int) -> String {
        String(format: "%02d:00–%02d:00", h, (h + 1) % 24)
    }

    /// Bundle id → the app's name on this Mac, else the id itself.
    static func appName(_ bundle: String) -> String {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) else { return bundle }
        return FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
    }

    /// Name and one line for each core title key (`insights.rs` `Title::key`).
    static func title(_ key: String) -> (name: String, blurb: String) {
        switch key {
        case "thinker":
            return (String(localized: "Gondolkodó"),
                    String(localized: "Hosszan, összefüggően mondod el, amit gondolsz."))
        case "quick_replier":
            return (String(localized: "Gyors válaszoló"),
                    String(localized: "Rövid, pörgős üzenetekben diktálsz."))
        case "night_owl":
            return (String(localized: "Éjszakai bagoly"),
                    String(localized: "Akkor is diktálsz, amikor mások már alszanak."))
        case "early_bird":
            return (String(localized: "Korán kelő"),
                    String(localized: "A nap első óráiban vagy a legaktívabb."))
        case "steady":
            return (String(localized: "Kitartó"),
                    String(localized: "Napról napra visszatérsz, legalább egy hétig egyhuzamban."))
        case "storyteller":
            return (String(localized: "Mesélő"),
                    String(localized: "Kiegyensúlyozottan, történetszerűen diktálsz."))
        default:
            return (String(localized: "Új hang"),
                    String(localized: "Most ismerkedünk. Diktálj tovább, és kirajzolódik a profilod."))
        }
    }
}
