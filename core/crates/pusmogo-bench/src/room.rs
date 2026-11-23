//! Room tone appended after a clip, to model the pause between the last word
//! and the key release (a real release lags the last word by a fraction of a
//! second, while bench clips end on it).

/// Peak level of the appended tone: about -54 dBFS, a quiet room through a
/// laptop microphone. Low enough that the voice-activity checks see no speech.
pub const ROOM_TONE_PEAK: f32 = 0.002;

/// `samples` followed by `secs` seconds of quiet, deterministic noise at
/// `rate` Hz. Deterministic so bench runs stay comparable.
pub fn with_room_tone(samples: &[f32], secs: f64, rate: u32) -> Vec<f32> {
    let extra = (secs.max(0.0) * rate as f64).round() as usize;
    let mut out = Vec::with_capacity(samples.len() + extra);
    out.extend_from_slice(samples);
    let mut state: u32 = 0x2545_f491;
    for _ in 0..extra {
        // xorshift32: cheap, fixed sequence.
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let unit = state as f32 / u32::MAX as f32 * 2.0 - 1.0;
        out.push(unit * ROOM_TONE_PEAK);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_quiet_tone_of_the_right_length() {
        let clip = [0.5f32; 10];
        let out = with_room_tone(&clip, 0.25, 16_000);
        assert_eq!(out.len(), 10 + 4_000);
        assert_eq!(&out[..10], &clip);
        assert!(out[10..].iter().all(|s| s.abs() <= ROOM_TONE_PEAK));
        assert!(out[10..].iter().any(|s| *s != 0.0));
        assert_eq!(out, with_room_tone(&clip, 0.25, 16_000));
    }

    #[test]
    fn zero_or_negative_adds_nothing() {
        assert_eq!(with_room_tone(&[0.1, 0.2], 0.0, 16_000), vec![0.1, 0.2]);
        assert_eq!(with_room_tone(&[0.1], -1.0, 16_000), vec![0.1]);
    }
}
