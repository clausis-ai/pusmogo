# Adding a language

Up: [README.md](README.md)
Verified: 2026-11-08

Pusmogo v1 dictates Hungarian and English only. This page is for
work beyond v1: it says where the per-language data of the text cleanup lives and
how to add a pack. A new language also needs a speech model that knows it and a
way to choose it in the app; those parts are not covered here.

## What a language pack is

The rule-based cleanup (`core/crates/pusmogo-core/src/cleanup.rs`) reads its word
lists from small data files compiled into the core:

- `core/crates/pusmogo-core/src/lang/hu.toml`
- `core/crates/pusmogo-core/src/lang/en.toml`

Each file holds:

| Key | Meaning |
|---|---|
| `language` | the language code, the same as the file name |
| `fillers` | hesitation sounds that are dropped ("öö", "um"); never real words |
| `stutter_words` | short function words whose direct repetition is always a stutter |
| `abbreviations` | words that end with a full stop without ending a sentence ("pl", "etc") |
| `lowercase_abbreviations` | abbreviations that are also names, so only count in lower case ("max") |
| `keep_final_stop` | words whose full stop stays when a casual style drops the final stop ("stb", "etc") |
| `noun_markers` | words that make the next word a noun, not a spoken command ("a vessző", "the period") |
| `ambiguous_marks` | commands that are also everyday words ("pont", "period"); they count only where the speaker paused |
| `number_words` | whole number words ("seven"), used to spot spoken self-corrections |
| `number_parts`, `number_endings` | for languages that build numbers by compounding: the parts ("tizen", "három") and the endings a number may carry ("kor"; `""` for none) |
| `when_stems`, `when_words` | days, months and times of day: stems that take endings ("kedd") and words matched whole ("today") |
| `[marks]` table | spoken punctuation and line breaks: each key names a mark, each value lists what is said |
| `[paired]` table | spoken brackets and quotes, in the same form |

Mark names in `[marks]`: `paragraph` (blank line), `line`, `question`,
`exclamation`, `colon`, `semicolon`, `comma`, `full_stop`. Names in `[paired]`:
`open_paren`, `close_paren`, `open_low_quote`, `close_low_quote`, `low_quote`
(„…”, opens or closes), `open_straight_quote`, `close_straight_quote`,
`straight_quote` ("…"). The names and their characters are listed in
`MARK_NAMES` and `PAIRED_NAMES` in `cleanup.rs`; an unknown name stops the
tests. A spoken phrase is lower case with words separated by one space, and
may include the misspellings Whisper writes for it ("vesző" for "vessző").
Longer phrases are tried first, so "idézőjel vége" wins over "idézőjel".

Words are lower case, without punctuation. The cleanup runs the same rules
whatever language was dictated, because a Hungarian dictation often carries
English words, so every lookup sees the union of all packs. Keep a pack free of
words that are ordinary words in another supported language.

The files use a small subset of TOML, read by `src/lang.rs` without an extra
dependency: `#` comments, `key = "value"`, `key = ["a", "b"]` lists that may
span lines, and `[table]` headers. Plain keys go before the first table.

Still in Rust code, not yet in the packs: number and date words, the
abbreviation list of the style rules (`style.rs`), and the Hungarian suffix
rules after numbers. Those are rules as much as word lists; moving them
must keep every output byte for byte the same.

## Steps

1. Copy `lang/en.toml` to `lang/<code>.toml`, set `language`, fill the lists.
2. Add `("<code>", include_str!("lang/<code>.toml"))` to `PACKS` in `src/lang.rs`.
3. Run `cd core && cargo test --all` (every pack must parse and have every list)
   and `cargo clippy --all-targets -- -D warnings`.
4. Check that Hungarian and English outputs did not change: clean a large set of
   stored transcripts with the `clean_text` tool of `pusmogo-mcp` built before
   and after the change and compare the results; any difference is a regression
   unless it was intended.
