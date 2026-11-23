import SwiftUI

/// Settings → AI apps: one row per AI app (Claude Desktop, Claude Code,
/// Cursor) that registers the bundled `pusmogo-mcp` in that app's MCP config.
/// Adding first shows the exact change as a diff and saves only after the
/// user confirms; the core backs the file up and touches only our entry.
/// Undo removes the entry again. Every file operation is a small, synchronous
/// core call (`mcp_config.rs`).
struct McpClientsView: View {
    @State private var status: [Core.McpClient: Int32] = [:]
    /// The client whose diff is open, and the diff itself.
    @State private var pending: (client: Core.McpClient, diff: String)?
    @State private var message: String?

    private var serverBundled: Bool {
        FileManager.default.isExecutableFile(atPath: Core.mcpCommand)
    }

    /// A config pointing into a build folder or a disk image breaks when that
    /// copy goes away, so say so when this copy is not in an Applications folder.
    private var outsideApplications: Bool {
        let parent = Bundle.main.bundleURL.deletingLastPathComponent().lastPathComponent
        return parent != "Applications"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            ForEach(Core.McpClient.allCases) { client in
                row(client)
                if let pending, pending.client == client {
                    diffBox(pending.diff, client: client)
                }
            }
            if !serverBundled {
                note("Ebben a buildben nincs MCP-szerver.")
            } else if outsideApplications {
                note("Ez a példány nem az Alkalmazások mappából fut: a beállítás erre a helyre mutat majd.")
            }
            if let message { plainNote(message) }
        }
        .onAppear(perform: refresh)
    }

    private func row(_ client: Core.McpClient) -> some View {
        HStack(spacing: DS.Space.s) {
            Text(verbatim: client.name)
            Spacer(minLength: DS.Space.s)
            switch status[client] ?? 0 {
            case 1:
                Image(systemName: "checkmark").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .accessibilityLabel(Text("Hozzáadva"))
                Button("Visszavonás") { undo(client) }.controlSize(.small)
            case 2:
                // Registered for a copy that was moved or deleted: the app can
                // no longer start the server, so offer to point it here.
                Text("Elavult").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                    .help(Text("A beállítás egy áthelyezett vagy törölt Pusmogóra mutat."))
                Button("Frissítés") { preview(client) }
                    .controlSize(.small)
                    .disabled(!serverBundled || pending != nil)
                Button("Visszavonás") { undo(client) }.controlSize(.small)
            case -2, -3:
                Text("A beállításfájl nem olvasható").font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            default:
                Button("Hozzáadás") { preview(client) }
                    .controlSize(.small)
                    .disabled(!serverBundled || pending != nil)
            }
        }
    }

    private func diffBox(_ diff: String, client: Core.McpClient) -> some View {
        VStack(alignment: .leading, spacing: DS.Space.xs) {
            Text(verbatim: Core.mcpConfigPath(client).map(abbreviated) ?? "")
                .font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
                .lineLimit(1).truncationMode(.middle)
            ScrollView {
                Text(verbatim: diff)
                    .font(DS.Font.mono)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
            }
            .frame(height: 110)
            .padding(DS.Space.xs)
            .background(RoundedRectangle(cornerRadius: DS.Radius.control).fill(DS.Color.quiet.opacity(0.08)))
            HStack(spacing: DS.Space.s) {
                Spacer()
                Button("Mégse") { pending = nil }.controlSize(.small)
                Button("Mentés") { install(client) }.controlSize(.small).keyboardShortcut(.defaultAction)
            }
        }
    }

    private func note(_ text: LocalizedStringKey) -> some View {
        Text(text).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func plainNote(_ text: String) -> some View {
        Text(verbatim: text).font(DS.Font.meta).foregroundStyle(DS.Color.quiet)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func abbreviated(_ path: String) -> String {
        let home = Core.mcpHome
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }

    private func refresh() {
        for client in Core.McpClient.allCases { status[client] = Core.mcpInstalled(client) }
        // Test hook: `PUSMOGO_PREVIEW_MCP_DIFF=<client id>` opens that client's
        // diff on appear, for a screenshot (pair with a scratch `PUSMOGO_MCP_HOME`).
        if pending == nil, let id = ProcessInfo.processInfo.environment["PUSMOGO_PREVIEW_MCP_DIFF"],
           let client = Core.McpClient(rawValue: id) {
            preview(client)
        }
    }

    private func preview(_ client: Core.McpClient) {
        message = nil
        guard let diff = Core.mcpPreview(client) else {
            message = String(localized: "A beállításfájl nem olvasható, ezért nem módosítottam.")
            return refresh()
        }
        if diff.isEmpty { return refresh() }
        pending = (client, diff)
    }

    private func install(_ client: Core.McpClient) {
        pending = nil
        let code = Core.mcpInstall(client)
        Timings.note("mcp: install \(client.rawValue) \(code)")
        message = code >= 0
            ? String(localized: "Hozzáadva. Indítsd újra: \(client.name).")
            : String(localized: "Nem sikerült menteni. A fájl változatlan.")
        status[client] = Core.mcpInstalled(client)
    }

    private func undo(_ client: Core.McpClient) {
        let code = Core.mcpUninstall(client)
        Timings.note("mcp: uninstall \(client.rawValue) \(code)")
        message = code >= 0
            ? String(localized: "Eltávolítva. Indítsd újra: \(client.name).")
            : String(localized: "Nem sikerült menteni. A fájl változatlan.")
        status[client] = Core.mcpInstalled(client)
    }
}
