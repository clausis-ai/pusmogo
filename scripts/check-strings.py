#!/usr/bin/env python3
"""Check that every Hungarian UI string in the app has an English translation.

The Swift code uses the Hungarian text as the localisation key and
app/Resources/en.lproj/Localizable.strings maps it to English. A literal that is
missing from that file shows up in Hungarian in an English UI, and nothing else
catches it. This script finds string literals passed to localising initialisers
(Text, Button, Label, Toggle, Section, Picker, TextField, LocalizedStringKey, .help,
.accessibilityLabel, .navigationTitle, String(localized:)) under
app/Sources/Pusmogo and checks each one against the keys.

Interpolations `\\(...)` match any format specifier (%@, %lld, %.0f ...), `%%` in
a key matches `%`, and `\\u{00A0}` matches a no-break space.

Exit 0: every literal has a key. Exit 1: missing keys are listed.
With --verbose, keys with no matching literal are listed too (strings passed
through custom views or built at run time, or stale keys); they never fail.

Usage: scripts/check-strings.py [--verbose]
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ROOT / "app" / "Sources" / "Pusmogo"
STRINGS = ROOT / "app" / "Resources" / "en.lproj" / "Localizable.strings"

# Literals that are the same in every language (brand names, sample text).
SKIP = {
    "Pusmogo",
    "Pusmogo 0.1 · vatson.ai",
    "· \x00 ms",  # unit only, the same in both languages
}

CALL = re.compile(
    r'(?:\b(?:Text|Button|Label|Toggle|Section|Picker|TextField|LocalizedStringKey)\(\s*'
    r'|\.(?:help|accessibilityLabel|navigationTitle)\(\s*'
    r'|\bString\(\s*localized:\s*)"'
)

PLACEHOLDER = "\x00"
FORMAT_SPEC = re.compile(r"%(?:\d+\$)?[-+ #0]*\d*(?:\.\d+)?(?:ll|l|h)?[@dDiuUxXoOfeEgGcCsSaA]")


def read_literal(src: str, start: int) -> tuple[str, int]:
    """Read a Swift string literal body starting just after its opening quote.

    Returns the normalised text and the index after the closing quote.
    Interpolations become PLACEHOLDER; escapes are decoded.
    """
    out = []
    i = start
    while i < len(src):
        c = src[i]
        if c == '"':
            return "".join(out), i + 1
        if c == "\n":
            raise ValueError("unterminated literal")
        if c != "\\":
            out.append(c)
            i += 1
            continue
        nxt = src[i + 1]
        if nxt == "(":
            depth, j = 1, i + 2
            while depth:
                if src[j] == "(":
                    depth += 1
                elif src[j] == ")":
                    depth -= 1
                elif src[j] == '"':
                    # A string literal inside the interpolation: skip it.
                    _, j = read_literal(src, j + 1)
                    continue
                j += 1
            out.append(PLACEHOLDER)
            i = j
        elif nxt == "u":
            close = src.index("}", i)
            out.append(chr(int(src[i + 3 : close], 16)))
            i = close + 1
        else:
            out.append({"n": "\n", "t": "\t", "0": "\0"}.get(nxt, nxt))
            i += 2
    raise ValueError("unterminated literal")


def normalise_key(key: str) -> str:
    key = key.replace("%%", "\x01")
    key = FORMAT_SPEC.sub(PLACEHOLDER, key)
    return key.replace("\x01", "%")


def load_keys() -> dict[str, str]:
    text = STRINGS.read_text(encoding="utf-8")
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    keys = {}
    for m in re.finditer(r'"((?:[^"\\]|\\.)*)"\s*=\s*"(?:[^"\\]|\\.)*"\s*;', text):
        raw = re.sub(r"\\(.)", lambda e: {"n": "\n", "t": "\t"}.get(e.group(1), e.group(1)), m.group(1))
        keys[normalise_key(raw)] = raw
    return keys


def main() -> int:
    keys = load_keys()
    used = set()
    missing = []
    for path in sorted(SOURCES.rglob("*.swift")):
        src = path.read_text(encoding="utf-8")
        for m in CALL.finditer(src):
            line = src.count("\n", 0, m.start()) + 1
            try:
                text, _ = read_literal(src, m.end())
            except (ValueError, IndexError):
                print(f"{path.relative_to(ROOT)}:{line}: could not read the string literal", file=sys.stderr)
                return 2
            text = normalise_key(text)  # format strings such as String(format: String(localized: "… %@"))
            if not re.search(r"[^\W\d_]", text) or text in SKIP:
                continue
            if text in keys:
                used.add(text)
                continue
            missing.append((path.relative_to(ROOT), line, text.replace(PLACEHOLDER, "\\(…)")))

    unused = sorted(raw for norm, raw in keys.items() if norm not in used)
    if unused and "--verbose" in sys.argv[1:]:
        print(f"note: {len(unused)} key(s) not matched by a literal (built at run time, or stale):")
        for raw in unused:
            print(f"  {raw}")
    if missing:
        print(f"error: {len(missing)} string(s) without an English translation in {STRINGS.relative_to(ROOT)}:")
        for path, line, text in missing:
            print(f"  {path}:{line}: {text}")
        return 1
    print(f"ok: {len(used)} localised literal(s), all with an English key")
    return 0


if __name__ == "__main__":
    sys.exit(main())
