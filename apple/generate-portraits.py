#!/usr/bin/env python3
"""Writes the camera card's children (art/portraits.json) as Swift.

    python3 apple/generate-portraits.py

The JSON at the top of the repository is what every client draws
(art/README.md). The Apple apps carry it compiled in rather than as a
bundled resource, so that nothing can be missing from a bundle at run time;
PortraitArtTests read the JSON again and fail while this output is stale.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(HERE, "..", "art", "portraits.json")
TARGET = os.path.join(HERE, "TomoshibiKit", "Sources", "TomoshibiBasics", "PortraitArt.swift")


def number(value):
    # The JSON's own spelling of the number: Swift reads it to the same
    # Double the tests read from the JSON.
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        sys.exit(f"not a number: {value!r}")
    return repr(float(value)) if isinstance(value, float) else str(value)


def string(value):
    if not isinstance(value, str) or any(ch in value for ch in '"\\\n'):
        sys.exit(f"not a plain string: {value!r}")
    return f'"{value}"'


def main():
    with open(SOURCE) as f:
        art = json.load(f)
    if art.get("format") != 1:
        sys.exit("art/portraits.json: format is not 1")
    lines = [
        "// Generated from art/portraits.json by apple/generate-portraits.py. Do not",
        "// edit: change the JSON and run the script (art/README.md).",
        "",
        "import CoreGraphics",
        "",
        "/// The camera card's children as every client draws them.",
        "public enum PortraitArt {",
        f"    public static let format = {int(art['format'])}",
        f"    public static let canvas = CGSize(width: {number(art['canvas']['width'])}, height: {number(art['canvas']['height'])})",
        f"    public static let lineWidth: CGFloat = {number(art['lineWidth'])}",
        f"    public static let placement = {string(art['placement'])}",
        f"    public static let gap: Double = {number(art['gap'])}",
        "",
        "    public static let portraits: [Portrait] = [",
    ]
    for person in art["portraits"]:
        lines.append(f"        Portrait(name: {string(person['name'])}, hold: {number(person['hold'])}, fade: {number(person['fade'])}, strokes: [")
        for stroke in person["strokes"]:
            lines.append(f"            .init(d: {string(stroke['d'])}, start: {number(stroke['start'])}, duration: {number(stroke['duration'])}),")
        lines.append("        ]),")
    lines += ["    ]", "}", ""]
    with open(TARGET, "w") as f:
        f.write("\n".join(lines))


if __name__ == "__main__":
    main()
