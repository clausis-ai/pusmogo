//! Usage insights computed from the local profile: totals, speaking pace,
//! daily streaks, the most used and most corrected words, the busiest hour and
//! app, and a playful title picked by fixed rules. Nothing leaves the Mac; the
//! app only renders what this returns.

use std::collections::HashMap;

use crate::profile::json_str;

/// One stored dictation, as far as insights need it.
#[derive(Debug, Clone, Copy)]
pub struct Row<'a> {
    /// Unix seconds.
    pub created_at: i64,
    pub text: &'a str,
    /// Bundle id or name of the target app; may be empty.
    pub app: &'a str,
    /// Length of the recording; 0 when unknown (rows saved before it was stored).
    pub audio_ms: i64,
    /// The row is a snippet's saved text, typed because the dictation matched
    /// its trigger. It counts as a dictation (streaks, hour, app) but its
    /// words were never spoken, so it stays out of words, pace and top word.
    pub expansion: bool,
    /// The local offset from UTC (east positive) at this row's own moment, so
    /// a summer dictation keeps its summer-time hour after the clocks change.
    /// `None` uses the offset passed to `compute`.
    pub utc_offset_s: Option<i64>,
}

/// A rule-picked title for the user's way of speaking. The app owns the
/// wording (Hungarian and English); the core only decides which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Title {
    /// Fewer than `MIN_FOR_TITLE` dictations: too early to say.
    Newcomer,
    /// Long dictations on average.
    Thinker,
    /// Many short messages.
    QuickReplier,
    /// Busiest hour late at night.
    NightOwl,
    /// Busiest hour early in the morning.
    EarlyBird,
    /// A daily streak of a week or more.
    Steady,
    /// Everything else.
    Storyteller,
}

impl Title {
    pub fn key(self) -> &'static str {
        match self {
            Title::Newcomer => "newcomer",
            Title::Thinker => "thinker",
            Title::QuickReplier => "quick_replier",
            Title::NightOwl => "night_owl",
            Title::EarlyBird => "early_bird",
            Title::Steady => "steady",
            Title::Storyteller => "storyteller",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insights {
    pub dictations: usize,
    pub words: usize,
    /// Words per minute of recorded audio; `None` until `MIN_AUDIO_MS` of
    /// audio with a known length has been dictated.
    pub words_per_minute: Option<u32>,
    /// Days in a row with at least one dictation, ending today or yesterday.
    pub current_streak: u32,
    pub longest_streak: u32,
    /// Most used word outside the stop-word lists, with its count.
    pub top_word: Option<(String, usize)>,
    /// Word the user corrected most often, with its count.
    pub top_correction: Option<(String, usize)>,
    /// Local hour (0–23) with the most dictations.
    pub peak_hour: Option<u8>,
    /// Target app with the most dictations.
    pub top_app: Option<(String, usize)>,
    pub title: Title,
}

const MIN_AUDIO_MS: i64 = 30_000;
const MIN_FOR_TITLE: usize = 5;

/// Words too common to be anyone's "most used word", Hungarian and English.
const STOP_WORDS: &[&str] = &[
    // Hungarian
    "a", "az", "egy", "és", "is", "nem", "hogy", "de", "meg", "ez", "azt", "ezt", "van", "volt", "lesz", "csak",
    "már", "még", "mint", "mert", "vagy", "ha", "itt", "ott", "akkor", "most", "nincs", "kell", "lehet", "olyan",
    "ami", "aki", "amit", "ahol", "amikor", "ezért", "azért", "pedig", "sem", "nagyon", "igen", "szia", "jó",
    "én", "te", "ő", "mi", "ti", "ők", "nekem", "neked", "neki", "nekünk", "velem", "veled", "vele", "rá", "rám",
    "ki", "be", "le", "fel", "el", "át", "össze", "vissza", "mit", "mi", "hol", "hogyan", "miért", "mikor",
    "ezek", "azok", "ennek", "annak", "ebben", "abban", "erre", "arra", "ebből", "abból", "után", "előtt",
    "alatt", "felett", "között", "szerint", "számára", "minden", "valami", "semmi", "sok", "kicsit", "kérlek",
    "vagyok", "vagy", "vagyunk", "vannak", "legyen", "tudom", "tudod", "szerintem", "úgy", "így", "aztán",
    "majd", "persze", "talán", "tehát", "illetve", "valamint", "amely", "amelyek", "melyik", "milyen", "mennyi",
    // English
    "the", "and", "to", "of", "in", "it", "is", "that", "for", "on", "with", "as", "at", "be", "this", "was",
    "are", "but", "not", "you", "your", "we", "our", "they", "their", "he", "she", "him", "her", "his", "my",
    "me", "us", "them", "i", "an", "or", "if", "so", "do", "does", "did", "have", "has", "had", "will", "would",
    "can", "could", "should", "just", "from", "by", "about", "what", "when", "where", "how", "why", "who",
    "which", "there", "here", "then", "than", "also", "all", "some", "any", "no", "yes", "up", "out", "into",
    "over", "after", "before", "been", "being", "were", "am", "its", "it's", "i'm", "don't", "let", "let's",
    "get", "got", "okay", "ok", "very", "really", "more", "one", "too", "please", "thanks", "hi", "hey",
];

/// Lower-cased words of `text`: letters and digits, keeping an inner
/// apostrophe or hyphen ("don't", "e-mail").
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’' || c == '-'))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).replace('’', "'").to_lowercase())
        .filter(|w| !w.is_empty())
}

fn countable(word: &str) -> bool {
    word.chars().count() >= 3 && word.chars().any(char::is_alphabetic) && !STOP_WORDS.contains(&word)
}

/// The key with the highest count; ties go to the alphabetically first key so
/// the result never depends on hash order.
fn top<K: Ord + Clone>(counts: &HashMap<K, usize>) -> Option<(K, usize)> {
    counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(k, n)| (k.clone(), *n))
}

