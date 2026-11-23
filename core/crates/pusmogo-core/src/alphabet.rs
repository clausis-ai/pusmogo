//! Keeps Hungarian dictation in the Latin alphabet.
//!
//! Whisper's vocabulary holds every script it was trained on, and a weak or
//! whispered Hungarian clip can still decode into Cyrillic, Greek or CJK
//! tokens. With the language fixed to Hungarian, those tokens are never
//! right, so the engine gives them a logit of minus infinity.
//!
//! The check works on a token's UTF-8 bytes, because byte-level tokens can
//! hold a part of a character. A token is foreign when it holds the lead byte
//! of a character outside Latin: two-byte leads from U+0180 (Latin
//! Extended-B, IPA, combining marks, Greek, Cyrillic, Armenian, Hebrew,
//! Arabic and the rest of that range), three-byte leads other than U+2000 to
//! U+2FFF (general punctuation such as „ ” – … and the euro sign stay
//! allowed), and every four-byte lead (emoji and rare CJK). Latin-1 and
//! Latin Extended-A stay, so á é í ó ö ő ú ü ű and names such as "Cañitas" or
//! "Schäfer" can still be written. A token that holds only continuation bytes
//! is allowed: its lead byte sits in the token before it, which this check has
//! already judged.

/// True when the token bytes start a character outside the scripts Hungarian
/// dictation uses.
pub fn foreign_script(bytes: &[u8]) -> bool {
    bytes.iter().any(|&b| matches!(b, 0xC6..=0xDF | 0xE0..=0xE1 | 0xE3..=0xEF | 0xF0..=0xF7))
}

#[cfg(test)]
mod tests {
    use super::foreign_script;

    #[test]
    fn keeps_hungarian_and_latin() {
        for s in [" árvíztűrő", " tükörfúrógép", "ŐŰ", " deployolni", " Cañitas", " Schäfer", "15%-kal", "„idézet”", " – ", "…", " 20 €"] {
            assert!(!foreign_script(s.as_bytes()), "{s}");
        }
    }

    #[test]
    fn drops_other_scripts() {
        for s in [" Привет", " Ωμέγα", " שלום", " مرحبا", "日本", " 한국", " ภาษา", " नमस्ते", "😀", "ə"] {
            assert!(foreign_script(s.as_bytes()), "{s}");
        }
    }

    #[test]
    fn allows_continuation_only_fragments() {
        assert!(!foreign_script(&[0x91]));
        assert!(!foreign_script(&[0xB1, 0x20]));
    }
}
