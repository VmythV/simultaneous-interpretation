// 生成菜单栏模板图标（黑色 + 透明，44×44 = 22pt@2x）
// 用法：swift scripts/dev/make-tray-icon.swift src-tauri/icons
import AppKit

let dir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let px = 44

func render(_ name: String, filled: Bool) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSColor.black.set()

    // 前面的气泡（左上）
    let front = NSBezierPath(roundedRect: NSRect(x: 4, y: 17, width: 24, height: 19), xRadius: 6, yRadius: 6)
    front.move(to: NSPoint(x: 9, y: 18))
    front.line(to: NSPoint(x: 6, y: 11))
    front.line(to: NSPoint(x: 15, y: 18))
    // 后面的气泡（右下）
    let back = NSBezierPath(roundedRect: NSRect(x: 20, y: 7, width: 20, height: 16), xRadius: 5, yRadius: 5)
    back.move(to: NSPoint(x: 35, y: 8))
    back.line(to: NSPoint(x: 38, y: 2))
    back.line(to: NSPoint(x: 30, y: 8))

    if filled {
        back.fill()
        // 前气泡用"挖空边框"区分两个气泡
        NSGraphicsContext.current?.compositingOperation = .clear
        NSBezierPath(roundedRect: NSRect(x: 2, y: 15, width: 28, height: 23), xRadius: 8, yRadius: 8).fill()
        NSGraphicsContext.current?.compositingOperation = .sourceOver
        front.fill()
    } else {
        front.lineWidth = 2.6
        back.lineWidth = 2.6
        back.stroke()
        NSGraphicsContext.current?.compositingOperation = .clear
        NSBezierPath(roundedRect: NSRect(x: 3, y: 16, width: 26, height: 21), xRadius: 7, yRadius: 7).fill()
        NSGraphicsContext.current?.compositingOperation = .sourceOver
        front.stroke()
    }
    NSGraphicsContext.restoreGraphicsState()
    let url = URL(fileURLWithPath: dir).appendingPathComponent(name)
    try! rep.representation(using: .png, properties: [:])!.write(to: url)
    print("wrote \(url.path)")
}

render("tray.png", filled: false)
render("tray-active.png", filled: true)
