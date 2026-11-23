import SwiftUI

@main
struct PusmogoApp: App {
    @StateObject private var state = AppState()

    init() {
        SingleInstance.quitIfAnotherCopyRuns()
        AppMover.offerIfNeeded()
        Updater.shared.start()
    }

    var body: some Scene {
        MenuBarExtra {
            PanelView(state: state)
        } label: {
            MenuBarIcon(phase: state.phase, level: state.levels.last ?? 0)
        }
        .menuBarExtraStyle(.window)
    }
}

/// The menu bar icon: five bars (the Pusmogo mark) that come alive while you talk.
struct MenuBarIcon: View {
    let phase: Phase
    let level: Float

    var body: some View {
        switch phase {
        case .thinking, .loading:
            Image(systemName: "ellipsis")
        case .message:
            Image(systemName: "exclamationmark.triangle")
        case .info:
            Image(nsImage: Self.bars(level: nil))
        default:
            Image(nsImage: Self.bars(level: phase == .idle || phase == .done ? nil : CGFloat(level)))
        }
    }

    /// The Pusmogo mark (`docs/assets/logo.svg`) as a template image, so macOS tints it for
    /// light and dark menu bars: five bars fused into one body, the two eyes cut out.
    /// While listening the bars follow the microphone level; the eyes stay put, and the
    /// three middle bars never shrink below them.
    static func bars(level: CGFloat?) -> NSImage {
        // Logo geometry, 840-unit viewBox: bars 120 wide on a 110 pitch (they overlap and
        // fuse), heights 300/470/600, centred on y = 420; eyes 80 × 120 at x = 354 and 486,
        // y = 410. Scaled so the tallest bar is 15 pt.
        let u: CGFloat = 15.0 / 600
        let base: [CGFloat] = [300, 470, 600, 470, 300]
        let image = NSImage(size: NSSize(width: 18, height: 16), flipped: false) { rect in
            let midX = rect.midX, midY = rect.midY
            let w = 120 * u, pitch = 110 * u
            let scale = level.map { 0.3 + 0.7 * min(1, $0 * 1.4) } ?? 1
            let body = NSBezierPath()
            for (i, b) in base.enumerated() {
                let minH: CGFloat = (1...3).contains(i) ? 8.5 : w
                let h = max(minH, b * u * scale)
                let x = midX + CGFloat(i - 2) * pitch - w / 2
                let bar = NSRect(x: x, y: midY - h / 2, width: w, height: h)
                body.append(NSBezierPath(roundedRect: bar, xRadius: w / 2, yRadius: w / 2))
            }
            NSColor.black.setFill()
            body.fill()
            // Eyes a touch larger than the logo's proportion so they survive 1x screens.
            NSGraphicsContext.current?.compositingOperation = .destinationOut
            let ew: CGFloat = 2.2, eh: CGFloat = 3.2, eyeY = midY + 10 * u
            for dx in [-66 * u, 66 * u] {
                NSBezierPath(ovalIn: NSRect(x: midX + dx - ew / 2, y: eyeY - eh / 2,
                                            width: ew, height: eh)).fill()
            }
            return true
        }
        image.isTemplate = true
        return image
    }
}
