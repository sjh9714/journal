// guarded_drag srcX srcY dstX dstY targetPid
// Drags only while Finder owns the topmost window at src and targetPid at dst.
import CoreGraphics
import Foundation

let a = CommandLine.arguments
let src = CGPoint(x: Double(a[1])!, y: Double(a[2])!), dst = CGPoint(x: Double(a[3])!, y: Double(a[4])!)
let target = Int(a[5])!

func topOwner(_ p: CGPoint) -> (pid: Int, name: String) {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
  for w in list where (w[kCGWindowLayer as String] as? Int) == 0 {
    guard let b = w[kCGWindowBounds as String] as? [String: Double],
          CGRect(x: b["X"]!, y: b["Y"]!, width: b["Width"]!, height: b["Height"]!).contains(p) else { continue }
    return (w[kCGWindowOwnerPID as String] as? Int ?? -1, w[kCGWindowOwnerName as String] as? String ?? "?")
  }
  return (-1, "none")
}
func post(_ t: CGEventType, _ p: CGPoint) {
  CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
}
func path(_ from: CGPoint, _ to: CGPoint, steps: Int = 25) {
  for i in 1...steps {
    let f = Double(i) / Double(steps)
    post(.leftMouseDragged, CGPoint(x: from.x + (to.x - from.x) * f, y: from.y + (to.y - from.y) * f)); usleep(25_000)
  }
}

let s = topOwner(src), d = topOwner(dst)
print("src top: \(s.name) dst top: \(d.name)(\(d.pid))")
guard s.name == "Finder", d.pid == target else { print("ABORT: wrong windows on top"); exit(2) }

post(.mouseMoved, src); usleep(150_000)
post(.leftMouseDown, src); usleep(250_000)
path(src, CGPoint(x: src.x + 12, y: src.y + 4), steps: 4)
path(CGPoint(x: src.x + 12, y: src.y + 4), dst)
usleep(400_000)
let d2 = topOwner(dst)
if d2.pid != target {
  print("ABORT mid-drag: \(d2.name) covered the drop point; returning to source")
  path(dst, src); usleep(200_000); post(.leftMouseUp, src); exit(3)
}
post(.leftMouseUp, dst)
print("dropped")
