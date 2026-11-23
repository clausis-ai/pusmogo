//! Which speech model a dictation uses.
//!
//! Hungarian uses Pusmogo HU, our Hungarian-only fine-tune of turbo: same size
//! and speed as turbo, fewer errors than large-v3 (`docs/bench/train-01.md`),
//! so it is the default on every Mac. English uses turbo (Pusmogo HU knows
//! Hungarian only). Large-v3 stays as an explicit "most accurate" choice.
//! An explicit choice in Settings always wins over the default.

/// The models the app knows. The values cross the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechModel {
    /// `ggml-large-v3-turbo-q5_0.bin`, ~574 MB: fast, the English default.
    Turbo = 0,
    /// `ggml-large-v3-q5_0.bin`, ~1.08 GB: slow, 2 GB of memory.
    LargeV3 = 1,
    /// `ggml-pusmogo-hu-q5_0.bin`, ~574 MB: the Hungarian default.
    PusmogoHu = 2,
}

/// What the user picked in Settings for one language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    /// No choice made: the per-language default below.
    Auto = 0,
    /// "Gyors" / "Fast": turbo.
    Fast = 1,
    /// "Legpontosabb" / "Most accurate": large-v3.
    Accurate = 2,
    /// "Pusmogo HU": Hungarian only; English falls back to turbo.
    Hungarian = 3,
}

impl Preference {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Fast,
            2 => Self::Accurate,
            3 => Self::Hungarian,
            _ => Self::Auto,
        }
    }
}

/// The model a language should use when every model is on disk.
pub fn wanted(language: &str, pref: Preference, _physical_memory: u64) -> SpeechModel {
    match pref {
        Preference::Fast => SpeechModel::Turbo,
        Preference::Accurate => SpeechModel::LargeV3,
        Preference::Auto | Preference::Hungarian if language == "hu" => SpeechModel::PusmogoHu,
        Preference::Auto | Preference::Hungarian => SpeechModel::Turbo,
    }
}

/// The model to load now: the wanted one if it is installed, otherwise the
/// next one that is (dictation keeps working while a download is pending),
/// or `None` when none is on disk. Pusmogo HU never serves English.
pub fn pick(
    language: &str,
    pref: Preference,
    physical_memory: u64,
    installed: impl Fn(SpeechModel) -> bool,
) -> Option<SpeechModel> {
    let first = wanted(language, pref, physical_memory);
    let fallback: &[SpeechModel] = if language == "hu" {
        &[SpeechModel::PusmogoHu, SpeechModel::LargeV3, SpeechModel::Turbo]
    } else {
        &[SpeechModel::Turbo, SpeechModel::LargeV3]
    };
    std::iter::once(first).chain(fallback.iter().copied()).find(|&m| installed(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn on_disk(models: &'static [SpeechModel]) -> impl Fn(SpeechModel) -> bool {
        move |m| models.contains(&m)
    }

    #[test]
    fn hungarian_defaults_to_pusmogo_hu_on_every_mac() {
        assert_eq!(wanted("hu", Preference::Auto, 8 * GB), SpeechModel::PusmogoHu);
        assert_eq!(wanted("hu", Preference::Auto, 64 * GB), SpeechModel::PusmogoHu);
    }

    #[test]
    fn english_defaults_to_turbo_and_never_gets_the_hungarian_model() {
        assert_eq!(wanted("en", Preference::Auto, 64 * GB), SpeechModel::Turbo);
        assert_eq!(wanted("en", Preference::Hungarian, 64 * GB), SpeechModel::Turbo);
        assert_eq!(pick("en", Preference::Auto, 16 * GB, on_disk(&[SpeechModel::PusmogoHu])), None);
    }

    #[test]
    fn explicit_choice_wins() {
        assert_eq!(wanted("hu", Preference::Fast, 64 * GB), SpeechModel::Turbo);
        assert_eq!(wanted("hu", Preference::Accurate, 8 * GB), SpeechModel::LargeV3);
        assert_eq!(wanted("en", Preference::Accurate, 16 * GB), SpeechModel::LargeV3);
        assert_eq!(wanted("hu", Preference::Hungarian, 8 * GB), SpeechModel::PusmogoHu);
        assert_eq!(Preference::from_i32(3), Preference::Hungarian);
    }

    #[test]
    fn pick_falls_back_while_the_wanted_model_downloads() {
        use SpeechModel::*;
        assert_eq!(pick("hu", Preference::Auto, 16 * GB, on_disk(&[Turbo])), Some(Turbo));
        assert_eq!(pick("hu", Preference::Auto, 16 * GB, on_disk(&[Turbo, LargeV3])), Some(LargeV3));
        assert_eq!(pick("hu", Preference::Auto, 16 * GB, on_disk(&[Turbo, PusmogoHu])), Some(PusmogoHu));
        assert_eq!(pick("hu", Preference::Fast, 16 * GB, on_disk(&[PusmogoHu])), Some(PusmogoHu));
        assert_eq!(pick("en", Preference::Auto, 16 * GB, on_disk(&[LargeV3])), Some(LargeV3));
        assert_eq!(pick("hu", Preference::Auto, 16 * GB, on_disk(&[])), None);
    }
}
