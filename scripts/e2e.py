#!/usr/bin/env python3
"""E2E smoke: run WAV clips through pusmogo-mcp (one process, warm engine) and time each call.

Usage: scripts/e2e.py <pusmogo-mcp binary> <prompts.tsv>
prompts.tsv: id<TAB>lang<TAB>reference text; <id>.wav sits next to it.
Needs PUSMOGO_MODEL in the environment. Prints a markdown table.
"""
import json
import os
import subprocess
import sys
import time

if len(sys.argv) != 3 or sys.argv[1] in ("-h", "--help"):
    sys.stderr.write(__doc__.lstrip())
    sys.exit(2)
binary, manifest = sys.argv[1], sys.argv[2]
base = os.path.dirname(os.path.abspath(manifest))
rows = [line.rstrip("\n").split("\t") for line in open(manifest, encoding="utf-8") if line.strip()]

proc = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)


def call(msg_id, method, params):
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "method": method, "params": params}) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())


call(0, "initialize", {"protocolVersion": "2025-06-18"})
print("| id | lang | audio s | wall s | reference | output |")
print("|---|---|---|---|---|---|")
for n, (clip_id, lang, ref) in enumerate(rows, start=1):
    wav = os.path.join(base, clip_id + ".wav")
    audio_s = (os.path.getsize(wav) - 44) / 32000  # 16 kHz mono s16
    t0 = time.monotonic()
    reply = call(n, "tools/call", {"name": "transcribe_file", "arguments": {"path": wav, "language": lang}})
    wall = time.monotonic() - t0
    result = reply.get("result", {})
    text = result.get("content", [{}])[0].get("text", json.dumps(reply))
    if result.get("isError"):
        text = "ERROR: " + text
    print(f"| {clip_id} | {lang} | {audio_s:.1f} | {wall:.2f} | {ref} | {text} |", flush=True)
proc.stdin.close()
proc.wait()
