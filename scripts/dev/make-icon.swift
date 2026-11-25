// 生成 1024×1024 应用图标：蓝色圆角底 + 两个对话气泡（"听" / "译"）
// 用法：swift scripts/dev/make-icon.swift src-tauri/icons/app-icon.png
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "app-icon.png"
let size: CGFloat = 1024
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

// macOS 图标网格：内容区 824，圆角约 185
let inset: CGFloat = 100
let bgRect = NSRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
let bg = NSBezierPath(roundedRect: bgRect, xRadius: 185, yRadius: 185)
NSGradient(starting: NSColor(srgbRed: 0.27, green: 0.47, blue: 1.0, alpha: 1),
           ending: NSColor(srgbRed: 0.36, green: 0.30, blue: 0.93, alpha: 1))!.draw(in: bg, angle: -60)

func bubble(_ rect: NSRect, tailLeft: Bool, fill: NSColor) {
    let path = NSBezierPath(roundedRect: rect, xRadius: 70, yRadius: 70)
    let tail = NSBezierPath()
    if tailLeft {
        tail.move(to: NSPoint(x: rect.minX + 70, y: rect.minY + 10))
        tail.line(to: NSPoint(x: rect.minX + 40, y: rect.minY - 60))
        tail.line(to: NSPoint(x: rect.minX + 150, y: rect.minY + 10))
    } else {
        tail.move(to: NSPoint(x: rect.maxX - 70, y: rect.minY + 10))
        tail.line(to: NSPoint(x: rect.maxX - 40, y: rect.minY - 60))
        tail.line(to: NSPoint(x: rect.maxX - 150, y: rect.minY + 10))
    }
    tail.close()
    fill.setFill()
    path.fill()
    tail.fill()
}

func glyph(_ text: String, in rect: NSRect, color: NSColor, size: CGFloat) {
    let font = NSFont(name: "PingFangSC-Semibold", size: size) ?? NSFont.boldSystemFont(ofSize: size)
    let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
    let s = NSAttributedString(string: text, attributes: attrs)
    let b = s.size()
    s.draw(at: NSPoint(x: rect.midX - b.width / 2, y: rect.midY - b.height / 2 + size * 0.02))
}

// 后面的气泡：半透明白，写"译"
let back = NSRect(x: 455, y: 250, width: 380, height: 330)
bubble(back, tailLeft: false, fill: NSColor(white: 1, alpha: 0.35))
glyph("译", in: back, color: .white, size: 210)

// 前面的气泡：白色，写"听"
let front = NSRect(x: 190, y: 470, width: 420, height: 360)
bubble(front, tailLeft: true, fill: .white)
glyph("听", in: front, color: NSColor(srgbRed: 0.27, green: 0.40, blue: 0.98, alpha: 1), size: 240)

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
