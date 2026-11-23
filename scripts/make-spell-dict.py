#!/usr/bin/env python3
"""Convert the Hungarian Hunspell dictionary for the core's spell check.

Source: hu_HU.aff / hu_HU.dic of Magyar Ispell 1.9 (LibreOffice dictionaries),
(c) Németh László and Godó Ferenc, MPL-2.0 or LGPL-3.0+; used under MPL-2.0.
The result is a modified form: one-byte flags become numbers (the source is not
valid UTF-8), and morphology, suggestion tables and comments are dropped. The
words and affix rules are unchanged. Flag aliases stay; `src/spell.rs` expands
them when it loads the files.

Usage:
  scripts/make-spell-dict.py <dir>/hu_HU core/crates/pusmogo-core/data/hu
"""

import gzip
import sys

ONE_FLAG = {b"COMPOUNDFLAG", b"COMPOUNDBEGIN", b"COMPOUNDLAST", b"COMPOUNDEND", b"COMPOUNDFIRST", b"COMPOUNDFORBIDFLAG",
            b"COMPOUNDPERMITFLAG", b"COMPOUNDROOT", b"KEEPCASE", b"FORBIDDENWORD", b"NEEDAFFIX", b"ONLYROOT",
            b"ONLYINCOMPOUND", b"NOSUGGEST", b"SUBSTANDARD", b"GENERATE", b"SYLLABLENUM", b"LEMMA_PRESENT"}
DROP = {b"AM", b"REP", b"TRY", b"KEY", b"NAME", b"VERSION", b"HOME", b"MAP"}


def num(flags: bytes) -> bytes:
    return ",".join(str(b) for b in flags).encode()


def convert_aff(text: bytes) -> bytes:
    out, aliases = [], False
    for line in text.split(b"\n"):
        f = line.split()
        if not f or line.startswith(b"#") or f[0] in DROP:
            continue
        if f[0] == b"AF":
            count = not aliases and len(f[1]) < 6 and f[1].isdigit()
            out.append(b"AF " + (f[1] if count else num(f[1])))
            aliases = True
        elif f[0] in (b"SFX", b"PFX"):
            f[1] = num(f[1])
            out.append(b" ".join(f[:5]))
        elif f[0] in ONE_FLAG and len(f) > 1:
            out.append(f[0] + b" " + num(f[1]))
        elif f[0] == b"COMPOUNDRULE" and len(f) > 1 and not f[1].isdigit():
            out.append(f[0] + b" " + b"".join(b"(%d)" % b if b not in b"*?()" else bytes([b]) for b in f[1]))
        elif f[0] == b"CHECKCOMPOUNDPATTERN" and len(f) > 2:
            parts = [a + b"/" + num(b) if sep else a for a, sep, b in (x.partition(b"/") for x in f[1:])]
            out.append(f[0] + b" " + b" ".join(parts))
        else:
            out.append(line.strip())
    result = b"\n".join(out).replace(b"SET UTF-8", b"SET UTF-8\nFLAG num", 1) + b"\n"
    result.decode("utf-8")
    return result


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    src, dst = sys.argv[1:]
    with open(src + ".aff", "rb") as f:
        aff = convert_aff(f.read())
    with open(src + ".dic", "rb") as f:
        dic = b"\n".join(x.split(b"\t")[0] for x in f.read().split(b"\n"))
    for name, data in ((".aff.gz", aff), (".dic.gz", dic)):
        with open(dst + name, "wb") as f:
            f.write(gzip.compress(data, 9, mtime=0))
    return 0


if __name__ == "__main__":
    sys.exit(main())
