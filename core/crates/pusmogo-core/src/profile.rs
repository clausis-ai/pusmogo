//! Local profile: one SQLite file with the user's history, dictionary,
//! corrections, settings and snippets. Nothing here leaves the Mac.
//!
//! The schema is versioned with `PRAGMA user_version`; `MIGRATIONS[i]` moves a
//! file from version `i` to `i + 1`. Never edit a shipped migration, append one.
//! Export writes a consistent copy of the whole file (`VACUUM INTO`); import
//! replaces the open profile with such a copy after checking it is one.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::cleanup::Dictionary;

pub use rusqlite::Error;

const MIGRATIONS: &[&str] = &[
    // 0 → 1: first schema.
    "CREATE TABLE history (
         id         INTEGER PRIMARY KEY,
         created_at INTEGER NOT NULL,
         lang       TEXT NOT NULL,
         text       TEXT NOT NULL,
         latency_ms INTEGER NOT NULL,
         app        TEXT NOT NULL DEFAULT ''
     );
     CREATE INDEX history_created ON history(created_at);
     CREATE TABLE dictionary (
         id         INTEGER PRIMARY KEY,
         from_text  TEXT NOT NULL UNIQUE,
         to_text    TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );
     CREATE TABLE corrections (
         id         INTEGER PRIMARY KEY,
         created_at INTEGER NOT NULL,
         wrong      TEXT NOT NULL,
         right      TEXT NOT NULL
     );
     CREATE TABLE settings (
         key   TEXT PRIMARY KEY,
         value TEXT NOT NULL
     );",
    // 1 → 2: snippets. `match_key` is the trigger folded by
    // `snippets::match_key`, so two triggers that sound the same cannot coexist.
    "CREATE TABLE snippets (
         id         INTEGER PRIMARY KEY,
         trigger    TEXT NOT NULL,
         match_key  TEXT NOT NULL UNIQUE,
         text       TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );",
    // 2 → 3: recording length per dictation, for the speaking pace in insights.
    // Rows saved before this read 0 (unknown) and are left out of the pace.
    "ALTER TABLE history ADD COLUMN audio_ms INTEGER NOT NULL DEFAULT 0;",
    // 3 → 4: corrections the user made but did not (yet) accept as a dictionary
    // entry are stored too (`accepted` 0), so a word fixed the same way twice
    // can be learned without asking. Older rows were all accepted offers.
    "ALTER TABLE corrections ADD COLUMN accepted INTEGER NOT NULL DEFAULT 1;",
    // 4 → 5: the text as spoken when the self-correction pass dropped words
    // from it ("háromkor, nem, négykor"), so History can show what was left
    // out. Empty when nothing was dropped and for older rows.
    "ALTER TABLE history ADD COLUMN spoken TEXT NOT NULL DEFAULT '';",
];

/// One saved snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub id: i64,
    /// What the user says, as they typed it.
    pub trigger: String,
    /// What gets inserted, verbatim.
    pub text: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// One dictation as stored in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    /// Unix seconds.
    pub created_at: i64,
    pub lang: String,
    pub text: String,
    pub latency_ms: i64,
    /// Bundle id or name of the app the text went to; may be empty.
    pub app: String,
    /// The text before spoken self-corrections were dropped; empty when
    /// nothing was dropped.
    pub spoken: String,
}

/// Totals for the stats line ("1 234 words in 87 dictations").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub dictations: i64,
    pub words: i64,
    pub median_latency_ms: i64,
}

pub struct Profile {
    conn: Connection,
}

/// Whether `err` says the file itself is damaged (not a database, corrupt),
/// as opposed to unreadable for a reason that leaves it intact.
fn is_damage(err: &Error) -> bool {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
    matches!(err, Error::SqliteFailure(e, _) if matches!(e.code, DatabaseCorrupt | NotADatabase))
}

