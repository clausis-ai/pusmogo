# Data card: Hungarian fine-tune set

Up: [README.md](README.md)
Verified: 2026-11-22

What the Hungarian speech fine-tune may train on, how each clip is checked, and what a published model must say. A source not in the "allowed" table is not used.

## Sources

### Allowed

| Source | Licence | Use | Notes |
|---|---|---|---|
| `openai/whisper-large-v3-turbo` (base model) | MIT | base | the fine-tuned weights may be released under MIT or Apache-2.0 |
| Common Voice Hungarian, Scripted Speech 27.0 (139 h validated, 1,683 speakers) | CC0-1.0 | train | downloaded by hand from the Mozilla Data Collective; its terms forbid trying to identify a speaker and re-hosting or re-sharing the dataset, so the audio is never republished and copies on a rented training machine are deleted after the run |
| VoxPopuli Hungarian, transcribed (63 h, 143 speakers) | CC0 for the data (raw audio also under the European Parliament legal notice); VoxPopuli's own models and code are CC BY-NC 4.0 and are not used | train | `facebook/voxpopuli`, config `hu`; train and validation splits only |
| FLEURS Hungarian, **train split only** | CC-BY-4.0 | train | attribution in the model card; the dev and test splits are bench sets and never train |
| Granary Hungarian (`nvidia/Granary`, `hu_voxpopuli`, about 3.8 M segments) | CC-BY-4.0 for the transcripts, audio from VoxPopuli (CC0) | train, second run at the earliest | transcripts were written by a speech model, so they carry its mistakes; only segments where two different engines agree with the transcript (CER under 5 %) are used, and the model card says so |
| Pseudo-whispered copies (`scripts/pseudo-whisper.py`) | as the clip they come from | train | made by `make-finetune-data.py` |

### Not allowed

| Source | Why |
|---|---|
| macOS text-to-speech voices (`bench/audio/synth`) | Apple's licence limits the system voices to personal, non-commercial use; bench only |
| Recordings from the recording kit | the consent text is a draft and its licence is not decided ([CONSENT.md](../bench/testset/CONSENT.md)) |
| Community fine-tunes with undocumented data | their training data cannot be checked, so they are not a base |
| Non-commercial corpora (BEA, BEA-Large, BEA-Dialogue), private or customer data | licence or consent |
| Any bench clip or bench sentence | the leakage guard in `make-finetune-data.py` drops it |

## Checks (`scripts/data-audit.py`)

Every clip goes through the audit before `make-finetune-data.py`. The audit keeps, fixes, drops or flags each clip and writes the reason down.

| Check | Action |
|---|---|
| not 16 kHz mono 16-bit, shorter than 1 s or longer than 30 s | drop |
| more than 0.1 % of samples at full scale | drop |
| speech level below −60 dBFS (no speech) | drop |
| speech level −60 to −45 dBFS (a very quiet take) | review |
| empty reference | drop |
| `õ`/`û` where Hungarian has `ő`/`ű` (a Latin-1 slip) | fix |
| reference without a final `.` `!` `?` | fix (adds `.`) |
| a character outside the language's letters, digits and punctuation | review |
| a Roman numeral (the app writes "16.", not "XVI.") | review |
| same speaker, same sentence | drop |
| the best ASR run (`pusmogo-bench --json`) has CER above 0.5 against the reference | drop: wrong audio or wrong text |
| CER 0.2 to 0.5 | review |
| more than 400 clips from one speaker | drop the rest, chosen by a fixed hash |

Then a person listens to the 200 clips in `listen.tsv`, a fixed random sample of the kept clips. If more than 5 % are wrong, the limits get stricter and the audit runs again.

```bash
cd core && cargo build --release -p pusmogo-bench --features whisper && cd ..
core/target/release/pusmogo-bench --model ~/Library/Application\ Support/Pusmogo/models/ggml-large-v3-q5_0.bin \
  --json /tmp/ft/asr-large-v3.json /tmp/ft/fleurs-hu-train/hu.tsv
scripts/data-audit.py /tmp/ft/audit --source /tmp/ft/fleurs-hu-train/hu.tsv \
  --asr /tmp/ft/asr-large-v3.json --licence "FLEURS: CC-BY-4.0"
scripts/make-finetune-data.py /tmp/ft/set --source /tmp/ft/fleurs-hu-train/hu.keep.tsv --licence "FLEURS: CC-BY-4.0"
```

## Scope

The fine-tuned model is Hungarian only: every training and eval clip is Hungarian and carries the Hungarian language token. The app keeps the base turbo model for English, so English accuracy is measured for information and does not gate a release.

## Releasing a model

The model card on Hugging Face states:

- that the model is for Hungarian only, with the language fixed to Hungarian
- the base model and its licence, and the licence of the fine-tuned weights
- every training source with its licence and the hours kept after the audit, with attribution for FLEURS (CC-BY-4.0)
- that no private, customer or kit recordings and no synthetic macOS voices were used
- the bench table against the base model on every subset, including whispered speech, and the ship gate from [whisper-04](bench/whisper-04.md)
- known weak spots

Only the weights, the model card and the training configuration are published. The training audio is not.

## Polish model (the second model, after Whisper)

A small local language model (Qwen3-0.6B, Apache-2.0) that turns the speech engine's Hungarian into proper Hungarian. Its only job, stated in its prompt: Hungarian dictation spoken or whispered into a microphone; fix misheard words, endings, spelling and punctuation; never change the meaning; keep business English words in English with Hungarian suffixes (deadline-ra, meetingen). Its training pairs:

| Source | Licence | Use |
|---|---|---|
| Common Voice 27.0 Hungarian, `other.tsv` clips (never used to train Whisper) | CC0-1.0 | Pusmogo HU's own transcripts against the read sentence (`scripts/import-common-voice.py --source other`, `scripts/make-polish-pairs.py`) |
| The same clips, pseudo-whispered (`scripts/pseudo-whisper.py`) | CC0-1.0 | the engine's mistakes on whispered speech |
| Common Voice validated sentences | CC0-1.0 | correct sentences with one rule-based slip each (`scripts/make-polish-synth.py`) |
| FineWeb-2, Hungarian (HuggingFaceFW/fineweb-2, hun_Latn) | ODC-By 1.0 (Common Crawl web text) | clean business Hungarian and real contexts of English business terms (`scripts/make-polish-text.py`) |
| Magyar Ispell 1.9 (Hunspell hu_HU, Németh László, Godó Ferenc) | MPL-2.0 / LGPL-3.0+ | spell check: a web sentence is kept only when every word is known |
| `scripts/data/polish-en-terms.tsv` (our own list) | this repository's licence | English business terms, their correct Hungarian forms (AkH. 217) and misheard forms |

Rules used for the slips and checks come from *A magyar helyesírás szabályai* (12th edition, 2015); its text is not copied. Not used: Hungarian Webcorpus 2.0 and NYTK corpora (CC-BY-SA, share-alike), HunSum (CC-BY-NC-SA, non-commercial). Web sentences go through: length 5 to 30 words, sentence shape, no URL or spam wording, no subordinate clause without its comma, every word known to Hunspell. No pair holds a bench sentence.
