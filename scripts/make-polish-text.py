#!/usr/bin/env python3
"""Pick clean Hungarian sentences, business register first, from web text for
the polish model's synthetic pairs.

Source: FineWeb-2, Hungarian part (hun_Latn), HuggingFaceFW/fineweb-2, ODC-By 1.0
(attribution: Hugging Face FineWeb-2; the text is Common Crawl web pages).
Only sentences that pass every check are kept, so the model learns from
correct Hungarian:

- 5 to 30 words, a capital first letter, ends in . ? or !
- no URL, e-mail, @, #, digit run, quote clutter, all-caps word or odd character
- no spam or adult wording; no subordinate clause without its comma
- every word known to the Hungarian Hunspell dictionary (Magyar Ispell 1.9,
  MPL-2.0 / LGPL-3.0+, László Németh and Ferenc Godó), except the correct forms
  of the English business terms in scripts/data/polish-en-terms.tsv

Writes, outside the repository:
  business.tsv    sentences with a business word (cég, ügyfél, határidő, ...)
  general.tsv     other clean sentences (a smaller share)
  en-context.tsv  sentences that use a trained English term correctly:
                  sentence, term, form (real contexts for the term pairs)

Usage:
  scripts/make-polish-text.py <out-dir> --parquet <file.parquet> --dict <dir>/hu_HU
      [--business 20000] [--general 8000]
"""

import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TERMS = ROOT / "scripts/data/polish-en-terms.tsv"
BUSINESS = re.compile(r"\b(cég|ügyfél|ügyfel|határidő|megbeszél|ajánlat|árajánlat|szerződés|számla|költség|"
                      r"bevétel|árbevétel|csapat|kolléga|munkatárs|vezető|értékesítés|stratégi|feladat|jelentés|"
                      r"prezentáci|megrendel|termék|szolgáltatás|partner|beszállító|projekt|tárgyal|költségvetés|"
                      r"pályázat|befektet|vállalat|vállalkozás|üzlet|piac|marketing|kampány|ügyvezető|osztály|"
                      r"értekezlet|egyeztet|visszajelzés|teljesít|ütemez|negyedév|könyvel|adó|munkaidő|iroda)",
                      re.IGNORECASE)
BAD = re.compile(r"(https?:|www\.|@|#|\d{2,}|[\"„”«»<>{}\[\]|\\/_*=+~^]|\.\.|\s-\s|[^\w\s.,;:?!'’()\-–])")
SPAM = re.compile(r"(szex|porn|erotik|kaszinó|casino|fogyókúr|viagra|társkeres|escort|nyerőgép|hitel nélkül)", re.IGNORECASE)
# A subordinate clause after a word with no comma before it (AkH: comma before
# hogy, mert, amely, ahol, amikor, mivel, ami-, aki-): web text often skips it,
# and the model must not learn that.
NO_COMMA = re.compile(r"\w (hogy|mert|amely\w*|ahol|amikor|mivel|ami\w*|aki\w*)\b")
SPLIT = re.compile(r"(?<=[.?!])\s+(?=[A-ZÁÉÍÓÖŐÚÜŰ])")


def term_forms() -> dict[str, str]:
    forms = {}
    for line in TERMS.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#") or line.startswith("!"):
            continue
        c = line.split("\t")
        for f in c[2:6]:
            if f:
                forms[f] = c[0]
    return forms


def candidates(parquet: Path, want: int):
    import pyarrow.parquet as pq
    seen = set()
    pf = pq.ParquetFile(parquet)
    for batch in pf.iter_batches(batch_size=2000, columns=["text"]):
        for doc in batch.column(0).to_pylist():
            for s in SPLIT.split(doc.replace("\n", " ")):
                s = " ".join(s.split())
                n = len(s.split())
                if not 5 <= n <= 30 or not s[0].isupper() or s[-1] not in ".?!" or BAD.search(s) \
                        or SPAM.search(s) or NO_COMMA.search(s):
                    continue
                if any(w.isupper() and len(w) > 1 for w in re.findall(r"\w+", s)):
                    continue
                if s in seen:
                    continue
                seen.add(s)
                yield s
                if len(seen) >= want:
                    return


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("out", type=Path)
    p.add_argument("--parquet", type=Path, required=True)
    p.add_argument("--dict", required=True)
    p.add_argument("--business", type=int, default=20000)
    p.add_argument("--general", type=int, default=8000)
    p.add_argument("--scan", type=int, default=3000000, help="candidate sentences to read")
    p.add_argument("--per-term", type=int, default=300, help="most en-context sentences per term")
    a = p.parse_args()
    out = a.out.resolve()
    if out == ROOT or ROOT in out.parents:
        p.error("write the sentences outside the repository (they hold web text)")
    out.mkdir(parents=True, exist_ok=True)
    forms = term_forms()
    form_re = re.compile(r"(?<![\w-])(" + "|".join(re.escape(f) for f in sorted(forms, key=len, reverse=True)) + r")(?![\w-])")

    business, general, en = [], [], []
    per_term: dict[str, int] = {}
    batch = []
    proc = subprocess.Popen(["hunspell", "-d", a.dict, "-a"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            text=True, bufsize=1)
    proc.stdout.readline()  # banner

    def known(s: str) -> bool:
        text = form_re.sub(" ", s)
        proc.stdin.write("^" + text + "\n")
        ok = True
        while True:
            line = proc.stdout.readline()
            if line in ("\n", ""):
                break
            if line[0] in "&#?":
                ok = False
        return ok

    for i, s in enumerate(candidates(a.parquet, a.scan), 1):
        if i >= a.scan:
            break
        is_business = bool(BUSINESS.search(s))
        has_term = form_re.search(s)
        if has_term and per_term.get(forms[has_term.group(1)], 0) >= a.per_term:
            has_term = None
        if not has_term and (len(business) >= a.business if is_business else len(general) >= a.general):
            continue
        if not known(s):
            continue
        if has_term:
            m = has_term
            en.append((s, forms[m.group(1)], m.group(1)))
            per_term[forms[m.group(1)]] = per_term.get(forms[m.group(1)], 0) + 1
        elif is_business:
            business.append(s)
        else:
            general.append(s)
        if i % 200000 == 0:
            print(f"read {i}: business {len(business)}, general {len(general)}, en {len(en)}", file=sys.stderr)
    proc.stdin.close()

    for name, rows in (("business", business), ("general", general)):
        with open(out / f"{name}.tsv", "w", encoding="utf-8", newline="") as f:
            w = csv.writer(f, delimiter="\t", quoting=csv.QUOTE_NONE, escapechar="\\")
            w.writerow(["sentence"])
            w.writerows([r] for r in rows)
    with open(out / "en-context.tsv", "w", encoding="utf-8") as f:
        f.write("sentence\tterm\tform\n")
        f.writelines(f"{s}\t{t}\t{fm}\n" for s, t, fm in en)
    print(f"business {len(business)}, general {len(general)}, en-context {len(en)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
