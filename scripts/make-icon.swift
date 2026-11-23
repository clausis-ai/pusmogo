// Draws the Pusmogo app icon from the mark in docs/assets/logo.svg (five sound-bar
// bars fused into one body, two ink eyes) and writes:
//   app/AppIcon.icns                       white body + ink eyes on the azure tile
//   app/AppIcon.icon/Assets/body.svg       macOS 26 Icon Composer layers (same shape,
//   app/AppIcon.icon/Assets/eyes.svg       full-bleed canvas; icon.json holds the fill)
// No image assets beyond these: the icon is code.
//   swift scripts/make-icon.swift            writes the files above
//   swift scripts/make-icon.swift out.png    also writes a 1024 px preview
import AppKit

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let accent = NSColor(srgbRed: 0x1F / 255, green: 0x6B / 255, blue: 0xFF / 255, alpha: 1)
let deep = NSColor(srgbRed: 0x14 / 255, green: 0x4F / 255, blue: 0xD6 / 255, alpha: 1)
let ink = NSColor(srgbRed: 0x14 / 255, green: 0x11 / 255, blue: 0x0C / 255, alpha: 1)

// The mark in its own 840-unit space (y down), as in docs/assets/logo.svg.
// Bars are 120 wide on a 110 step, so neighbours overlap and read as one body.
let bars: [(x: CGFloat, y: CGFloat, h: CGFloat)] = [
    (140, 270, 300), (250, 185, 470), (360, 120, 600), (470, 185, 470), (580, 270, 300),
]
let barWidth: CGFloat = 120
let eyes: [(cx: CGFloat, cy: CGFloat)] = [(354, 410), (486, 410)]
let eyeRx: CGFloat = 40, eyeRy: CGFloat = 60
let markCentre: CGFloat = 420, markHeight: CGFloat = 600

/// Draws the mark centred in `box` (AppKit coordinates, y up), its body
/// `fraction` of the box height.
func drawMark(in box: NSRect, fraction: CGFloat) {
    let k = box.height * fraction / markHeight
    func pt(_ x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: box.midX + (x - markCentre) * k, y: box.midY - (y - markCentre) * k)
    }
    NSColor.white.setFill()
    for b in bars {
        let o = pt(b.x, b.y + b.h)
        let r = NSRect(x: o.x, y: o.y, width: barWidth * k, height: b.h * k)
        NSBezierPath(roundedRect: r, xRadius: barWidth * k / 2, yRadius: barWidth * k / 2).fill()
    }
    ink.setFill()
    for e in eyes {
        let c = pt(e.cx, e.cy)
        NSBezierPath(ovalIn: NSRect(x: c.x - eyeRx * k, y: c.y - eyeRy * k,
                                    width: 2 * eyeRx * k, height: 2 * eyeRy * k)).fill()
    }
}

func render(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = CGFloat(px)
    // macOS icon grid: an 824/1024 rounded square, centred, corner radius ~185/1024.
    let inset = s * 100 / 1024
    let tile = NSRect(x: inset, y: inset, width: s - 2 * inset, height: s - 2 * inset)
    let path = NSBezierPath(roundedRect: tile, xRadius: s * 185 / 1024, yRadius: s * 185 / 1024)
    NSGradient(starting: accent, ending: deep)!.draw(in: path, angle: -90)
    // Soft radial highlight, upper centre, so the flat tile has some light on it.
    NSGraphicsContext.saveGraphicsState()
    path.addClip()
    let glow = NSGradient(starting: NSColor.white.withAlphaComponent(0.22),
                          ending: NSColor.white.withAlphaComponent(0))!
    glow.draw(fromCenter: NSPoint(x: tile.midX, y: tile.maxY - tile.height * 0.18), radius: 0,
              toCenter: NSPoint(x: tile.midX, y: tile.maxY - tile.height * 0.18),
              radius: tile.height * 0.75, options: [])
    NSGraphicsContext.restoreGraphicsState()
    drawMark(in: tile, fraction: 0.72)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

/// Icon Composer layer: the mark on a 1024 full-bleed canvas, body 72 % of it.
func layerSVG(eyesOnly: Bool) -> String {
    let k = 1024 * 0.72 / markHeight
    func f(_ v: CGFloat) -> String { String(format: "%.1f", v) }
    func x(_ v: CGFloat) -> CGFloat { 512 + (v - markCentre) * k }
    var shapes: [String]
    if eyesOnly {
        shapes = eyes.map {
            "  <ellipse cx=\"\(f(x($0.cx)))\" cy=\"\(f(x($0.cy)))\" rx=\"\(f(eyeRx * k))\" ry=\"\(f(eyeRy * k))\" fill=\"#14110C\"/>"
        }
    } else {
        shapes = bars.map {
            "  <rect x=\"\(f(x($0.x)))\" y=\"\(f(x($0.y)))\" width=\"\(f(barWidth * k))\" height=\"\(f($0.h * k))\" rx=\"\(f(barWidth * k / 2))\" fill=\"#FFFFFF\"/>"
        }
    }
    return """
    <svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
    \(shapes.joined(separator: "\n"))
    </svg>

    """
}

let fm = FileManager.default
let iconset = fm.temporaryDirectory.appendingPathComponent("Pusmogo.iconset")
try? fm.removeItem(at: iconset)
try! fm.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try! render(base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try! render(base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let out = root.appendingPathComponent("app/AppIcon.icns")
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
p.arguments = ["-c", "icns", iconset.path, "-o", out.path]
try! p.run(); p.waitUntilExit()
guard p.terminationStatus == 0 else { fatalError("iconutil failed") }
let assets = root.appendingPathComponent("app/AppIcon.icon/Assets")
try! fm.createDirectory(at: assets, withIntermediateDirectories: true)
try! layerSVG(eyesOnly: false).write(to: assets.appendingPathComponent("body.svg"), atomically: true, encoding: .utf8)
try! layerSVG(eyesOnly: true).write(to: assets.appendingPathComponent("eyes.svg"), atomically: true, encoding: .utf8)
if CommandLine.arguments.count > 1 {
    try! render(1024).write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
}
print("wrote \(out.path) and \(assets.path)/{body,eyes}.svg")