/// Whether `path` holds bytes that cannot be an SQLite database: a non-empty
/// file not starting with the SQLite header. Missing, empty or unreadable
/// files are not foreign (SQLite creates or reports those itself).
fn has_foreign_header(path: &Path) -> bool {
    use std::io::Read;
    const MAGIC: &[u8; 16] = b"SQLite format 3\0";
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut head = [0u8; 16];
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return false,
        }
    }
    n > 0 && (n < MAGIC.len() || head != *MAGIC)
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Profile {
    /// Opens (or creates) the profile at `path` and brings its schema up to date.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::init(Connection::open(path)?)
    }

    /// Like `open`, but a file that is not an SQLite database or fails SQLite's
    /// own consistency check is moved aside (`<stem>.broken-<unix secs>.<ext>`,
    /// with its `-wal`/`-shm` files) and a fresh profile is started in its
    /// place, so the app keeps History and the Dictionary working instead of
    /// running without a profile. The second value is where the old file went.
    /// Every other failure (a profile from a newer version, no permission, a
    /// full disk) is returned untouched: those files may still be good.
    pub fn open_or_recover(path: impl AsRef<Path>) -> Result<(Self, Option<std::path::PathBuf>), Error> {
        let path = path.as_ref();
        // A file without the SQLite header is judged before SQLite opens it,
        // because opening would already truncate its `-wal` file.
        let err = if has_foreign_header(path) {
            Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB), None)
        } else {
            match Self::open(path).and_then(|p| p.check_integrity().map(|()| p)) {
                Ok(p) => return Ok((p, None)),
                Err(e) => e,
            }
        };
        if !is_damage(&err) {
            return Err(err);
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("profile");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("sqlite");
        let aside = path.with_file_name(format!("{stem}.broken-{}.{ext}", now()));
        let side = |p: &Path, suffix: &str| {
            let mut s = p.as_os_str().to_owned();
            s.push(suffix);
            std::path::PathBuf::from(s)
        };
        std::fs::rename(path, &aside).map_err(|_| err)?;
        for suffix in ["-wal", "-shm"] {
            let from = side(path, suffix);
            if from.exists() {
                let _ = std::fs::rename(&from, side(&aside, suffix));
            }
        }
        Ok((Self::open(path)?, Some(aside)))
    }

    /// Whether the file at `path` is a profile written by a newer Pusmogo
    /// (its schema version is above this build's), so the app can say
    /// "update the app" instead of showing an empty History. Opens the file
    /// read-only and never creates it; false for a missing, damaged or
    /// current file.
    pub fn is_from_newer_version(path: impl AsRef<Path>) -> bool {
        use rusqlite::OpenFlags;
        let path = path.as_ref();
        if !path.is_file() || has_foreign_header(path) {
            return false;
        }
        let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) else {
            return false;
        };
        matches!(conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)), Ok(v) if v > MIGRATIONS.len() as i64)
    }

    /// SQLite's quick consistency check (`PRAGMA quick_check`): an error when
    /// pages or indexes are damaged.
    fn check_integrity(&self) -> Result<(), Error> {
        let verdict: String = self.conn.pragma_query_value(None, "quick_check", |r| r.get(0))?;
        if verdict == "ok" {
            Ok(())
        } else {
            Err(Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT), Some(verdict)))
        }
    }

    /// Opens an existing profile for reading only, for tools outside the app
    /// (the MCP server). Never creates the file, never migrates it and refuses
    /// a schema other than this build's, so a reader can never lock the app out
    /// of its own profile or read columns that are not there.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, Error> {
        use rusqlite::OpenFlags;
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        let p = Self { conn };
        if p.schema_version()? != MIGRATIONS.len() as i64 {
            return Err(Error::InvalidQuery);
        }
        Ok(p)
    }

    /// The settings key that must read "1" before outside tools may read the
    /// history (`pusmogo-mcp search_history`). Off when missing.
    pub const SHARE_HISTORY_KEY: &'static str = "mcp.shareHistory";
    /// Same for the dictionary and snippets (`get_dictionary`, `list_snippets`).
    pub const SHARE_DICTIONARY_KEY: &'static str = "mcp.shareDictionary";

    /// Whether the user allowed outside tools to read what `key` guards.
    pub fn shares(&self, key: &str) -> bool {
        matches!(self.setting(key), Ok(Some(v)) if v == "1")
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, Error> {
        // Version first: a file from a newer version is refused before
        // anything (even the journal mode) is written to it.
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > MIGRATIONS.len() as i64 {
            return Err(Error::InvalidQuery);
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let mut p = Self { conn };
        p.migrate()?;
        Ok(p)
    }

    fn migrate(&mut self) -> Result<(), Error> {
        let version: usize = self.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
        if version > MIGRATIONS.len() {
            // A newer Pusmogo wrote this file; refuse rather than guess.
            return Err(Error::InvalidQuery);
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, Error> {
        self.conn.pragma_query_value(None, "user_version", |r| r.get(0))
    }

    // History

    pub fn add_history(&self, lang: &str, text: &str, latency_ms: i64, app: &str) -> Result<i64, Error> {
        self.add_history_at(now(), lang, text, latency_ms, app)
    }

    pub fn add_history_at(&self, created_at: i64, lang: &str, text: &str, latency_ms: i64, app: &str) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO history (created_at, lang, text, latency_ms, app) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![created_at, lang, text, latency_ms, app],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Stores how long the recording behind history entry `id` was.
    pub fn set_history_spoken(&self, id: i64, spoken: &str) -> Result<(), Error> {
        self.conn.execute("UPDATE history SET spoken = ?2 WHERE id = ?1", params![id, spoken])?;
        Ok(())
    }

    pub fn set_history_audio_ms(&self, id: i64, audio_ms: i64) -> Result<(), Error> {
        self.conn.execute("UPDATE history SET audio_ms = ?2 WHERE id = ?1", params![id, audio_ms.max(0)])?;
        Ok(())
    }

    /// Usage insights over the whole history (see `insights::compute`), with
    /// one fixed `utc_offset_s` for every row and for `now_s`.
    pub fn insights(&self, utc_offset_s: i64, now_s: i64) -> Result<crate::insights::Insights, Error> {
        self.insights_in(false, utc_offset_s, now_s)
    }

    /// Insights for the current moment in the system time zone. Each row's
    /// hour and day use the offset that was in force at its own moment
    /// (SQLite's `localtime`, which reads the system zone rules), so summer
    /// dictations keep their hour after the clocks change. `fallback_offset_s`
    /// is used only where that conversion fails.
    pub fn insights_now(&self, fallback_offset_s: i64) -> Result<crate::insights::Insights, Error> {
        self.insights_in(true, fallback_offset_s, now())
    }

    fn insights_in(&self, local: bool, utc_offset_s: i64, now_s: i64) -> Result<crate::insights::Insights, Error> {
        // A row whose text is a snippet's saved text was typed by a trigger,
        // not spoken (the app stores the snippet verbatim in History).
        let mut stmt = self.conn.prepare_cached(
            "SELECT h.created_at, h.text, h.app, h.audio_ms, EXISTS (SELECT 1 FROM snippets s WHERE s.text = h.text),
                    CASE WHEN ?1 THEN CAST(strftime('%s', h.created_at, 'unixepoch', 'localtime') AS INTEGER) - h.created_at END
             FROM history h",
        )?;
        type Raw = (i64, String, String, i64, bool, Option<i64>);
        let rows: Vec<Raw> = stmt
            .query_map([local], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<Result<_, _>>()?;
        let mut stmt = self.conn.prepare_cached("SELECT right FROM corrections WHERE accepted = 1")?;
        let fixes: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let rows: Vec<crate::insights::Row> = rows
            .iter()
            .map(|(created_at, text, app, audio_ms, expansion, offset)| crate::insights::Row {
                created_at: *created_at,
                text,
                app,
                audio_ms: *audio_ms,
                expansion: *expansion,
                utc_offset_s: *offset,
            })
            .collect();
        let now_offset = if local { local_offset(&self.conn, now_s).unwrap_or(utc_offset_s) } else { utc_offset_s };
        let fixes: Vec<&str> = fixes.iter().map(String::as_str).collect();
        Ok(crate::insights::compute(&rows, &fixes, now_offset, now_s))
    }

    /// Newest first. An empty `query` lists everything; otherwise a case-insensitive
    /// literal substring match. Folding is done here with Unicode lower-casing,
    /// because SQLite's `LIKE` folds ASCII only ("Ágnes" would miss "ágnes").
    pub fn search_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>, Error> {
        self.search_history_where(query, limit, false)
    }

    /// Like `search_history`, but leaves out rows whose text is a current
    /// snippet's saved text (the app stores a snippet expansion verbatim), so a
    /// reader allowed the history but not the snippets does not see them. An
    /// edited or deleted snippet's older rows are not recognised.
    pub fn search_spoken_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>, Error> {
        self.search_history_where(query, limit, true)
    }

    fn search_history_where(&self, query: &str, limit: usize, skip_snippets: bool) -> Result<Vec<HistoryEntry>, Error> {
        let mut stmt = self.conn.prepare_cached(if skip_snippets {
            "SELECT id, created_at, lang, text, latency_ms, app, spoken FROM history h
             WHERE NOT EXISTS (SELECT 1 FROM snippets s WHERE s.text = h.text)
             ORDER BY created_at DESC, id DESC"
        } else {
            "SELECT id, created_at, lang, text, latency_ms, app, spoken FROM history
             ORDER BY created_at DESC, id DESC"
        })?;
        let needle = query.to_lowercase();
        let mut rows = stmt.query([])?;
        let mut hits = Vec::new();
        while hits.len() < limit {
            let Some(r) = rows.next()? else { break };
            let text: String = r.get(3)?;
            if !needle.is_empty() && !text.to_lowercase().contains(&needle) {
                continue;
            }
            hits.push(HistoryEntry {
                id: r.get(0)?,
                created_at: r.get(1)?,
                lang: r.get(2)?,
                text,
                latency_ms: r.get(4)?,
                app: r.get(5)?,
                spoken: r.get(6)?,
            });
        }
        Ok(hits)
    }

    pub fn delete_history(&self, id: i64) -> Result<(), Error> {
        self.conn.execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<(), Error> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Retention: drops entries older than `keep_days` (0 keeps everything).
    /// Returns how many were removed.
    pub fn prune_history(&self, keep_days: i64) -> Result<usize, Error> {
        self.prune_history_at(keep_days, now())
    }

    pub fn prune_history_at(&self, keep_days: i64, now: i64) -> Result<usize, Error> {
        if keep_days <= 0 {
            return Ok(0);
        }
        self.conn.execute("DELETE FROM history WHERE created_at < ?1", [now - keep_days * 86_400])
    }

    pub fn stats(&self) -> Result<Stats, Error> {
        let mut stmt = self.conn.prepare_cached("SELECT text, latency_ms FROM history")?;
        let mut words = 0i64;
        let mut latencies = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let text: String = r.get(0)?;
            words += text.split_whitespace().count() as i64;
            latencies.push(r.get::<_, i64>(1)?);
        }
        latencies.sort_unstable();
        Ok(Stats {
            dictations: latencies.len() as i64,
            words,
            median_latency_ms: latencies.get(latencies.len() / 2).copied().unwrap_or(0),
        })
    }

    // Dictionary

    /// Adds or updates an entry; `from` is matched case-insensitively, so
    /// "clausis" and "Clausis" are the same entry.
    pub fn set_dictionary_entry(&self, from: &str, to: &str) -> Result<(), Error> {
        let from = from.trim().to_lowercase();
        let to = to.trim();
        if from.is_empty() || to.is_empty() {
            return Err(Error::InvalidParameterName("empty dictionary entry".into()));
        }
        self.conn.execute(
            "INSERT INTO dictionary (from_text, to_text, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(from_text) DO UPDATE SET to_text = excluded.to_text",
            params![from, to, now()],
        )?;
        Ok(())
    }

    pub fn remove_dictionary_entry(&self, from: &str) -> Result<(), Error> {
        self.conn.execute("DELETE FROM dictionary WHERE from_text = ?1", [from.trim().to_lowercase()])?;
        Ok(())
    }

    /// `(from, to)` pairs in the order they were added.
    pub fn dictionary_entries(&self) -> Result<Vec<(String, String)>, Error> {
        let mut stmt = self.conn.prepare_cached("SELECT from_text, to_text FROM dictionary ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// The dictionary the engine and cleanup use.
    pub fn dictionary(&self) -> Result<Dictionary, Error> {
        let mut d = Dictionary::new();
        for (from, to) in self.dictionary_entries()? {
            d.add(&from, &to);
        }
        Ok(d)
    }

    // Snippets

    /// Adds a snippet, or replaces the text (and the trigger's spelling) of the
    /// one whose trigger has the same match key. Returns its id.
    pub fn set_snippet(&self, trigger: &str, text: &str) -> Result<i64, Error> {
        let trigger = trigger.trim();
        let key = crate::snippets::match_key(trigger);
        if key.is_empty() || text.trim().is_empty() {
            return Err(Error::InvalidParameterName("empty snippet".into()));
        }
        self.conn.execute(
            "INSERT INTO snippets (trigger, match_key, text, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(match_key) DO UPDATE SET trigger = excluded.trigger, text = excluded.text",
            params![trigger, key, text, now()],
        )?;
        self.conn.query_row("SELECT id FROM snippets WHERE match_key = ?1", [key], |r| r.get(0))
    }

    /// Changes the trigger and text of snippet `id`. Fails if the new trigger
    /// is empty or already belongs to another snippet.
    pub fn update_snippet(&self, id: i64, trigger: &str, text: &str) -> Result<(), Error> {
        let trigger = trigger.trim();
        let key = crate::snippets::match_key(trigger);
        if key.is_empty() || text.trim().is_empty() {
            return Err(Error::InvalidParameterName("empty snippet".into()));
        }
        self.conn.execute(
            "UPDATE snippets SET trigger = ?2, match_key = ?3, text = ?4 WHERE id = ?1",
            params![id, trigger, key, text],
        )?;
        Ok(())
    }

    pub fn delete_snippet(&self, id: i64) -> Result<(), Error> {
        self.conn.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// All snippets in the order they were added.
    pub fn snippets(&self) -> Result<Vec<Snippet>, Error> {
        let mut stmt = self.conn.prepare_cached("SELECT id, trigger, text, created_at FROM snippets ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Snippet { id: r.get(0)?, trigger: r.get(1)?, text: r.get(2)?, created_at: r.get(3)? })
        })?;
        rows.collect()
    }

    /// The text of the snippet whose trigger is the whole `utterance`, if any.
    pub fn snippet_for(&self, utterance: &str) -> Result<Option<String>, Error> {
        let key = crate::snippets::match_key(utterance);
        if key.is_empty() {
            return Ok(None);
        }
        let exact = self.conn.query_row("SELECT text FROM snippets WHERE match_key = ?1", [&key], |r| r.get(0)).optional()?;
        if exact.is_some() {
            return Ok(exact);
        }
        // "My e-mail." against the trigger "my email": compare without spaces.
        self.conn
            .query_row(
                "SELECT text FROM snippets WHERE REPLACE(match_key, ' ', '') = ?1 ORDER BY id LIMIT 1",
                [crate::snippets::joined_key(&key)],
                |r| r.get(0),
            )
            .optional()
    }

    // Corrections

    /// An accepted "Add to dictionary?" offer, for the most-corrected insight.
    pub fn add_correction(&self, wrong: &str, right: &str) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO corrections (created_at, wrong, right, accepted) VALUES (?1, ?2, ?3, 1)",
            params![now(), wrong, right],
        )?;
        Ok(())
    }

    /// Records that the user fixed `wrong` to `right` right after a dictation
    /// (the edit watch saw it; nothing was accepted). When the same fix was
    /// seen before (`wrong` compared case-insensitively, `right` exactly), it
    /// goes into the dictionary without asking and this returns true; the app
    /// then says so and offers Undo. A fix the dictionary already makes is
    /// not counted again.
    pub fn note_correction(&self, wrong: &str, right: &str) -> Result<bool, Error> {
        let (wrong, right) = (wrong.trim(), right.trim());
        if wrong.is_empty() || right.is_empty() {
            return Err(Error::InvalidParameterName("empty correction".into()));
        }
        let key = wrong.to_lowercase();
        let known: Option<String> = self
            .conn
            .query_row("SELECT to_text FROM dictionary WHERE from_text = ?1", [&key], |r| r.get(0))
            .optional()?;
        if known.as_deref() == Some(right) {
            return Ok(false);
        }
        let mut stmt = self.conn.prepare_cached("SELECT wrong FROM corrections WHERE accepted = 0 AND right = ?1")?;
        let earlier: Vec<String> = stmt.query_map([right], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let seen = earlier.iter().filter(|w| w.trim().to_lowercase() == key).count();
        self.conn.execute(
            "INSERT INTO corrections (created_at, wrong, right, accepted) VALUES (?1, ?2, ?3, 0)",
            params![now(), wrong, right],
        )?;
        if seen == 0 {
            return Ok(false);
        }
        self.set_dictionary_entry(wrong, right)?;
        Ok(true)
    }

    // Settings

    pub fn setting(&self, key: &str) -> Result<Option<String>, Error> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), Error> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // Export / import

    /// Writes a consistent copy of the whole profile to `path` (must not exist).
    pub fn export_to(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref().to_str().ok_or(Error::InvalidPath(path.as_ref().to_path_buf()))?;
        self.conn.execute("VACUUM INTO ?1", [path])?;
        Ok(())
    }

    /// Replaces this profile's contents with the file at `path`. The file is
    /// opened read-only and must already be a Pusmogo profile (our first
    /// tables, a schema version this build knows); anything else, including a
    /// missing or empty file, is refused and changes nothing on either side.
    /// Only this profile is migrated afterwards; the source is never written.
    pub fn import_from(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        use rusqlite::OpenFlags;
        let src = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        let version: i64 = src.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if !(1..=MIGRATIONS.len() as i64).contains(&version) {
            return Err(Error::InvalidQuery);
        }
        let ours: i64 = src.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
               AND name IN ('history', 'dictionary', 'corrections', 'settings')",
            [],
            |r| r.get(0),
        )?;
        if ours != 4 {
            return Err(Error::InvalidQuery);
        }
        let backup = rusqlite::backup::Backup::new(&src, &mut self.conn)?;
        backup.run_to_completion(256, std::time::Duration::ZERO, None)?;
        drop(backup);
        self.migrate()
    }
}

/// Minimal JSON string escaping for the C ABI (no serde in the app binary).
pub(crate) fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// History entries as a JSON array of objects with the `HistoryEntry` field names
/// (`spoken` only when it is not empty).
pub fn history_json(entries: &[HistoryEntry]) -> String {
    let mut out = String::from("[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{},\"created_at\":{},\"lang\":", e.id, e.created_at));
        json_str(&mut out, &e.lang);
        out.push_str(",\"text\":");
        json_str(&mut out, &e.text);
        out.push_str(&format!(",\"latency_ms\":{},\"app\":", e.latency_ms));
        json_str(&mut out, &e.app);
        if !e.spoken.is_empty() {
            out.push_str(",\"spoken\":");
            json_str(&mut out, &e.spoken);
        }
        out.push('}');
    }
    out.push(']');
    out
}

/// Snippets as a JSON array of `{"id": …, "trigger": …, "text": …, "created_at": …}`.
pub fn snippets_json(entries: &[Snippet]) -> String {
    let mut out = String::from("[");
    for (i, s) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{},\"trigger\":", s.id));
        json_str(&mut out, &s.trigger);
        out.push_str(",\"text\":");
        json_str(&mut out, &s.text);
        out.push_str(&format!(",\"created_at\":{}}}", s.created_at));
    }
    out.push(']');
    out
}