/// Computes insights. `utc_offset_s` is the local time zone's offset (east
/// positive) at `now`, and for rows without their own offset, so days and
/// hours follow the user's clock; `now` is Unix seconds. `corrections` are the
/// replacement words the user accepted, one per event.
pub fn compute(rows: &[Row], corrections: &[&str], utc_offset_s: i64, now: i64) -> Insights {
    let local = |t: i64| t + utc_offset_s;
    let mut total_words = 0usize;
    let mut timed_words = 0usize;
    let mut audio_ms = 0i64;
    let mut word_counts: HashMap<String, usize> = HashMap::new();
    let mut hours: HashMap<u8, usize> = HashMap::new();
    let mut apps: HashMap<String, usize> = HashMap::new();
    let mut days: Vec<i64> = Vec::with_capacity(rows.len());

    let mut spoken = 0usize;
    for r in rows {
        let t = r.created_at + r.utc_offset_s.unwrap_or(utc_offset_s);
        *hours.entry((t.rem_euclid(86_400) / 3_600) as u8).or_default() += 1;
        days.push(t.div_euclid(86_400));
        if !r.app.is_empty() {
            *apps.entry(r.app.to_string()).or_default() += 1;
        }
        if r.expansion {
            continue;
        }
        spoken += 1;
        let n = r.text.split_whitespace().count();
        total_words += n;
        if r.audio_ms > 0 {
            timed_words += n;
            audio_ms += r.audio_ms;
        }
        for w in words(r.text).filter(|w| countable(w)) {
            *word_counts.entry(w).or_default() += 1;
        }
    }

    days.sort_unstable();
    days.dedup();
    let mut longest = 0u32;
    let mut run = 0u32;
    for (i, d) in days.iter().enumerate() {
        run = if i > 0 && days[i - 1] == d - 1 { run + 1 } else { 1 };
        longest = longest.max(run);
    }
    let today = local(now).div_euclid(86_400);
    let current = match days.last() {
        Some(&last) if last >= today - 1 => run,
        _ => 0,
    };

    let mut fixes: HashMap<String, usize> = HashMap::new();
    for c in corrections {
        let c = c.trim();
        if !c.is_empty() {
            *fixes.entry(c.to_string()).or_default() += 1;
        }
    }

    // Peak hour: ties go to the earlier hour.
    let peak_hour = hours.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(h, _)| *h);
    let words_per_minute = (audio_ms >= MIN_AUDIO_MS).then(|| (timed_words as f64 * 60_000.0 / audio_ms as f64).round() as u32);

    let title = pick_title(spoken, total_words, peak_hour, longest);
    Insights {
        dictations: rows.len(),
        words: total_words,
        words_per_minute,
        current_streak: current,
        longest_streak: longest,
        top_word: top(&word_counts),
        top_correction: top(&fixes),
        peak_hour,
        top_app: top(&apps),
        title,
    }
}

