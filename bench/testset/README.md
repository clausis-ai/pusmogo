# Recording kit: reading script

Up: [docs/README.md](../../docs/README.md)

`script.tsv` is the text a volunteer reads aloud to build a personal test set: 130 lines (`id`, `lang`, `category`, `whisper`, `text`). 120 are Hungarian (everyday 40, tech jargon 25, names 15, numbers and dates 15, h/l/r-initial words 15, "X vs Y" questions 10) and 10 are English. Read every line once in a normal voice; the 20 lines with `whisper=yes` are read a second time whispering, so the bench can report normal and whispered accuracy side by side.

Recordings stay on the Mac, in a `testset/<speaker>/` folder beside the models folder (`~/Library/Application Support/Pusmogo/testset/`), as `<id>.wav` and `<id>-whisper.wav` (16 kHz mono). They are never committed or uploaded. To score them:

```bash
scripts/testset-manifest.py ~/Library/Application\ Support/Pusmogo/testset/<speaker>
core/target/release/pusmogo-bench --model <model.bin> <speaker dir>/normal.tsv <speaker dir>/whisper.tsv
```

Missing recordings are skipped, and so are takes that cannot be read or are shorter than 0.3 s (with a warning); a 16-bit take that hits full scale is kept with a "clipped" warning so it can be re-recorded. Add `--by-category` to also write `normal-<category>.tsv` (everyday, tech, names, numbers, hlr, vs, everyday-en) and pass those to `pusmogo-bench` for accuracy per category. Before recording, go through the consent text in [CONSENT.md](CONSENT.md) (Hungarian and English, two levels: local only or release) and save the speaker's choice as `consent.txt` in their folder; `testset-manifest.py` prints the level it found and warns when the file is missing or unclear (both count as `private`). To record, open Settings → Test recording (Tesztfelvétel) → Open in the app: it asks for a pseudonym and the consent level (saved as `consent.txt`), then shows one line at a time; Space starts and stops each take (or hold fn while reading and let go to save; a quick tap or Esc discards it), and the window resumes where you stopped. The speaker folder is the export: copy it as it is.

A dry run of this chain with synthetic takes, and its per-category table, is in [docs/bench/testset-01.md](../../docs/bench/testset-01.md).
