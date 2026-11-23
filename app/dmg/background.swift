// Disk image background for scripts/make-dmg.sh: a plain, neutral placeholder.
// This file is the one place the look lives; replace it (or pass PUSMOGO_DMG_BACKGROUND
// to make-dmg.sh) to change the background. Geometry must match the icon positions
// in make-dmg.sh: window 640 × 428 points, app at (160, 200), Applications at (480, 200).
//
//   swift app/dmg/background.swift <out-dir>   → background.png (1x) and background@2x.png
import AppKit

let width: CGFloat = 640, height: CGFloat = 428
let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let azure = NSColor(srgbRed: 0x1f / 255, green: 0x6b / 255, blue: 0xff / 255, alpha: 1)
let ink = NSColor(srgbRed: 0x14 / 255, green: 0x11 / 255, blue: 0x0c / 255, alpha: 1)
let quiet = NSColor(srgbRed: 0x6b / 255, green: 0x67 / 255, blue: 0x60 / 255, alpha: 1)

/// The mark from docs/assets/logo.svg (840 × 840 units), drawn at `rect` (flipped y).
func drawMark(in rect: CGRect) {
    let s = rect.width / 840
    func r(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect {
        CGRect(x: rect.minX + x * s, y: rect.minY + y * s, width: w * s, height: h * s)
    }
    azure.setFill()
    for (x, y, h) in [(140.0, 270.0, 300.0), (250, 185, 470), (360, 120, 600), (470, 185, 470), (580, 270, 300)] {
        NSBezierPath(roundedRect: r(x, y, 120, h), xRadius: 60 * s, yRadius: 60 * s).fill()
    }
    ink.setFill()
    NSBezierPath(ovalIn: r(314, 350, 80, 120)).fill()
    NSBezierPath(ovalIn: r(446, 350, 80, 120)).fill()
}

func text(_ string: String, size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSAttributedString {
    NSAttributedString(string: string, attributes: [
        .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color,
    ])
}

func render(scale: CGFloat, to path: String) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: width, height: height)  // 72 dpi at 1x, 144 dpi at 2x
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    // Top-left origin, the same as Finder's icon coordinates.
    context.cgContext.translateBy(x: 0, y: height)
    context.cgContext.scaleBy(x: 1, y: -1)
    let flipped = NSGraphicsContext(cgContext: context.cgContext, flipped: true)
    NSGraphicsContext.current = flipped

    NSColor.white.setFill()
    CGRect(x: 0, y: 0, width: width, height: height).fill()

    // Small logo and name, top-left.
    drawMark(in: CGRect(x: 24, y: 22, width: 32, height: 32))
    text("Pusmogo", size: 14, weight: .semibold, color: ink).draw(at: CGPoint(x: 62, y: 22))
    text("by vatson.ai", size: 11, weight: .regular, color: quiet).draw(at: CGPoint(x: 62, y: 40))

    // One thin arrow between the two icons (icons are 128 pt, centred at y 200).
    quiet.setStroke()
    let arrow = NSBezierPath()
    arrow.lineWidth = 1.5
    arrow.lineCapStyle = .round
    arrow.lineJoinStyle = .round
    arrow.move(to: CGPoint(x: 256, y: 200))
    arrow.line(to: CGPoint(x: 384, y: 200))
    arrow.move(to: CGPoint(x: 374, y: 192))
    arrow.line(to: CGPoint(x: 384, y: 200))
    arrow.line(to: CGPoint(x: 374, y: 208))
    arrow.stroke()

    // The bilingual hint, centred under the icons and their labels.
    let hint = text("Húzd az Alkalmazások mappába · Drag to Applications", size: 13, weight: .regular, color: quiet)
    hint.draw(at: CGPoint(x: (width - hint.size().width) / 2, y: 340))

    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

render(scale: 1, to: "\(outDir)/background.png")
render(scale: 2, to: "\(outDir)/background@2x.png")
