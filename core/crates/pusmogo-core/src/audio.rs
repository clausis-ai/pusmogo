//! WAV input for tools (MCP server, bench). The app records straight into f32.

use crate::SAMPLE_RATE;

/// Reads any PCM WAV, downmixes to mono and linearly resamples to 16 kHz.
pub fn read_wav_16k_mono(path: &str) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("cannot open {path}: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>()
        }
    }
    .map_err(|e| format!("bad wav data: {e}"))?;

    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    Ok(resample_linear(&mono, spec.sample_rate))
}

/// Linear resampling to 16 kHz. Good enough for speech models; not for music.
pub fn resample_linear(mono: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == SAMPLE_RATE || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = from_rate as f64 / SAMPLE_RATE as f64;
    let out_len = (mono.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = mono[idx];
            let b = *mono.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_halves_48k_to_16k_length() {
        let x = vec![0.25f32; 48_000];
        let y = resample_linear(&x, 48_000);
        assert_eq!(y.len(), 16_000);
        assert!(y.iter().all(|v| (*v - 0.25).abs() < 1e-6));
    }

    #[test]
    fn reads_written_wav() {
        let path = std::env::temp_dir().join(format!("pusmogo-audio-test-{}.wav", std::process::id()));
        let spec = hound::WavSpec { channels: 2, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..1600 {
            w.write_sample(16384i16).unwrap();
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        let s = read_wav_16k_mono(path.to_str().unwrap()).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(s.len(), 1600);
        assert!((s[0] - 0.25).abs() < 1e-3, "{}", s[0]);
    }
}