/// First matching rule wins; the order puts the most telling trait first.
fn pick_title(dictations: usize, words: usize, peak_hour: Option<u8>, longest_streak: u32) -> Title {
    if dictations < MIN_FOR_TITLE {
        return Title::Newcomer;
    }
    let avg = words as f64 / dictations as f64;
    if avg >= 40.0 {
        return Title::Thinker;
    }
    if avg <= 12.0 && dictations >= 20 {
        return Title::QuickReplier;
    }
    match peak_hour {
        Some(h) if !(5..22).contains(&h) => return Title::NightOwl,
        Some(5..=8) => return Title::EarlyBird,
        _ => {}
    }
    if longest_streak >= 7 {
        return Title::Steady;
    }
    Title::Storyteller
}

/// Insights as one JSON object; absent values are `null`.
pub fn to_json(i: &Insights) -> String {
    let mut out = format!(
        "{{\"dictations\":{},\"words\":{},\"words_per_minute\":{},\"current_streak\":{},\"longest_streak\":{},\"top_word\":",
        i.dictations,
        i.words,
        i.words_per_minute.map_or("null".to_string(), |w| w.to_string()),
        i.current_streak,
        i.longest_streak
    );
    let pair = |out: &mut String, p: &Option<(String, usize)>| match p {
        Some((w, n)) => {
            out.push_str("{\"text\":");
            json_str(out, w);
            out.push_str(&format!(",\"count\":{n}}}"));
        }
        None => out.push_str("null"),
    };
    pair(&mut out, &i.top_word);
    out.push_str(",\"top_correction\":");
    pair(&mut out, &i.top_correction);
    out.push_str(&format!(
        ",\"peak_hour\":{},\"top_app\":",
        i.peak_hour.map_or("null".to_string(), |h| h.to_string())
    ));
    pair(&mut out, &i.top_app);
    out.push_str(&format!(",\"title\":\"{}\"}}", i.title.key()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    // 2026-11-02 00:00 UTC, a Monday.
    const T0: i64 = 1_793_577_600;

    fn row(created_at: i64, text: &str) -> Row<'_> {
        Row { created_at, text, app: "", audio_ms: 0, expansion: false, utc_offset_s: None }
    }

    #[test]
    fn empty_profile_has_nothing_to_show() {
        let i = compute(&[], &[], 0, T0);
        assert_eq!(i.dictations, 0);
        assert_eq!(i.words_per_minute, None);
        assert_eq!((i.current_streak, i.longest_streak), (0, 0));
        assert_eq!((i.top_word.clone(), i.peak_hour, i.top_app.clone()), (None, None, None));
        assert_eq!(i.title, Title::Newcomer);
        assert!(to_json(&i).contains("\"words_per_minute\":null"));
    }

    #[test]
    fn most_used_word_skips_stop_words_and_folds_case() {
        let rows = [
            row(T0, "A projekt határideje péntek, és a Projekt fontos."),
            row(T0 + 60, "Küldd el a projekt tervét, kérlek."),
            row(T0 + 120, "The project plan is ready, and the plan is good."),
        ];
        let i = compute(&rows, &[], 0, T0);
        assert_eq!(i.top_word, Some(("projekt".to_string(), 3)));
        assert_eq!(i.words, 24);
    }

    #[test]
    fn streaks_follow_local_days() {
        // Days 0, 1, 2, then a gap, then 5 and 6.
        let rows: Vec<Row> = [0, 1, 2, 5, 6].iter().map(|d| row(T0 + d * DAY + 3_600, "szia")).collect();
        let now = T0 + 6 * DAY + 7_200;
        let i = compute(&rows, &[], 0, now);
        assert_eq!((i.current_streak, i.longest_streak), (2, 3));
        // Yesterday's dictation still counts as an unbroken streak today …
        assert_eq!(compute(&rows, &[], 0, now + DAY).current_streak, 2);
        // … but not after a whole day without one.
        assert_eq!(compute(&rows, &[], 0, now + 2 * DAY).current_streak, 0);
        // 23:30 UTC is already the next day at UTC+1.
        let late = [row(T0 + 23 * 3_600 + 1_800, "a"), row(T0 + DAY + 3_600, "b")];
        assert_eq!(compute(&late, &[], 0, T0 + DAY).longest_streak, 2);
        assert_eq!(compute(&late, &[], 3_600, T0 + DAY).longest_streak, 1);
    }

    #[test]
    fn each_row_keeps_its_own_summer_or_winter_offset() {
        // 2026-07-14 22:30 UTC is 00:30 on 15 July in Budapest summer time
        // (UTC+2); 2026-11-02 23:30 UTC is 00:30 on 3 November in winter
        // time (UTC+1). Today's offset (winter) must not move the summer row.
        let summer = 1_784_068_200;
        let rows = [
            Row { utc_offset_s: Some(7_200), ..row(summer, "nyári éjfél után") },
            Row { utc_offset_s: Some(7_200), ..row(summer + DAY, "másnap is") },
            Row { utc_offset_s: Some(3_600), ..row(T0 + 23 * 3_600 + 1_800, "téli éjfél után") },
        ];
        let i = compute(&rows, &[], 3_600, T0 + DAY + 3_600);
        assert_eq!(i.peak_hour, Some(0));
        // The two summer rows are 15 and 16 July, not 14 and 15; the winter
        // row is 3 November, today, so the current streak is 1.
        assert_eq!((i.current_streak, i.longest_streak), (1, 2));
        // With one offset for all rows (the old behaviour) the summer rows
        // fall at 23:30 the day before.
        let flat: Vec<Row> = rows.iter().map(|r| Row { utc_offset_s: None, ..*r }).collect();
        assert_eq!(compute(&flat, &[], 3_600, T0 + DAY + 3_600).peak_hour, Some(23));
    }

    #[test]
    fn pace_needs_known_audio() {
        let many = "szó ".repeat(54);
        let mut rows = vec![Row { created_at: T0, text: "egy kettő három négy öt hat", app: "", audio_ms: 3_000, expansion: false, utc_offset_s: None }];
        assert_eq!(compute(&rows, &[], 0, T0).words_per_minute, None);
        rows.push(Row { created_at: T0, text: &many, app: "", audio_ms: 27_000, expansion: false, utc_offset_s: None });
        // Rows without a length do not count toward the pace.
        rows.push(row(T0, "ez nem számít bele"));
        assert_eq!(compute(&rows, &[], 0, T0).words_per_minute, Some(120));
    }

    #[test]
    fn snippet_expansions_are_not_speech() {
        // Ten dictations of 100 words in 60 s each: 100 words per minute.
        let hundred = "beszéd ".repeat(100);
        let mut rows: Vec<Row> = (0..10)
            .map(|k| Row { created_at: T0 + k * 60, text: &hundred, app: "", audio_ms: 60_000, expansion: false, utc_offset_s: None })
            .collect();
        // A 1 s trigger that typed a 60-word signature.
        let signature = "aláírás ".repeat(60);
        rows.push(Row { created_at: T0 + DAY, text: &signature, app: "com.apple.mail", audio_ms: 1_000, expansion: true, utc_offset_s: None });
        let i = compute(&rows, &[], 0, T0 + DAY);
        assert_eq!((i.words, i.words_per_minute), (1_000, Some(100)));
        assert_eq!(i.top_word, Some(("beszéd".to_string(), 1_000)));
        // Still a dictation: it counts, keeps the streak and names the app.
        assert_eq!((i.dictations, i.longest_streak), (11, 2));
        assert_eq!(i.top_app, Some(("com.apple.mail".to_string(), 1)));
    }

    #[test]
    fn peak_hour_top_app_and_corrections() {
        let rows = [
            Row { created_at: T0 + 9 * 3_600, text: "a", app: "com.apple.mail", audio_ms: 0, expansion: false, utc_offset_s: None },
            Row { created_at: T0 + 9 * 3_600 + 60, text: "b", app: "com.tinyspeck.slackmacgap", audio_ms: 0, expansion: false, utc_offset_s: None },
            Row { created_at: T0 + 14 * 3_600, text: "c", app: "com.tinyspeck.slackmacgap", audio_ms: 0, expansion: false, utc_offset_s: None },
            Row { created_at: T0 + 15 * 3_600, text: "d", app: "", audio_ms: 0, expansion: false, utc_offset_s: None },
        ];
        let i = compute(&rows, &["Kubernetes", "Pusmogo", "Kubernetes", " "], 0, T0);
        assert_eq!(i.peak_hour, Some(9));
        assert_eq!(compute(&rows, &[], 2 * 3_600, T0).peak_hour, Some(11));
        assert_eq!(i.top_app, Some(("com.tinyspeck.slackmacgap".to_string(), 2)));
        assert_eq!(i.top_correction, Some(("Kubernetes".to_string(), 2)));
        let json = to_json(&i);
        assert!(json.contains("\"top_correction\":{\"text\":\"Kubernetes\",\"count\":2}"), "{json}");
        assert!(json.contains("\"peak_hour\":9"), "{json}");
    }

    #[test]
    fn titles_follow_the_rules() {
        let long = "szó ".repeat(45);
        let at = |h: i64, text: &'static str| -> Vec<Row<'static>> { (0..6).map(|k| row(T0 + h * 3_600 + k, text)).collect() };
        assert_eq!(compute(&at(10, "rövid")[..4], &[], 0, T0).title, Title::Newcomer);
        let thinkers: Vec<Row> = (0..6).map(|k| row(T0 + 10 * 3_600 + k, &long)).collect();
        assert_eq!(compute(&thinkers, &[], 0, T0).title, Title::Thinker);
        let quick: Vec<Row> = (0..20).map(|k| row(T0 + 10 * 3_600 + k, "oké, mindjárt jövök")).collect();
        assert_eq!(compute(&quick, &[], 0, T0).title, Title::QuickReplier);
        assert_eq!(compute(&at(23, "késő este még írok egy levelet"), &[], 0, T0).title, Title::NightOwl);
        assert_eq!(compute(&at(6, "reggel még írok egy levelet"), &[], 0, T0).title, Title::EarlyBird);
        assert_eq!(compute(&at(13, "délben írok egy levelet"), &[], 0, T0).title, Title::Storyteller);
        let week: Vec<Row> = (0..7).map(|d| row(T0 + d * DAY + 13 * 3_600, "délben írok egy levelet")).collect();
        assert_eq!(compute(&week, &[], 0, T0 + 6 * DAY).title, Title::Steady);
    }
}
