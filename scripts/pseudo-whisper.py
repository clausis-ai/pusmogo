#!/usr/bin/env python3
"""Turn normal speech into pseudo-whispered speech for the bench.

Whispering has no vocal-fold vibration: the voice source is turbulent noise,
so there is no pitch, the spectrum is flatter (no glottal roll-off), the
lowest formants move up and the level drops by 20 dB or more. Whisper (the
speech model) does badly on it, and nobody wants to record hundreds of
whispered clips to measure that. This script fakes it with classic
linear-prediction (LPC) analysis and resynthesis:

  1. Pre-emphasise the signal and fit an all-pole vocal-tract filter to each
     32 ms frame (autocorrelation method, Levinson-Durbin recursion; Atal and
     Hanauer 1971, Makhoul 1975). Pre-emphasis keeps the glottal tilt out of
     the filter, so the result has the flatter tilt of a noise source.
  2. Move the filter's resonances: formants below 1.5 kHz up by --shift-hz
     (fading to no shift at 4 kHz) and widen every bandwidth a little, as
     measured for whispered vowels (Kallail and Emanuel 1984; Ito, Takeda and
     Itakura 2005, "Analysis and recognition of whispered speech").
  3. Drive the shifted filter with white noise instead of the voiced
     excitation (no F0 at all), match each frame's energy to the original
     frame, overlap-add, then de-emphasise partly (--deemph).
  4. Add breath rumble (low-passed noise below 300 Hz that follows the speech
     level, --breath-db relative to the whisper) as a close built-in
     microphone picks it up, scale the clip to --level-dbfs RMS (default
     -44 dBFS) and add a -62 dBFS microphone noise floor (--floor-dbfs).

The defaults were fitted to one real whispered Hungarian dictation on a
built-in microphone (2.5 s, -44 dBFS): it measured spectral tilt 12.3 dB and
17 % voiced frames; this script's version of the same phrase from TTS
measures 12.1 dB and 2 % (the TTS original: 74 % voiced).

This is the same family of method as the "pseudo-whisper" generation used to
train whisper-robust recognisers (Ghaffarzadegan, Bořil and Hansen, IEEE/ACM
TASLP 2016), written from the textbook description; it is a proxy, not real
whispering. `--features` prints the measures used to compare it with a real
whispered recording (level, voicing ratio, spectral tilt = energy 0.1-1 kHz
over 1-4 kHz in dB).

Input and output are 16 kHz mono 16-bit WAV. Manifests (`id<TAB>lang<TAB>ref`)
are copied with the clips, so pusmogo-bench can run the whispered set as is.
Write the output outside the repository (audio is never committed).

Usage:
  scripts/pseudo-whisper.py <manifest.tsv> <out-dir> [--shift-hz 150]
      [--deemph 0.9] [--breath-db 6] [--level-dbfs -44] [--floor-dbfs -62]
      [--seed 1]
  scripts/pseudo-whisper.py --features <clip.wav>...
"""

import argparse
import hashlib
import shutil
import sys
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, lfilter

SR = 16000
FRAME = 512  # 32 ms
HOP = FRAME // 2
ORDER = 18
PRE = 0.97


def read_wav(path: Path) -> np.ndarray:
    with wave.open(str(path), "rb") as w:
        if w.getframerate() != SR or w.getnchannels() != 1 or w.getsampwidth() != 2:
            raise SystemExit(f"{path}: need 16 kHz mono 16-bit WAV")
        data = w.readframes(w.getnframes())
    return np.frombuffer(data, dtype="<i2").astype(np.float64) / 32768.0


def write_wav(path: Path, x: np.ndarray) -> None:
    pcm = np.clip(np.round(x * 32767.0), -32768, 32767).astype("<i2")
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def lpc(frame: np.ndarray, order: int) -> np.ndarray:
    """All-pole coefficients a[0..order] (a[0] = 1) by Levinson-Durbin."""
    r = np.correlate(frame, frame, "full")[len(frame) - 1 : len(frame) + order]
    if r[0] <= 1e-12:
        return np.r_[1.0, np.zeros(order)]
    r[0] *= 1.0 + 1e-9  # white-noise correction keeps the recursion stable
    a = np.zeros(order + 1)
    a[0] = 1.0
    err = r[0]
    for i in range(1, order + 1):
        k = -(r[i] + np.dot(a[1:i], r[i - 1 : 0 : -1])) / err
        a[1 : i + 1] = a[1 : i + 1] + k * np.r_[a[i - 1 : 0 : -1], 1.0]
        err *= 1.0 - k * k
        if err <= 0:
            break
    return a


def shift_formants(a: np.ndarray, shift_hz: float) -> np.ndarray:
    roots = np.roots(a)
    out = []
    for z in roots:
        mag, ang = abs(z), np.angle(z)
        f = abs(ang) * SR / (2 * np.pi)
        if abs(z.imag) > 1e-9:
            weight = 1.0 if f < 1500 else max(0.0, (4000 - f) / 2500)
            f2 = min(f + shift_hz * weight, 0.95 * SR / 2)
            ang = np.sign(ang) * f2 * 2 * np.pi / SR
            mag = min(mag, 0.995) * 0.985  # wider bandwidths
        else:
            mag = min(mag, 0.995)
        out.append(mag * np.exp(1j * ang))
    return np.real(np.poly(out))


