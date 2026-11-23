// Drives a Tauri window on macOS with raw CGEvents.
//
// System Events' `click at` fails against a WKWebView with -25208: that path goes through the
// Accessibility API, which the web view does not implement. CGEvents posted to the HID tap go
// to whatever is under the point instead, and drive the app completely.
//
// Every point here is *window-relative*, and the window's origin is read fresh on each call.
// A CGEvent goes wherever it lands, and this is the user's own desktop — a point read off a
// full-screen capture once landed in their chat client and pulled a private channel forward.
//
// Build:  swiftc -O -o uidrive uidrive.swift

import Cocoa

let app = "mxb-app"
/// Which instance to drive. This machine routinely has two of them up — one per checkout —
/// and a CGEvent goes wherever it lands, so the window is picked by process id rather than by
/// name whenever MXB_PID is set.
let wantPid = ProcessInfo.processInfo.environment["MXB_PID"].flatMap { Int32($0) }

func frontWindow() -> CGRect? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
        return nil
    }
    for w in list {
        if let want = wantPid {
            guard let pid = w[kCGWindowOwnerPID as String] as? Int32, pid == want else { continue }
        } else {
            guard let owner = w[kCGWindowOwnerName as String] as? String, owner.lowercased().contains(app.lowercased()) else { continue }
        }
        guard let b = w[kCGWindowBounds as String] as? [String: Any],
              let x = b["X"] as? CGFloat, let y = b["Y"] as? CGFloat,
              let width = b["Width"] as? CGFloat, let height = b["Height"] as? CGFloat,
              width > 200, height > 200 else { continue }
        return CGRect(x: x, y: y, width: width, height: height)
    }
    return nil
}

func focus() {
    if let want = wantPid, let a = NSRunningApplication(processIdentifier: want) {
        a.activate(options: [.activateAllWindows])
        usleep(400_000)
        return
    }
    for a in NSWorkspace.shared.runningApplications where (a.localizedName ?? "").lowercased().contains(app.lowercased()) {
        a.activate(options: [.activateAllWindows])
    }
    usleep(400_000)
}

/// Window-relative point to screen point. Read after focusing, never cached.
func screenPoint(_ x: CGFloat, _ y: CGFloat) -> CGPoint? {
    guard let r = frontWindow() else { return nil }
    guard x >= 0, y >= 0, x <= r.width, y <= r.height else {
        FileHandle.standardError.write("point \(x),\(y) is outside the window \(r.width)x\(r.height)\n".data(using: .utf8)!)
        return nil
    }
    return CGPoint(x: r.origin.x + x, y: r.origin.y + y)
}

func post(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton = .left, clicks: Int64 = 1) {
    guard let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: button) else { return }
    e.setIntegerValueField(.mouseEventClickState, value: clicks)
    e.post(tap: .cghidEventTap)
}

func click(_ x: CGFloat, _ y: CGFloat, clicks: Int64 = 1) {
    guard let p = screenPoint(x, y) else { exit(2) }
    post(.mouseMoved, p)
    usleep(80_000)
    for _ in 0..<clicks {
        post(.leftMouseDown, p, .left, clicks: clicks)
        usleep(40_000)
        post(.leftMouseUp, p, .left, clicks: clicks)
        usleep(60_000)
    }
}

func drag(_ x1: CGFloat, _ y1: CGFloat, _ x2: CGFloat, _ y2: CGFloat) {
    guard let a = screenPoint(x1, y1), let b = screenPoint(x2, y2) else { exit(2) }
    post(.mouseMoved, a); usleep(60_000)
    post(.leftMouseDown, a); usleep(60_000)
    let steps = 24
    for i in 1...steps {
        let t = CGFloat(i) / CGFloat(steps)
        post(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
        usleep(12_000)
    }
    post(.leftMouseUp, b); usleep(60_000)
}

func scroll(_ x: CGFloat, _ y: CGFloat, _ amount: Int32) {
    guard let p = screenPoint(x, y) else { exit(2) }
    post(.mouseMoved, p); usleep(60_000)
    // Scrolled in steps: the viewer's zoom is per-event, so one large delta moves less than
    // many small ones and stops short of the dirt.
    let step: Int32 = amount > 0 ? 3 : -3
    for _ in 0..<(abs(amount) / 3) {
        if let e = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: step, wheel2: 0, wheel3: 0) {
            e.location = p
            e.post(tap: .cghidEventTap)
        }
        usleep(40_000)
    }
}

func type(_ s: String) {
    for ch in s {
        guard let d = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
              let u = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else { continue }
        var chars = Array(String(ch).utf16)
        d.keyboardSetUnicodeString(stringLength: chars.count, unicodeString: &chars)
        u.keyboardSetUnicodeString(stringLength: chars.count, unicodeString: &chars)
        d.post(tap: .cghidEventTap); usleep(20_000)
        u.post(tap: .cghidEventTap); usleep(30_000)
    }
}

func key(_ code: CGKeyCode) {
    CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)?.post(tap: .cghidEventTap)
    usleep(40_000)
    CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)?.post(tap: .cghidEventTap)
    usleep(60_000)
}

let args = CommandLine.arguments
guard args.count >= 2 else {
    print("usage: uidrive geometry|focus|click X Y|double X Y|drag X1 Y1 X2 Y2|scroll X Y N|type TEXT|esc|enter")
    exit(1)
}
let n = { (i: Int) -> CGFloat in CGFloat(Double(args[i]) ?? 0) }

switch args[1] {
case "list":
    guard let l = CGWindowListCopyWindowInfo([.excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { exit(3) }
    for w in l {
        let owner = w[kCGWindowOwnerName as String] as? String ?? "?"
        let pid = w[kCGWindowOwnerPID as String] as? Int32 ?? -1
        let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        let ww = b["Width"] as? CGFloat ?? 0, hh = b["Height"] as? CGFloat ?? 0
        let id = w[kCGWindowNumber as String] as? Int ?? -1
        let on = (w[kCGWindowIsOnscreen as String] as? Bool) ?? false
        let layer = w[kCGWindowLayer as String] as? Int ?? -99
        let xx = b["X"] as? CGFloat ?? 0, yy = b["Y"] as? CGFloat ?? 0
        if wantPid == nil || (w[kCGWindowOwnerPID as String] as? Int32) == wantPid {
            print("\(pid)\t\(id)\tlayer=\(layer)\ton=\(on)\t\(owner)\t\(Int(ww))x\(Int(hh))@\(Int(xx)),\(Int(yy))")
        }
    }
case "geometry":
    focus()
    _ = frontWindow()          // an unfocused app answers with nothing; ask twice
    guard let r = frontWindow() else { print("no window"); exit(3) }
    print("\(Int(r.origin.x)) \(Int(r.origin.y)) \(Int(r.width)) \(Int(r.height))")
case "focus": focus()
case "click": focus(); click(n(2), n(3))
case "double": focus(); click(n(2), n(3), clicks: 2)
case "drag": focus(); drag(n(2), n(3), n(4), n(5))
case "scroll": focus(); scroll(n(2), n(3), Int32(args[4]) ?? 0)
case "type": focus(); type(args[2])
case "esc": focus(); key(53)
case "enter": focus(); key(36)
default: print("unknown: \(args[1])"); exit(1)
}
