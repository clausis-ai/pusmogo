import SwiftUI

/// Pusmogo design tokens. Native Apple HIG (SF Pro, SF Symbols, system materials,
/// Liquid Glass on macOS 26) with a thin brand layer from vatson.ai:
/// one azure accent, Vatson ink colours, the 8/10/14/18/24 radius scale, hairlines.
/// One radius per level: inside a 24 stage only 10 or 14, never 18 in 18.
/// Hairlines are black (or white on dark) at 6–8 %, 15 % on hover; no 2 pt borders.
/// Copy: Hungarian, informal "te", no exclamation marks, no em dashes.
enum DS {
    enum Color {
        /// The one accent (Vatson azure). Used sparingly: hands-free state, primary button.
        static let accent = SwiftUI.Color(red: 0x1F / 255, green: 0x6B / 255, blue: 0xFF / 255)
        static let ink = SwiftUI.Color(red: 0x1D / 255, green: 0x1D / 255, blue: 0x1F / 255)
        static let quiet = SwiftUI.Color.secondary
        /// Flow bar surface: near-black so white bars read on any wallpaper or app.
        static let barFill = SwiftUI.Color(white: 0.06).opacity(0.94)
        static let barStroke = SwiftUI.Color.white.opacity(0.16)
        static let barInk = SwiftUI.Color.white
        static let warning = SwiftUI.Color(red: 1.0, green: 0.62, blue: 0.2)
    }

    enum Radius {
        static let control: CGFloat = 8   // buttons, inputs, chips (incl. shortcut keys), tabs
        static let small: CGFloat = 10    // list rows, small cards
        static let card: CGFloat = 14     // cards
        static let panel: CGFloat = 18    // panels
        static let stage: CGFloat = 24    // big coloured stage plates (onboarding)
        static let key: CGFloat = 4       // tiny key caps in bar tooltips
    }

    enum Space {
        static let xs: CGFloat = 4
        static let s: CGFloat = 8
        static let m: CGFloat = 12
        static let l: CGFloat = 16
        static let xl: CGFloat = 24
    }

    enum Font {
        static let title = SwiftUI.Font.system(size: 15, weight: .semibold)
        static let body = SwiftUI.Font.system(size: 13)
        static let label = SwiftUI.Font.system(size: 12, weight: .medium)
        static let meta = SwiftUI.Font.system(size: 11)
        static let mono = SwiftUI.Font.system(size: 11, design: .monospaced)
    }

    enum Motion {
        /// The flow bar morph: quick, slightly springy, never bouncy.
        static let morph = Animation.spring(response: 0.30, dampingFraction: 0.82)
        static let fade = Animation.easeOut(duration: 0.18)
    }

    /// Flow bar geometry per state (width × height, points).
    enum Bar {
        static let idle = CGSize(width: 40, height: 9)
        static let listening = CGSize(width: 150, height: 32)
        static let locked = CGSize(width: 198, height: 32)
        static let thinking = CGSize(width: 92, height: 32)
        static let thinkingTagged = CGSize(width: 116, height: 32)
        static let done = CGSize(width: 32, height: 32)
        static let message = CGSize(width: 300, height: 32)
        static let bottomInset: CGFloat = 10
    }
}

extension View {
    /// Dark glass capsule for the flow bar: Liquid Glass on macOS 26+, a solid
    /// near-black fill below (contrast first).
    func flowBarSurface() -> some View {
        flowBarSurface(in: Capsule())
    }

    /// The same surface in a rounded rectangle, for multi-line text.
    func flowBarSurface(cornerRadius: CGFloat) -> some View {
        flowBarSurface(in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }

    @ViewBuilder
    private func flowBarSurface<S: InsettableShape>(in shape: S) -> some View {
        if #available(macOS 26.0, *) {
            self
                .background(shape.fill(DS.Color.barFill))
                .glassEffect(.regular.tint(.black.opacity(0.55)), in: shape)
                .overlay(shape.strokeBorder(DS.Color.barStroke, lineWidth: 0.5))
        } else {
            self
                .background(shape.fill(DS.Color.barFill))
                .overlay(shape.strokeBorder(DS.Color.barStroke, lineWidth: 0.5))
                .shadow(color: .black.opacity(0.25), radius: 8, y: 2)
        }
    }
}