def whisperise(x: np.ndarray, shift_hz: float, level_dbfs: float, rng,
               deemph: float = 0.9, breath_db: float = 0.0, floor_dbfs: float = -62.0) -> np.ndarray:
    pre = lfilter([1.0, -PRE], [1.0], x)
    win = np.hanning(FRAME + 1)[:-1]  # periodic Hann: 50 % overlap-add sums to 1
    n = len(x)
    padded = np.r_[pre, np.zeros(FRAME)]
    y = np.zeros(n + FRAME)
    for start in range(0, n, HOP):
        seg = padded[start : start + FRAME] * win
        energy = np.sqrt(np.mean(seg * seg))
        if energy < 1e-6:
            continue
        a = shift_formants(lpc(seg, ORDER), shift_hz)
        noise = rng.standard_normal(FRAME * 2)
        synth = lfilter([1.0], a, noise)[FRAME:] * win  # drop the filter's run-in
        s_e = np.sqrt(np.mean(synth * synth))
        if s_e > 0:
            y[start : start + FRAME] += synth * (energy / s_e)
    y = lfilter([1.0], [1.0, -deemph], y[:n])
    rms = np.sqrt(np.mean(y * y))
    if rms > 0:
        y /= rms
    if breath_db > -90 and n:
        # Breath hitting a close microphone: low rumble that follows the speech level.
        env = np.sqrt(np.convolve(y * y, np.ones(FRAME) / FRAME, "same"))
        b, a = butter(2, 300 / (SR / 2))
        rumble = lfilter(b, a, rng.standard_normal(n))
        rumble *= env / max(np.sqrt(np.mean(rumble * rumble)), 1e-9)
        y += rumble * 10 ** (breath_db / 20)
    rms = np.sqrt(np.mean(y * y))
    if rms > 0:
        y *= 10 ** (level_dbfs / 20) / rms
    if floor_dbfs > -120 and n:
        b, a = butter(1, 1000 / (SR / 2))
        hiss = lfilter(b, a, rng.standard_normal(n))
        y += hiss * 10 ** (floor_dbfs / 20) / max(np.sqrt(np.mean(hiss * hiss)), 1e-9)
    peak = np.max(np.abs(y)) if n else 0
    if peak > 0.99:
        y *= 0.99 / peak
    return y


def features(x: np.ndarray) -> dict:
    """Level, voicing ratio and spectral tilt over the clip's active frames.

    Voicing is read on the 300-3400 Hz band (breath rumble and hum would read
    as periodic otherwise): a frame is voiced when its normalised
    autocorrelation peaks above 0.5 at a 70-400 Hz pitch lag. Active frames
    are within 20 dB of the loudest one.
    """
    b, a = butter(4, [300 / (SR / 2), 3400 / (SR / 2)], "band")
    xb = lfilter(b, a, x)
    starts = range(0, max(len(x) - FRAME, 0), HOP)
    e = np.array([np.sqrt(np.mean(xb[i : i + FRAME] ** 2)) for i in starts])
    active = [i for i, v in zip(starts, e) if len(e) and v > np.max(e) * 0.1]
    voiced = 0
    low = high = 0.0
    freqs = np.fft.rfftfreq(FRAME, 1 / SR)
    for i in active:
        f = xb[i : i + FRAME]
        ac = np.correlate(f, f, "full")[FRAME - 1 :]
        if ac[0] > 0 and np.max(ac[SR // 400 : SR // 70]) / ac[0] > 0.5:
            voiced += 1
        spec = np.abs(np.fft.rfft(x[i : i + FRAME] * np.hanning(FRAME))) ** 2
        low += spec[(freqs >= 100) & (freqs < 1000)].sum()
        high += spec[(freqs >= 1000) & (freqs < 4000)].sum()
    return {
        "seconds": round(len(x) / SR, 2),
        "level_dbfs": round(float(20 * np.log10(max(np.sqrt(np.mean(x * x)), 1e-9))), 1),
        "voiced_ratio": round(voiced / max(len(active), 1), 2),
        "tilt_db": round(float(10 * np.log10(max(low, 1e-12) / max(high, 1e-12))), 1),
    }


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("paths", nargs="+")
    p.add_argument("--features", action="store_true")
    p.add_argument("--shift-hz", type=float, default=150.0)
    p.add_argument("--level-dbfs", type=float, default=-44.0)
    p.add_argument("--deemph", type=float, default=0.9)
    p.add_argument("--breath-db", type=float, default=6.0)
    p.add_argument("--floor-dbfs", type=float, default=-62.0)
    p.add_argument("--seed", type=int, default=1)
    args = p.parse_args()

    if args.features:
        for path in args.paths:
            print(path.rsplit("/", 1)[-1], features(read_wav(Path(path))))
        return 0

    if len(args.paths) != 2:
        p.error("need <manifest.tsv> <out-dir>")
    manifest, out = Path(args.paths[0]), Path(args.paths[1])
    out.mkdir(parents=True, exist_ok=True)
    count = 0
    for line in manifest.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        cid = line.split("\t", 1)[0]
        src = manifest.parent / (cid if cid.endswith(".wav") else f"{cid}.wav")
        dst = out / src.name
        dst.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256(f"{args.seed}:{cid}".encode()).digest()
        rng = np.random.default_rng(int.from_bytes(digest[:8], "little"))
        write_wav(dst, whisperise(read_wav(src), args.shift_hz, args.level_dbfs, rng,
                                  args.deemph, args.breath_db, args.floor_dbfs))
        count += 1
    shutil.copyfile(manifest, out / manifest.name)
    print(f"{count} clips -> {out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