/// Dictionary entries as a JSON array of `{"from": …, "to": …}`.
pub fn dictionary_json(entries: &[(String, String)]) -> String {
    let mut out = String::from("[");
    for (i, (from, to)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"from\":");
        json_str(&mut out, from);
        out.push_str(",\"to\":");
        json_str(&mut out, to);
        out.push('}');
    }
    out.push(']');
    out
}

/// The system time zone's offset from UTC at Unix second `t`, from SQLite's
/// `localtime` conversion; `None` if it fails.
fn local_offset(conn: &Connection, t: i64) -> Option<i64> {
    conn.query_row("SELECT CAST(strftime('%s', ?1, 'unixepoch', 'localtime') AS INTEGER) - ?1", [t], |r| r.get::<_, Option<i64>>(0))
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("pusmogo-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn migrates_a_fresh_file_and_reopens() {
        let path = tmp("migrate.db");
        {
            let p = Profile::open(&path).unwrap();
            assert_eq!(p.schema_version().unwrap(), MIGRATIONS.len() as i64);
            p.add_history("hu", "Szia, holnap találkozunk.", 800, "").unwrap();
        }
        let p = Profile::open(&path).unwrap();
        assert_eq!(p.search_history("", 10).unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_only_open_reads_beside_the_app_and_never_writes() {
        let path = tmp("readonly.db");
        assert!(Profile::open_read_only(&path).is_err(), "a missing file is not created");
        assert!(!path.exists());
        let app = Profile::open(&path).unwrap();
        app.add_history("hu", "Szia, holnap találkozunk.", 800, "").unwrap();
        let reader = Profile::open_read_only(&path).unwrap();
        assert_eq!(reader.search_history("holnap", 10).unwrap().len(), 1);
        assert!(!reader.shares(Profile::SHARE_HISTORY_KEY), "sharing is off by default");
        assert!(reader.add_history("hu", "x", 1, "").is_err(), "the reader cannot write");
        app.set_setting(Profile::SHARE_HISTORY_KEY, "1").unwrap();
        app.add_history("hu", "Második.", 700, "").unwrap();
        assert!(reader.shares(Profile::SHARE_HISTORY_KEY));
        assert!(!reader.shares(Profile::SHARE_DICTIONARY_KEY));
        assert_eq!(reader.search_history("", 10).unwrap().len(), 2, "sees the app's later writes");
        drop((reader, app));
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn read_only_open_refuses_an_older_schema_instead_of_migrating() {
        let path = tmp("older.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(MIGRATIONS[0]).unwrap();
            c.pragma_update(None, "user_version", 1).unwrap();
        }
        assert!(Profile::open_read_only(&path).is_err());
        let c = Connection::open(&path).unwrap();
        let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v, 1, "left as it was");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_moves_a_damaged_file_aside_and_starts_fresh() {
        let dir = std::env::temp_dir().join(format!("pusmogo-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile.sqlite");
        let garbage: Vec<u8> = (0..8192u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        std::fs::write(&path, &garbage).unwrap();
        std::fs::write(dir.join("profile.sqlite-wal"), b"stale").unwrap();

        let (p, aside) = Profile::open_or_recover(&path).unwrap();
        let aside = aside.expect("moved aside");
        assert!(aside.file_name().unwrap().to_str().unwrap().starts_with("profile.broken-"));
        assert_eq!(std::fs::read(&aside).unwrap(), garbage, "old bytes kept as they were");
        let mut wal = aside.clone().into_os_string();
        wal.push("-wal");
        assert_eq!(std::fs::read(&wal).unwrap(), b"stale");
        assert!(p.add_history("hu", "Szia", 100, "").unwrap() > 0);
        drop(p);

        // A good profile opens as it is, nothing moved.
        let (p, aside) = Profile::open_or_recover(&path).unwrap();
        assert!(aside.is_none());
        assert_eq!(p.search_history("", 10).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_catches_damaged_pages_behind_a_good_header() {
        let path = tmp("recover-pages.sqlite");
        {
            let p = Profile::open(&path).unwrap();
            for i in 0..400 {
                p.add_history("hu", &format!("Mondat száma {i}, elég hosszú ahhoz, hogy több lapot töltsön ki."), 100, "").unwrap();
            }
            p.conn.pragma_update(None, "journal_mode", "DELETE").unwrap();
        }
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 4096 * 4);
        for b in &mut bytes[4096 * 2..4096 * 4] {
            *b = 0xA5;
        }
        std::fs::write(&path, &bytes).unwrap();
        let (p, aside) = Profile::open_or_recover(&path).unwrap();
        let aside = aside.expect("damaged file moved aside");
        assert_eq!(p.search_history("", 10).unwrap().len(), 0);
        let _ = std::fs::remove_file(&aside);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_leaves_a_newer_profile_alone() {
        let path = tmp("recover-newer.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE history (id INTEGER PRIMARY KEY);").unwrap();
            c.pragma_update(None, "user_version", 99).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert!(Profile::open_or_recover(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(Profile::is_from_newer_version(&path));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn only_a_newer_schema_counts_as_newer() {
        let missing = tmp("newer-missing.db");
        let _ = std::fs::remove_file(&missing);
        assert!(!Profile::is_from_newer_version(&missing));
        assert!(!missing.exists());
        let current = tmp("newer-current.db");
        drop(Profile::open(&current).unwrap());
        assert!(!Profile::is_from_newer_version(&current));
        let text = tmp("newer-text.db");
        std::fs::write(&text, b"not a database").unwrap();
        assert!(!Profile::is_from_newer_version(&text));
        for p in [current, text] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn refuses_a_newer_schema() {
        let path = tmp("newer.db");
        {
            let c = Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", 99).unwrap();
        }
        assert!(Profile::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn history_search_is_newest_first_and_literal() {
        let p = Profile::open_in_memory().unwrap();
        p.add_history_at(100, "hu", "Az ár 15%-kal nőtt.", 700, "TextEdit").unwrap();
        p.add_history_at(200, "en", "Meeting at three", 650, "").unwrap();
        p.add_history_at(300, "hu", "Nincs benne százalék", 720, "").unwrap();
        let all = p.search_history("", 10).unwrap();
        assert_eq!(all.iter().map(|e| e.created_at).collect::<Vec<_>>(), [300, 200, 100]);
        // "%" is matched literally, not as a wildcard.
        let pct = p.search_history("%", 10).unwrap();
        assert_eq!(pct.len(), 1);
        assert_eq!(pct[0].app, "TextEdit");
        assert_eq!(p.search_history("MEETING", 10).unwrap().len(), 1);
        assert_eq!(p.search_history("", 2).unwrap().len(), 2);
    }

    #[test]
    fn history_keeps_the_text_before_self_corrections() {
        let p = Profile::open_in_memory().unwrap();
        let plain = p.add_history_at(100, "hu", "Rendben.", 700, "").unwrap();
        let fixed = p.add_history_at(200, "hu", "Holnap négykor találkozunk.", 700, "").unwrap();
        p.set_history_spoken(fixed, "Holnap háromkor, nem, négykor találkozunk.").unwrap();
        let all = p.search_history("", 10).unwrap();
        assert_eq!(all[0].spoken, "Holnap háromkor, nem, négykor találkozunk.");
        assert_eq!((all[1].id, all[1].spoken.as_str()), (plain, ""));
        // Search matches the typed text, not the dropped words.
        assert!(p.search_history("háromkor", 10).unwrap().is_empty());
        let json = history_json(&all);
        assert!(json.contains(r#""text":"Holnap négykor találkozunk.","latency_ms":700,"app":"","spoken":"Holnap háromkor, nem, négykor találkozunk."}"#), "{json}");
        assert!(json.ends_with(r#""text":"Rendben.","latency_ms":700,"app":""}]"#), "{json}");
    }

    #[test]
    fn history_search_folds_accented_capitals() {
        let p = Profile::open_in_memory().unwrap();
        p.add_history_at(100, "hu", "Ágnes holnap jön.", 700, "").unwrap();
        p.add_history_at(200, "hu", "ŐSZ VAN", 700, "").unwrap();
        p.add_history_at(300, "hu", "Éva szerint jó.", 700, "").unwrap();
        p.add_history_at(400, "hu", "Az ősz szép.", 700, "").unwrap();
        let texts = |q: &str| p.search_history(q, 10).unwrap().into_iter().map(|e| e.text).collect::<Vec<_>>();
        assert_eq!(texts("ágnes"), ["Ágnes holnap jön."]);
        assert_eq!(texts("ÁGNES"), ["Ágnes holnap jön."]);
        assert_eq!(texts("éva"), ["Éva szerint jó."]);
        assert_eq!(texts("ősz"), ["Az ősz szép.", "ŐSZ VAN"]);
        assert_eq!(texts("Ősz"), ["Az ősz szép.", "ŐSZ VAN"]);
        // The limit counts hits, not scanned rows.
        assert_eq!(p.search_history("ősz", 1).unwrap()[0].text, "Az ősz szép.");
        assert!(texts("osz").is_empty(), "accents still matter, only case is folded");
    }

    #[test]
    fn thousand_entries_search_fast() {
        let p = Profile::open_in_memory().unwrap();
        for i in 0..1000 {
            p.add_history_at(i, "hu", &format!("Ez a {i}. diktálás a projektről, Clausis {}", i % 7), 700, "").unwrap();
        }
        let t = Instant::now();
        let hits = p.search_history("clausis 3", 50).unwrap();
        let elapsed = t.elapsed();
        assert_eq!(hits.len(), 50);
        assert!(elapsed.as_millis() < 50, "search took {elapsed:?}");
    }

    #[test]
    fn prune_and_delete() {
        let p = Profile::open_in_memory().unwrap();
        let day = 86_400;
        let old = p.add_history_at(0, "hu", "régi", 1, "").unwrap();
        p.add_history_at(40 * day, "hu", "új", 1, "").unwrap();
        assert_eq!(p.prune_history_at(0, 41 * day).unwrap(), 0);
        assert_eq!(p.prune_history_at(30, 41 * day).unwrap(), 1);
        assert!(p.search_history("régi", 10).unwrap().is_empty());
        p.delete_history(old).unwrap(); // already gone: no error
        p.clear_history().unwrap();
        assert!(p.search_history("", 10).unwrap().is_empty());
    }

    #[test]
    fn stats_count_words_and_median_latency() {
        let p = Profile::open_in_memory().unwrap();
        assert_eq!(p.stats().unwrap(), Stats::default());
        p.add_history("hu", "egy kettő három", 900, "").unwrap();
        p.add_history("hu", "négy", 700, "").unwrap();
        p.add_history("en", "five six", 800, "").unwrap();
        assert_eq!(p.stats().unwrap(), Stats { dictations: 3, words: 6, median_latency_ms: 800 });
    }

    #[test]
    fn dictionary_upserts_case_insensitively_and_feeds_cleanup() {
        let p = Profile::open_in_memory().unwrap();
        p.set_dictionary_entry("klózisz", "Clausis").unwrap();
        p.set_dictionary_entry("OTP", "OTP").unwrap();
        p.set_dictionary_entry("Klózisz", "Clausis AI").unwrap();
        assert!(p.set_dictionary_entry(" ", "x").is_err());
        assert_eq!(
            p.dictionary_entries().unwrap(),
            [("klózisz".to_string(), "Clausis AI".to_string()), ("otp".to_string(), "OTP".to_string())]
        );
        let d = p.dictionary().unwrap();
        assert_eq!(crate::clean("a klózisz csapat", &d), "A Clausis AI csapat");
        assert_eq!(d.prompt(), format!("{} Clausis AI, OTP.", crate::cleanup::HU_PROMPT_LABEL));
        p.remove_dictionary_entry("KLÓZISZ").unwrap();
        assert_eq!(p.dictionary_entries().unwrap().len(), 1);
    }

    #[test]
    fn settings_round_trip() {
        let p = Profile::open_in_memory().unwrap();
        assert_eq!(p.setting("history_days").unwrap(), None);
        p.set_setting("history_days", "30").unwrap();
        p.set_setting("history_days", "90").unwrap();
        assert_eq!(p.setting("history_days").unwrap().as_deref(), Some("90"));
        p.add_correction("klózisz", "Clausis").unwrap();
    }

    #[test]
    fn a_fix_seen_twice_is_learned_without_asking() {
        let p = Profile::open_in_memory().unwrap();
        assert!(!p.note_correction("Kubernetis", "Kubernetes").unwrap());
        assert!(p.dictionary_entries().unwrap().is_empty());
        // Another word fixed once does not count towards this one.
        assert!(!p.note_correction("klózisz", "Clausis").unwrap());
        assert!(!p.note_correction("Kubernetis", "Kubernetisz").unwrap());
        assert!(p.note_correction("kubernetis", "Kubernetes").unwrap());
        assert_eq!(p.dictionary_entries().unwrap(), [("kubernetis".to_string(), "Kubernetes".to_string())]);
        // Already learned: not added or counted again.
        assert!(!p.note_correction("Kubernetis", "Kubernetes").unwrap());
        assert!(p.note_correction(" ", "x").is_err());
        // Seen fixes stay out of the most-corrected insight; accepted ones count.
        assert_eq!(p.insights(0, 0).unwrap().top_correction, None);
        p.add_correction("klózisz", "Clausis").unwrap();
        assert_eq!(p.insights(0, 0).unwrap().top_correction, Some(("Clausis".to_string(), 1)));
    }

    #[test]
    fn export_then_import_restores_everything() {
        let out = tmp("export.db");
        let a = Profile::open_in_memory().unwrap();
        a.add_history("hu", "Exportált sor", 800, "").unwrap();
        a.set_dictionary_entry("pusmogó", "Pusmogo").unwrap();
        a.set_setting("lang", "hu").unwrap();
        a.export_to(&out).unwrap();

        let mut b = Profile::open_in_memory().unwrap();
        b.add_history("en", "will be replaced", 1, "").unwrap();
        b.import_from(&out).unwrap();
        assert_eq!(b.search_history("", 10).unwrap()[0].text, "Exportált sor");
        assert_eq!(b.dictionary_entries().unwrap().len(), 1);
        assert_eq!(b.setting("lang").unwrap().as_deref(), Some("hu"));

        // A file that is not a profile leaves the open profile untouched.
        let junk = tmp("junk.db");
        std::fs::write(&junk, b"not a database at all, just text....").unwrap();
        assert!(b.import_from(&junk).is_err());
        assert_eq!(b.search_history("", 10).unwrap().len(), 1);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&junk);
    }

    #[test]
    fn import_refuses_what_is_not_a_profile_and_never_writes_the_source() {
        let mut b = Profile::open_in_memory().unwrap();
        b.add_history("hu", "megmarad", 1, "").unwrap();
        b.set_dictionary_entry("pusmogó", "Pusmogo").unwrap();

        let empty = tmp("empty.db");
        std::fs::write(&empty, b"").unwrap();
        let foreign = tmp("foreign.sqlite");
        {
            let c = Connection::open(&foreign).unwrap();
            c.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT); INSERT INTO notes (body) VALUES ('x');")
                .unwrap();
        }
        // Our version number on a file without our tables is refused too.
        let fake = tmp("fake.sqlite");
        {
            let c = Connection::open(&fake).unwrap();
            c.execute_batch("CREATE TABLE notes (id INTEGER PRIMARY KEY);").unwrap();
            c.pragma_update(None, "user_version", 1).unwrap();
        }
        let missing = tmp("missing.db");

        for path in [&empty, &foreign, &fake, &missing] {
            let before = std::fs::read(path).ok();
            assert!(b.import_from(path).is_err(), "{path:?} was accepted");
            assert_eq!(std::fs::read(path).ok(), before, "{path:?} was written");
            let mut wal = path.as_os_str().to_owned();
            wal.push("-wal");
            assert!(!std::path::Path::new(&wal).exists(), "{path:?} gained a -wal");
            assert_eq!(b.search_history("", 10).unwrap()[0].text, "megmarad");
            assert_eq!(b.dictionary_entries().unwrap().len(), 1);
        }
        assert!(!missing.exists());
        for p in [&empty, &foreign, &fake] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn import_of_a_version_1_export_leaves_the_file_as_it_was() {
        let path = tmp("v1-import.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(MIGRATIONS[0]).unwrap();
            c.pragma_update(None, "user_version", 1).unwrap();
            c.execute("INSERT INTO history (created_at, lang, text, latency_ms) VALUES (1, 'hu', 'régi sor', 5)", [])
                .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let mut b = Profile::open_in_memory().unwrap();
        b.import_from(&path).unwrap();
        assert_eq!(b.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(b.search_history("", 10).unwrap()[0].text, "régi sor");
        assert!(b.snippets().unwrap().is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_version_1_file_gains_snippets_and_keeps_its_data() {
        let path = tmp("v1.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(MIGRATIONS[0]).unwrap();
            c.pragma_update(None, "user_version", 1).unwrap();
            c.execute("INSERT INTO history (created_at, lang, text, latency_ms) VALUES (1, 'hu', 'régi sor', 5)", [])
                .unwrap();
        }
        let p = Profile::open(&path).unwrap();
        assert_eq!(p.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(p.search_history("", 10).unwrap()[0].text, "régi sor");
        assert!(p.snippets().unwrap().is_empty());
        // The old row has no recording length, so it stays out of the pace.
        assert_eq!(p.insights(0, 1).unwrap().words_per_minute, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snippets_add_edit_delete_and_match() {
        let p = Profile::open_in_memory().unwrap();
        let addr = p.set_snippet(" Címem ", "1051 Budapest, Példa utca 1.").unwrap();
        let link = p.set_snippet("zoom link", "https://example.com/j/123").unwrap();
        assert!(p.set_snippet("?!", "x").is_err());
        assert!(p.set_snippet("üres", "  ").is_err());

        assert_eq!(p.snippet_for("CÍMEM.").unwrap().as_deref(), Some("1051 Budapest, Példa utca 1."));
        assert_eq!(p.snippet_for("Zoom-link!").unwrap().as_deref(), Some("https://example.com/j/123"));
        assert_eq!(p.snippet_for("Küldd el a címem.").unwrap(), None);
        assert_eq!(p.snippet_for("").unwrap(), None);

        // Same trigger by its match key: the text is replaced, not duplicated.
        assert_eq!(p.set_snippet("cimem", "Új cím\n2. sor").unwrap(), addr);
        let all = p.snippets().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].trigger.as_str(), all[0].text.as_str()), ("cimem", "Új cím\n2. sor"));

        p.update_snippet(link, "meeting link", "https://example.com/j/456").unwrap();
        assert_eq!(p.snippet_for("zoom link").unwrap(), None);
        assert_eq!(p.snippet_for("Meeting link.").unwrap().as_deref(), Some("https://example.com/j/456"));
        // Renaming onto another snippet's trigger is refused.
        assert!(p.update_snippet(link, "Címem", "x").is_err());

        p.delete_snippet(addr).unwrap();
        assert_eq!(p.snippets().unwrap().len(), 1);
        assert_eq!(
            snippets_json(&p.snippets().unwrap()[..1].iter().map(|s| Snippet { created_at: 7, ..s.clone() }).collect::<Vec<_>>()),
            format!(r#"[{{"id":{link},"trigger":"meeting link","text":"https://example.com/j/456","created_at":7}}]"#)
        );
    }

    #[test]
    fn snippet_trigger_matches_whispers_hyphenated_spelling() {
        let p = Profile::open_in_memory().unwrap();
        p.set_snippet("my email", "me@example.com").unwrap();
        assert_eq!(p.snippet_for("My e-mail.").unwrap().as_deref(), Some("me@example.com"));
        assert_eq!(p.snippet_for("my e mail").unwrap().as_deref(), Some("me@example.com"));
        assert_eq!(p.snippet_for("Send my e-mail.").unwrap(), None);
        // An exact key wins over the joined fallback.
        p.set_snippet("e mail", "a").unwrap();
        p.set_snippet("email", "b").unwrap();
        assert_eq!(p.snippet_for("E-mail.").unwrap().as_deref(), Some("a"));
        assert_eq!(p.snippet_for("Email.").unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn insights_read_history_audio_length_and_corrections() {
        let p = Profile::open_in_memory().unwrap();
        let t0 = 1_793_577_600; // 2026-11-02 00:00 UTC
        let a = p.add_history_at(t0 + 9 * 3_600, "hu", "Küldd el a projekt tervét.", 700, "com.apple.mail").unwrap();
        let b = p.add_history_at(t0 + 86_400 + 9 * 3_600, "hu", &"projekt ".repeat(20), 700, "com.apple.mail").unwrap();
        p.set_history_audio_ms(a, 2_000).unwrap();
        p.set_history_audio_ms(b, 28_000).unwrap();
        p.add_correction("Kubernetis", "Kubernetes").unwrap();
        let i = p.insights(3_600, t0 + 86_400 + 12 * 3_600).unwrap();
        assert_eq!((i.dictations, i.words, i.words_per_minute), (2, 25, Some(50)));
        assert_eq!((i.current_streak, i.longest_streak, i.peak_hour), (2, 2, Some(10)));
        assert_eq!(i.top_word, Some(("projekt".to_string(), 21)));
        assert_eq!(i.top_correction, Some(("Kubernetes".to_string(), 1)));
        assert_eq!(i.top_app, Some(("com.apple.mail".to_string(), 2)));

        // A snippet expansion saved in History: a dictation, but not speech.
        let sig = "Üdvözlettel, Példa Anna, termékfelelős, Példa Kft., Budapest";
        p.set_snippet("aláírásom", sig).unwrap();
        let c = p.add_history_at(t0 + 86_400 + 10 * 3_600, "hu", sig, 700, "com.apple.mail").unwrap();
        p.set_history_audio_ms(c, 1_000).unwrap();
        let i = p.insights(3_600, t0 + 86_400 + 12 * 3_600).unwrap();
        assert_eq!((i.dictations, i.words, i.words_per_minute), (3, 25, Some(50)));
        assert_eq!(i.top_app, Some(("com.apple.mail".to_string(), 3)));
    }

    #[test]
    fn insights_now_uses_each_rows_own_summer_or_winter_offset() {
        // The zone must be set before the process reads it, so the check runs
        // in a child copy of this test binary with TZ set.
        if std::env::var_os("PUSMOGO_TZ_CHILD").is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "profile::tests::insights_now_uses_each_rows_own_summer_or_winter_offset", "--nocapture"])
                .env("PUSMOGO_TZ_CHILD", "1")
                .env("TZ", "Europe/Budapest")
                .output()
                .unwrap();
            let log = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success() && log.contains("1 passed"), "{log}{}", String::from_utf8_lossy(&out.stderr));
            return;
        }
        let p = Profile::open_in_memory().unwrap();
        assert_eq!(local_offset(&p.conn, 1_784_068_200), Some(7_200)); // July: CEST
        assert_eq!(local_offset(&p.conn, 1_793_577_600), Some(3_600)); // November: CET
        // 00:30 local on 15 July (summer) and on 3 November (winter).
        p.add_history_at(1_784_068_200, "hu", "nyári éjfél után", 700, "").unwrap();
        p.add_history_at(1_793_577_600 + 23 * 3_600 + 1_800, "hu", "téli éjfél után", 700, "").unwrap();
        // The fallback offset is not used when the zone can be read.
        let i = p.insights_now(-12 * 3_600).unwrap();
        assert_eq!(i.peak_hour, Some(0));
        assert_eq!((i.dictations, i.longest_streak), (2, 1));
    }

    #[test]
    fn json_escapes_quotes_and_newlines() {
        let e = HistoryEntry {
            id: 1,
            created_at: 2,
            lang: "hu".into(),
            text: "\"Idézet\"\núj sor\\".into(),
            latency_ms: 3,
            app: String::new(),
            spoken: String::new(),
        };
        assert_eq!(
            history_json(&[e]),
            r#"[{"id":1,"created_at":2,"lang":"hu","text":"\"Idézet\"\núj sor\\","latency_ms":3,"app":""}]"#
        );
        assert_eq!(dictionary_json(&[("a".into(), "B".into())]), r#"[{"from":"a","to":"B"}]"#);
        assert_eq!(dictionary_json(&[]), "[]");
    }
}
