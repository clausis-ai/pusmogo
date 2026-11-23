# Text insertion paths (insert-01)

Up: [../README.md](../README.md)
Verified: 2026-11-07

How each insertion path behaves, measured with `PUSMOGO_INSERT_TEST` on an Apple silicon Mac with a clipboard-history app running (common on Macs, and the reason for the fix below). Target windows come from [`scripts/insert-probe.swift`](../../scripts/insert-probe.swift), which prints what landed in its field, plus TextEdit read back through Accessibility. The clipboard held a sentinel string before each run.

## The defect this fixed

Before, a pasteboard read was taken as proof the paste landed. Clipboard managers read every new clipboard at once, so ⌘V into a window with nothing to paste into reported `pasted`, then restored the old clipboard: **the dictated text was lost**. Measured before the fix: `nomenu` (⌘V does nothing) → `pasted`, field empty; `blank` → `pasted`, field empty, clipboard back to the sentinel.

Now the focused field is read back after the read (up to 0.6 s). It changed (or, if it was unreadable before, now contains the text) → `pasted`, old clipboard restored. It stayed the same → `copied:not-read`, text on the clipboard, "Vágólapra másolva, nyomd meg: ⌘V" notice. No readable field at all → `pasted:unverified`, the text stays on the clipboard so it is never lost. WebKit and Chromium build their accessibility tree on first request, so the field is queried once before ⌘V to wake it.

## Results after the fix

| Target | Mode | Outcome | Time | Field | Clipboard after |
|---|---|---|---|---|---|
| probe `native` (NSTextView) | AX direct | `direct` | 33 ms | text | sentinel |
| probe `native` | paste | `pasted` | 54 ms | text | sentinel (restored) |
| TextEdit | paste | `pasted` | 57 ms | text appended | sentinel (restored) |
| probe `web` (WKWebView textarea) | default (web area → paste) | `pasted` | 54 ms | text | sentinel (restored) |
| probe `web` | paste | `pasted` | 56 ms | text | sentinel (restored) |
| probe `nomenu` (⌘V ignored) | paste | `copied:not-read` | 928–1265 ms | empty | dictated text, notice shown |
| probe `blank` (no field) | default | `pasted:unverified` | 1051 ms | none | dictated text |

AX direct into TextEdit: 38 ms (earlier run, 3862064).

## Line breaks (spoken "új sor", "új bekezdés")

Text `Szia, holnap jövök.\nHozol kávét?\n\nKöszi.`, both paths, clipboard sentinel restored in every run.

| Target | AX direct | Paste | Field |
|---|---|---|---|
| probe `native` (NSTextView) | `direct` 27 ms | `pasted` 48 ms | line breaks kept |
| probe `web` (textarea) | `pasted` 51 ms | `pasted` 51 ms | line breaks kept |
| probe `single` (NSTextField) | `direct` 19 ms | `pasted` 48 ms | before: raw line breaks stored in a one-line field; now `Szia, holnap jövök. Hozol kávét? Köszi.` |
| probe `webline` (`<input type=text>`) | `pasted` 50 ms | `pasted` 46–52 ms (4/4) | WebKit turns each break into a space: `kávét?  Köszi.` (two spaces for a paragraph break) |

Single-line fields (AX role `AXTextField` or `AXComboBox`) now get the text with its lines joined by one space (`single_line` in core, `pusmogo_single_line` in the C ABI); the app logs `insert: single-line field, line breaks become spaces`. A web `<input>` is not recognised this way (its focus is reported as the web area), so WebKit's own replacement applies there. One early `webline` paste run reported `copied:not-read` with the text in the field; it did not recur in 5 later runs.

Not covered: Safari, Chrome and Electron apps (not driven, to leave the user's apps alone; the WKWebView probe stands in for web fields), Terminal (its scrollback is usually over the 100 000-character read-back limit, so expect `pasted:unverified`), and secure fields.

## Reproduce

```bash
swiftc -O scripts/insert-probe.swift -o /tmp/insert-probe
printf SENTINEL | pbcopy
/tmp/insert-probe nomenu 5 & sleep 1.5
PUSMOGO_INSERT_TEST="teszt" PUSMOGO_INSERT_MODE=paste PUSMOGO_INSERT_DELAY=1 \
  ./build/Pusmogo.app/Contents/MacOS/Pusmogo 2>&1 | grep insert
wait; pbpaste
```
