import CoreGraphics
import Foundation
// Prints the number of the largest ordinary window the given process owns.
let pid = Int32(CommandLine.arguments[1]) ?? 0
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
func area(_ w: [String: Any]) -> Double {
    let r = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
    return (r["Width"] ?? 0) * (r["Height"] ?? 0)
}
if let best = mine.max(by: { area($0) < area($1) }), let id = best[kCGWindowNumber as String] as? Int { print(id) }
