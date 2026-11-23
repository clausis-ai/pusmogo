#!/usr/bin/env python3
"""Fine-tune Whisper on a set built by make-finetune-data.py.

Reads <data>/train.jsonl and <data>/eval.jsonl (audio path relative to <data>,
text, language) and fine-tunes a Whisper checkpoint (default
openai/whisper-large-v3-turbo, MIT) with Hugging Face transformers. The
language token is fixed per clip, as the app fixes it per dictation; there is
no language detection to learn.

Runs on CUDA (bf16) for the real run and on Apple MPS or CPU for a smoke test
(--max-steps 10 --limit 64). Checkpoints go to <out>/checkpoint-*; the final
model and processor to <out>/final, ready for whisper.cpp's
convert-h5-to-ggml.py and quantize (q5_0, like the shipped models).

Dependencies (in a venv, never in the app):
  torch transformers soundfile jiwer accelerate

Usage:
  scripts/train-whisper.py <data-dir> <out-dir> [--base openai/whisper-large-v3-turbo]
      [--epochs 2] [--lr 1e-5] [--batch 16] [--accum 2] [--warmup 500]
      [--eval-every 1000] [--eval-limit 400] [--limit N] [--max-steps N]
      [--freeze-decoder] [--workers 8] [--seed 1]
"""

import argparse
import json
import random
import sys
from pathlib import Path

LANG = {"hu": "hungarian", "en": "english"}


def load(path: Path, limit: int | None, seed: int) -> list[dict]:
    rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if limit and len(rows) > limit:
        random.Random(seed).shuffle(rows)
        rows = rows[:limit]
    return rows


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("data", type=Path)
    p.add_argument("out", type=Path)
    p.add_argument("--base", default="openai/whisper-large-v3-turbo")
    p.add_argument("--epochs", type=float, default=2.0)
    p.add_argument("--lr", type=float, default=1e-5)
    p.add_argument("--batch", type=int, default=16)
    p.add_argument("--accum", type=int, default=2)
    p.add_argument("--warmup", type=int, default=500)
    p.add_argument("--eval-every", type=int, default=1000)
    p.add_argument("--eval-limit", type=int, default=400)
    p.add_argument("--limit", type=int)
    p.add_argument("--max-steps", type=int, default=-1)
    p.add_argument("--freeze-decoder", action="store_true")
    p.add_argument("--workers", type=int, default=8)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()

    import jiwer
    import numpy as np
    import soundfile as sf
    import torch
    from torch.utils.data import Dataset
    from transformers import (Seq2SeqTrainer, Seq2SeqTrainingArguments, WhisperForConditionalGeneration,
                              WhisperProcessor)

    cuda = torch.cuda.is_available()
    processor = WhisperProcessor.from_pretrained(a.base)
    model = WhisperForConditionalGeneration.from_pretrained(a.base, dtype=torch.float32)
    model.config.forced_decoder_ids = None
    model.generation_config.forced_decoder_ids = None
    model.config.use_cache = False
    if a.freeze_decoder:
        for prm in model.model.decoder.parameters():
            prm.requires_grad = False
    tok = processor.tokenizer

    class Clips(Dataset):
        def __init__(self, rows):
            self.rows = rows

        def __len__(self):
            return len(self.rows)

        def __getitem__(self, i):
            r = self.rows[i]
            audio, sr = sf.read(a.data / r["audio"], dtype="float32")
            if sr != 16000:
                raise ValueError(f"{r['audio']}: {sr} Hz, need 16000")
            feats = processor.feature_extractor(audio, sampling_rate=16000).input_features[0]
            tok.set_prefix_tokens(language=LANG.get(r["language"], r["language"]), task="transcribe")
            labels = tok(r["text"]).input_ids
            return {"input_features": feats, "labels": labels}

    def collate(batch):
        feats = torch.tensor(np.stack([b["input_features"] for b in batch]))
        width = max(len(b["labels"]) for b in batch)
        labels = torch.full((len(batch), width), -100, dtype=torch.long)
        for i, b in enumerate(batch):
            labels[i, : len(b["labels"])] = torch.tensor(b["labels"])
        if (labels[:, 0] == model.config.decoder_start_token_id).all():
            labels = labels[:, 1:]
        return {"input_features": feats, "labels": labels}

    def metrics(pred):
        ids = pred.predictions
        ref = pred.label_ids
        ref[ref == -100] = tok.pad_token_id
        hyp = tok.batch_decode(ids, skip_special_tokens=True)
        gold = tok.batch_decode(ref, skip_special_tokens=True)
        norm = lambda s: " ".join("".join(c if c.isalnum() or c.isspace() else " " for c in s.casefold()).split())
        pairs = [(norm(g), norm(h)) for g, h in zip(gold, hyp) if norm(g)]
        return {"wer": jiwer.wer([g for g, _ in pairs], [h for _, h in pairs]),
                "cer": jiwer.cer([g for g, _ in pairs], [h for _, h in pairs])}

    train = Clips(load(a.data / "train.jsonl", a.limit, a.seed))
    evals = Clips(load(a.data / "eval.jsonl", a.eval_limit, a.seed))
    print(f"train {len(train)} clips, eval {len(evals)} clips, device {'cuda' if cuda else 'mps/cpu'}", file=sys.stderr)

    args = Seq2SeqTrainingArguments(
        output_dir=str(a.out),
        per_device_train_batch_size=a.batch,
        per_device_eval_batch_size=a.batch,
        gradient_accumulation_steps=a.accum,
        learning_rate=a.lr,
        warmup_steps=min(a.warmup, a.max_steps // 2) if a.max_steps > 0 else a.warmup,
        num_train_epochs=a.epochs,
        max_steps=a.max_steps,
        lr_scheduler_type="linear",
        gradient_checkpointing=True,
        bf16=cuda,
        eval_strategy="steps",
        eval_steps=a.eval_every if a.max_steps <= 0 else a.max_steps,
        save_strategy="steps",
        save_steps=a.eval_every if a.max_steps <= 0 else a.max_steps,
        save_total_limit=3,
        load_best_model_at_end=True,
        metric_for_best_model="wer",
        greater_is_better=False,
        predict_with_generate=True,
        generation_max_length=225,
        logging_steps=25 if a.max_steps <= 0 else 1,
        dataloader_num_workers=a.workers if cuda else 0,
        remove_unused_columns=False,
        report_to=[],
        seed=a.seed,
    )
    trainer = Seq2SeqTrainer(model=model, args=args, train_dataset=train, eval_dataset=evals,
                             data_collator=collate, compute_metrics=metrics)
    trainer.train()
    final = a.out / "final"
    trainer.save_model(str(final))
    processor.save_pretrained(str(final))
    result = trainer.evaluate()
    (a.out / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
