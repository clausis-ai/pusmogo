//! Edit-distance error rates and latency percentiles.

/// Levenshtein distance between two token sequences.
pub fn edit_distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, x) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(x != y);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Errors and reference length, so corpus rates are sum(errors) / sum(len).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Count {
    pub errors: usize,
    pub len: usize,
}

impl Count {
    pub fn rate(self) -> f64 {
        if self.len == 0 {
            return if self.errors == 0 { 0.0 } else { 1.0 };
        }
        self.errors as f64 / self.len as f64
    }

    pub fn add(&mut self, other: Count) {
        self.errors += other.errors;
        self.len += other.len;
    }
}

/// Word errors on already-normalised text.
pub fn word_count(reference: &str, hypothesis: &str) -> Count {
    let r: Vec<&str> = reference.split_whitespace().collect();
    let h: Vec<&str> = hypothesis.split_whitespace().collect();
    Count { errors: edit_distance(&r, &h), len: r.len() }
}

/// Word errors after merging compound splits and joins: adjacent words on one
/// side whose concatenation is a word on the other side ("marketing kampány" vs
/// "marketingkampány") become one word. The difference to `word_count` is the
/// share of WER caused only by word boundaries, which Hungarian spelling makes
/// frequent and which a reader barely notices.
pub fn word_count_joined(reference: &str, hypothesis: &str) -> Count {
    let r: Vec<&str> = reference.split_whitespace().collect();
    let h: Vec<&str> = hypothesis.split_whitespace().collect();
    let r2 = join_pairs(&r, &h);
    let h2 = join_pairs(&h, &r);
    Count { errors: edit_distance(&r2, &h2), len: r.len() }
}

/// Joins adjacent pairs of `words` whose concatenation occurs in `other`.
fn join_pairs(words: &[&str], other: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if i + 1 < words.len() {
            let joined = format!("{}{}", words[i], words[i + 1]);
            if other.contains(&joined.as_str()) {
                out.push(joined);
                i += 2;
                continue;
            }
        }
        out.push(words[i].to_string());
        i += 1;
    }
    out
}

/// Character errors on already-normalised text (spaces count as characters).
pub fn char_count(reference: &str, hypothesis: &str) -> Count {
    let r: Vec<char> = reference.chars().collect();
    let h: Vec<char> = hypothesis.chars().collect();
    Count { errors: edit_distance(&r, &h), len: r.len() }
}

/// Nearest-rank percentile (p in 0..=100) of unsorted values.
pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    v[rank.clamp(1, v.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_basics() {
        assert_eq!(edit_distance(&["a", "b", "c"], &["a", "b", "c"]), 0);
        assert_eq!(edit_distance(&["a", "b", "c"], &["a", "x", "c"]), 1);
        assert_eq!(edit_distance(&["a", "b", "c"], &["a", "c"]), 1);
        assert_eq!(edit_distance::<&str>(&[], &["a", "b"]), 2);
    }

    #[test]
    fn wer_and_cer() {
        let w = word_count("a build piros", "a bujild piros");
        assert_eq!(w, Count { errors: 1, len: 3 });
        assert!((w.rate() - 1.0 / 3.0).abs() < 1e-9);
        let c = char_count("build", "bujild");
        assert_eq!(c, Count { errors: 1, len: 5 });
    }

    #[test]
    fn joined_wer_ignores_compound_boundaries_only() {
        // split in the output
        assert_eq!(word_count("a marketingkampány indul", "a marketing kampány indul").errors, 2);
        assert_eq!(word_count_joined("a marketingkampány indul", "a marketing kampány indul").errors, 0);
        // join in the output
        assert_eq!(word_count_joined("közel merészkednek", "közelmerészkednek").errors, 0);
        // a real error next to a split still counts
        assert_eq!(word_count_joined("ebben a negyedévben", "abban a negyed évben").errors, 1);
        // unrelated words are never merged
        assert_eq!(word_count_joined("a b", "a c"), Count { errors: 1, len: 2 });
        assert_eq!(word_count_joined("", ""), Count { errors: 0, len: 0 });
    }

    #[test]
    fn empty_reference() {
        assert_eq!(word_count("", "").rate(), 0.0);
        assert_eq!(word_count("", "köszönöm").rate(), 1.0);
    }

    #[test]
    fn percentiles() {
        let v = [0.9, 0.1, 0.5, 0.3, 0.7];
        assert_eq!(percentile(&v, 50.0), 0.5);
        assert_eq!(percentile(&v, 95.0), 0.9);
        assert_eq!(percentile(&v, 0.0), 0.1);
        assert_eq!(percentile(&[], 50.0), 0.0);
    }
}
