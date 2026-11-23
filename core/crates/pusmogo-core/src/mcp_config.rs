//! Registering the bundled `pusmogo-mcp` server with local AI apps.
//!
//! Each supported client keeps its MCP servers in a JSON file under the
//! user's home folder, in an object called `mcpServers`. This module adds or
//! removes one entry, `"pusmogo"`, and leaves every other key and its order
//! exactly as it was (numbers too). The change is spliced into the original
//! text, so the rest of the file keeps its own layout and the preview shows
//! only our lines; if a splice ever disagrees with the parsed result, the
//! whole file is re-printed instead. Writing is careful: the old file is copied
//! to a backup beside it first, and the new text is written to a temporary
//! file and renamed over the old one, so a crash never leaves half a file.
//!
//! "Undo" removes our entry again instead of restoring the backup: the client
//! apps rewrite these files themselves (one of them on every start), so an old
//! copy could throw away their newer changes. The backup stays for a person
//! who wants to restore it by hand, so Undo never overwrites it: it holds the
//! file as it was before our entry was last added.

use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The key our server is stored under in `mcpServers`.
pub const SERVER_NAME: &str = "pusmogo";

/// Suffix of the backup written beside a config before it is changed.
pub const BACKUP_SUFFIX: &str = ".pusmogo-backup";

/// An AI app whose MCP config we know how to edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
}

impl Client {
    pub const ALL: [Client; 3] = [Client::ClaudeDesktop, Client::ClaudeCode, Client::Cursor];

    /// Stable id used across the C ABI and in settings.
    pub fn id(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "claude-desktop",
            Client::ClaudeCode => "claude-code",
            Client::Cursor => "cursor",
        }
    }

    pub fn from_id(id: &str) -> Option<Client> {
        Client::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The config file, relative to the home folder.
    pub fn relative_path(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "Library/Application Support/Claude/claude_desktop_config.json",
            // User scope: servers available in every project.
            Client::ClaudeCode => ".claude.json",
            Client::Cursor => ".cursor/mcp.json",
        }
    }

    pub fn config_path(self, home: &Path) -> PathBuf {
        home.join(self.relative_path())
    }

    /// The entry this client should hold for a server binary at `command`.
    pub fn entry(self, command: &str) -> Value {
        let mut m = Map::new();
        if self == Client::ClaudeCode {
            m.insert("type".into(), Value::from("stdio"));
        }
        m.insert("command".into(), Value::from(command));
        m.insert("args".into(), Value::Array(Vec::new()));
        Value::Object(m)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The file is not valid JSON; we never overwrite what we cannot read.
    Unreadable(String),
    /// The top level, or its `mcpServers`, is not a JSON object.
    UnexpectedShape(&'static str),
    Io(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Unreadable(e) => write!(f, "the config file is not valid JSON: {e}"),
            ConfigError::UnexpectedShape(what) => write!(f, "{what} is not a JSON object"),
            ConfigError::Io(e) => write!(f, "{e}"),
        }
    }
}

fn parse(text: Option<&str>) -> Result<Map<String, Value>, ConfigError> {
    let text = text.unwrap_or("");
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(ConfigError::UnexpectedShape("the top level")),
        Err(e) => Err(ConfigError::Unreadable(e.to_string())),
    }
}

fn render(root: Value) -> String {
    let mut s = serde_json::to_string_pretty(&root).expect("a JSON value always serialises");
    s.push('\n');
    s
}

/// The spliced text when it parses to exactly `expected`, else a re-print.
fn choose(spliced: Option<String>, expected: Value) -> String {
    match spliced {
        Some(s) if serde_json::from_str::<Value>(&s).ok().as_ref() == Some(&expected) => s,
        _ => render(expected),
    }
}

// A small scanner over text serde_json has already accepted, giving byte
// spans so a change can be spliced in without re-printing the rest.

/// One object member: where its key starts and where its value lies.
struct Member {
    key: String,
    start: usize,
    value: (usize, usize),
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while matches!(b.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

/// `i` is at an opening quote; the index after the closing one.
fn string_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    loop {
        match *b.get(j)? {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
}

fn value_end(b: &[u8], i: usize) -> Option<usize> {
    match *b.get(i)? {
        b'"' => string_end(b, i),
        b'{' | b'[' => {
            let (mut depth, mut j) = (0usize, i);
            loop {
                match *b.get(j)? {
                    b'"' => {
                        j = string_end(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
        }
        _ => {
            let mut j = i;
            while b.get(j).is_some_and(|c| !matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')) {
                j += 1;
            }
            Some(j)
        }
    }
}

/// The members of the object whose `{` is at `open`, and the index of its `}`.
fn members(text: &str, open: usize) -> Option<(Vec<Member>, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = skip_ws(b, open + 1);
    if b.get(i) == Some(&b'}') {
        return Some((out, i));
    }
    loop {
        let key_end = string_end(b, i)?;
        let key = serde_json::from_str::<String>(&text[i..key_end]).ok()?;
        let colon = skip_ws(b, key_end);
        let v = skip_ws(b, colon + 1);
        let v_end = value_end(b, v)?;
        out.push(Member { key, start: i, value: (v, v_end) });
        i = skip_ws(b, v_end);
        match *b.get(i)? {
            b',' => i = skip_ws(b, i + 1),
            b'}' => return Some((out, i)),
            _ => return None,
        }
    }
}

/// The whitespace before `pos` on its line, when nothing else precedes it.
fn own_line_indent(text: &str, pos: usize) -> Option<&str> {
    let line_start = text[..pos].rfind('\n')? + 1;
    let lead = &text[line_start..pos];
    lead.chars().all(|c| c == ' ' || c == '\t').then_some(lead)
}

/// The leading whitespace of the line holding `pos`.
fn line_lead(text: &str, pos: usize) -> &str {
    let line_start = text[..pos].rfind('\n').map_or(0, |n| n + 1);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// `value` pretty-printed with the file's indent `unit`, continuation lines
/// shifted by `base`. JSON strings hold no raw newlines, so lines are safe.
fn pretty(value: &Value, unit: &str, base: &str) -> String {
    let two = serde_json::to_string_pretty(value).expect("a JSON value always serialises");
    let lines: Vec<String> = two
        .lines()
        .enumerate()
        .map(|(n, line)| {
            let body = line.trim_start_matches(' ');
            let depth = (line.len() - body.len()) / 2;
            let lead = if n == 0 { String::new() } else { base.to_string() };
            format!("{lead}{}{body}", unit.repeat(depth))
        })
        .collect();
    lines.join("\n")
}

/// Add `key: value` at the end of the object at `open`, in its own layout.
fn insert_member(text: &str, open: usize, ms: &[Member], close: usize, key: &str, value: &Value, unit: &str) -> String {
    let key = serde_json::to_string(key).expect("a string always serialises");
    let (at, end, piece) = match ms.last() {
        Some(last) => {
            let piece = match own_line_indent(text, last.start) {
                Some(ind) => format!(",\n{ind}{key}: {}", pretty(value, unit, ind)),
                None => format!(", {key}: {}", serde_json::to_string(value).expect("a JSON value always serialises")),
            };
            (last.value.1, last.value.1, piece)
        }
        None => {
            let base = line_lead(text, open);
            let inner = format!("{base}{unit}");
            (open, close + 1, format!("{{\n{inner}{key}: {}\n{base}}}", pretty(value, unit, &inner)))
        }
    };
    format!("{}{piece}{}", &text[..at], &text[end..])
}

/// The root object, its members, and the indent unit the file uses.
fn root_of(text: &str) -> Option<(usize, Vec<Member>, usize, String)> {
    let open = skip_ws(text.as_bytes(), 0);
    if text.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    let (ms, close) = members(text, open)?;
    let unit = ms
        .first()
        .and_then(|m| own_line_indent(text, m.start))
        .filter(|s| !s.is_empty())
        .unwrap_or("  ")
        .to_string();
    Some((open, ms, close, unit))
}

fn splice_install(text: &str, entry: &Value) -> Option<String> {
    let (open, root, close, unit) = root_of(text)?;
    let Some(servers) = root.iter().rev().find(|m| m.key == "mcpServers") else {
        let mut obj = Map::new();
        obj.insert(SERVER_NAME.into(), entry.clone());
        return Some(insert_member(text, open, &root, close, "mcpServers", &Value::Object(obj), &unit));
    };
    let s_open = servers.value.0;
    let (ms, s_close) = members(text, s_open)?;
    match ms.iter().rev().find(|m| m.key == SERVER_NAME) {
        Some(ours) => {
            let (v0, v1) = ours.value;
            let value = match own_line_indent(text, ours.start) {
                Some(ind) => pretty(entry, &unit, ind),
                None => serde_json::to_string(entry).ok()?,
            };
            Some(format!("{}{value}{}", &text[..v0], &text[v1..]))
        }
        None => Some(insert_member(text, s_open, &ms, s_close, SERVER_NAME, entry, &unit)),
    }
}

fn splice_remove(text: &str) -> Option<String> {
    let (_, root, _, _) = root_of(text)?;
    let s_open = root.iter().rev().find(|m| m.key == "mcpServers")?.value.0;
    let (ms, s_close) = members(text, s_open)?;
    let k = ms.iter().rposition(|m| m.key == SERVER_NAME)?;
    let (from, to) = if k > 0 {
        // Drop the comma and layout before our member along with it.
        (ms[k - 1].value.1, ms[k].value.1)
    } else if ms.len() > 1 {
        (ms[0].start, ms[1].start)
    } else {
        (s_open + 1, s_close)
    };
    Some(format!("{}{}", &text[..from], &text[to..]))
}

/// Whether a config holds our server entry, and whether it starts `command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    Missing,
    /// The entry runs `command`.
    Current,
    /// The entry runs another path, for example a copy of the app that was
    /// moved or deleted since: the client can no longer start the server.
    OtherCommand,
}

/// Whether the config holds our server entry, and whether its command is
/// `command`.
pub fn registration(text: Option<&str>, command: &str) -> Result<Registration, ConfigError> {
    let root = parse(text)?;
    let entry = match root.get("mcpServers") {
        Some(Value::Object(servers)) => servers.get(SERVER_NAME),
        _ => None,
    };
    Ok(match entry {
        None => Registration::Missing,
        Some(e) if e.get("command").and_then(Value::as_str) == Some(command) => Registration::Current,
        Some(_) => Registration::OtherCommand,
    })
}

/// The config text with our entry added or replaced. `None` when the file
/// already holds exactly this entry, so nothing needs writing.
pub fn with_server(text: Option<&str>, client: Client, command: &str) -> Result<Option<String>, ConfigError> {
    let mut root = parse(text)?;
    let servers = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(servers) = servers else {
        return Err(ConfigError::UnexpectedShape("mcpServers"));
    };
    let entry = client.entry(command);
    if servers.get(SERVER_NAME) == Some(&entry) {
        return Ok(None);
    }
    servers.insert(SERVER_NAME.into(), entry.clone());
    let spliced = text.filter(|t| !t.trim().is_empty()).and_then(|t| splice_install(t, &entry));
    Ok(Some(choose(spliced, Value::Object(root))))
}

/// The config text with our entry removed. `None` when it was not there.
/// An `mcpServers` object left empty is kept: the client may expect it.
pub fn without_server(text: Option<&str>) -> Result<Option<String>, ConfigError> {
    let mut root = parse(text)?;
    match root.get_mut("mcpServers") {
        Some(Value::Object(servers)) => {
            if servers.shift_remove(SERVER_NAME).is_none() {
                return Ok(None);
            }
        }
        Some(_) => return Err(ConfigError::UnexpectedShape("mcpServers")),
        None => return Ok(None),
    }
    Ok(Some(choose(text.and_then(splice_remove), Value::Object(root))))
}

/// A line diff of `old` → `new` for showing before the user confirms:
/// unchanged lines start with two spaces, removed with "- ", added with "+ ".
/// Runs of more than `context` unchanged lines are folded to "  …".
pub fn diff(old: &str, new: &str, context: usize) -> String {
    let all_a: Vec<&str> = old.lines().collect();
    let all_b: Vec<&str> = new.lines().collect();
    // Our edits touch a few lines of a file that may be very long: keep the
    // shared head and tail out of the table, which is quadratic in size.
    let head = all_a.iter().zip(&all_b).take_while(|(x, y)| x == y).count();
    let tail = all_a[head..].iter().rev().zip(all_b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let a = &all_a[head..all_a.len() - tail];
    let b = &all_b[head..all_b.len() - tail];
    // Longest common subsequence table of the middle, from the end.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut ops: Vec<(char, &str)> = all_a[..head].iter().map(|l| (' ', *l)).collect();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            ops.push((' ', a[i]));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            // Removals first, so a changed line reads "- old" then "+ new".
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    ops.extend(all_a[all_a.len() - tail..].iter().map(|l| (' ', *l)));
    let changed: Vec<bool> = ops.iter().map(|(k, _)| *k != ' ').collect();
    let near = |idx: usize| {
        let lo = idx.saturating_sub(context);
        let hi = (idx + context).min(ops.len() - 1);
        (lo..=hi).any(|k| changed[k])
    };
    let mut out = String::new();
    let mut folded = false;
    for (idx, (kind, line)) in ops.iter().enumerate() {
        if *kind == ' ' && !near(idx) {
            if !folded {
                out.push_str("  …\n");
                folded = true;
            }
            continue;
        }
        folded = false;
        out.push(*kind);
        out.push(' ');
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn read_optional(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::Io(format!("cannot read {}: {e}", path.display()))),
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(BACKUP_SUFFIX);
    PathBuf::from(s)
}

/// The file a write should replace: the target of a symlinked config (kept in
/// a dotfiles folder, say), so the link survives and the target gets the
/// change; the path itself otherwise.
fn write_target(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(path).unwrap_or_else(|_| match fs::read_link(path) {
            // A dangling link: write where it points, relative to its folder.
            Ok(t) => path.parent().map(|d| d.join(&t)).unwrap_or(t),
            Err(_) => path.to_path_buf(),
        }),
        _ => path.to_path_buf(),
    }
}

/// Back up the current file (if any) beside it, then replace it atomically.
/// A symlinked config is followed: the backup and the new text go beside
/// and over the link's target. With `keep_backup` an existing backup is left
/// as it is (Undo must not replace the copy taken before install).
fn write_with_backup(path: &Path, old: Option<&str>, new: &str, keep_backup: bool) -> Result<(), ConfigError> {
    let path = &write_target(path);
    let io = |what: &str, e: std::io::Error| ConfigError::Io(format!("cannot {what} {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io("create the folder of", e))?;
    }
    let backup_kept = keep_backup && fs::symlink_metadata(backup_path(path)).is_ok();
    if let Some(old) = old.filter(|_| !backup_kept) {
        // The backup gets the original's permissions before any byte is
        // written, so a private config never has a readable copy beside it.
        use std::os::unix::fs::OpenOptionsExt;
        let mut b = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(backup_path(path))
            .map_err(|e| io("back up", e))?;
        if let Ok(meta) = fs::metadata(path) {
            b.set_permissions(meta.permissions()).map_err(|e| io("back up", e))?;
        }
        b.write_all(old.as_bytes()).and_then(|_| b.sync_all()).map_err(|e| io("back up", e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".pusmogo-tmp");
    let tmp = PathBuf::from(tmp);
    let mut f = fs::File::create(&tmp).map_err(|e| io("write", e))?;
    f.write_all(new.as_bytes()).and_then(|_| f.sync_all()).map_err(|e| io("write", e))?;
    // Keep the original file's permissions (one client's config holds tokens).
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, path).map_err(|e| io("replace", e))
}

/// What installing would change in `client`'s config: the diff, or `None`
/// when the entry is already there.
pub fn preview_install(home: &Path, client: Client, command: &str) -> Result<Option<String>, ConfigError> {
    let path = client.config_path(home);
    let old = read_optional(&path)?;
    Ok(with_server(old.as_deref(), client, command)?.map(|new| diff(old.as_deref().unwrap_or(""), &new, 3)))
}

/// Add our server to `client`'s config. Returns whether the file changed.
pub fn install(home: &Path, client: Client, command: &str) -> Result<bool, ConfigError> {
    let path = client.config_path(home);
    let old = read_optional(&path)?;
    match with_server(old.as_deref(), client, command)? {
        Some(new) => write_with_backup(&path, old.as_deref(), &new, false).map(|_| true),
        None => Ok(false),
    }
}

/// Undo: remove our server from `client`'s config. Returns whether it changed.
pub fn uninstall(home: &Path, client: Client) -> Result<bool, ConfigError> {
    let path = client.config_path(home);
    let old = read_optional(&path)?;
    match without_server(old.as_deref())? {
        Some(new) => write_with_backup(&path, old.as_deref(), &new, true).map(|_| true),
        None => Ok(false),
    }
}

/// Whether `client`'s config on disk holds our server, and whether it points
/// at `command` (the bundled server of the running copy).
pub fn installed(home: &Path, client: Client, command: &str) -> Result<Registration, ConfigError> {
    registration(read_optional(&client.config_path(home))?.as_deref(), command)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/Applications/Pusmogo.app/Contents/MacOS/pusmogo-mcp";

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pusmogo-mcpconfig-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn adds_to_a_missing_or_empty_file() {
        let new = with_server(None, Client::Cursor, CMD).unwrap().unwrap();
        assert_eq!(
            new,
            format!("{{\n  \"mcpServers\": {{\n    \"pusmogo\": {{\n      \"command\": \"{CMD}\",\n      \"args\": []\n    }}\n  }}\n}}\n")
        );
        assert_eq!(with_server(Some("  \n"), Client::Cursor, CMD).unwrap().unwrap(), new);
        assert_eq!(registration(Some(&new), CMD).unwrap(), Registration::Current);
        assert_eq!(registration(None, CMD).unwrap(), Registration::Missing);
    }

    #[test]
    fn claude_code_entries_say_stdio() {
        let new = with_server(None, Client::ClaudeCode, CMD).unwrap().unwrap();
        let v: Value = serde_json::from_str(&new).unwrap();
        assert_eq!(v["mcpServers"]["pusmogo"]["type"], "stdio");
        let new = with_server(None, Client::ClaudeDesktop, CMD).unwrap().unwrap();
        let v: Value = serde_json::from_str(&new).unwrap();
        assert!(v["mcpServers"]["pusmogo"].get("type").is_none());
    }

    #[test]
    fn keeps_other_keys_their_order_and_numbers() {
        let old = r#"{"zeta": 1, "numStartups": 12345678901234567890, "ratio": 0.10, "mcpServers": {"other": {"command": "x"}}, "alpha": [true, null]}"#;
        let new = with_server(Some(old), Client::ClaudeCode, CMD).unwrap().unwrap();
        let keys: Vec<&str> = ["\"zeta\"", "\"numStartups\"", "\"ratio\"", "\"mcpServers\"", "\"other\"", "\"pusmogo\"", "\"alpha\""]
            .into_iter()
            .collect();
        let positions: Vec<usize> = keys.iter().map(|k| new.find(k).unwrap_or_else(|| panic!("{k} in {new}"))).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{new}");
        assert!(new.contains("12345678901234567890"), "{new}");
        assert!(new.contains("0.10"), "{new}");
        // Removing ours gives the other content back.
        let back = without_server(Some(&new)).unwrap().unwrap();
        let a: Value = serde_json::from_str(old).unwrap();
        let b: Value = serde_json::from_str(&back).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn same_entry_means_no_write_and_a_new_path_replaces() {
        let new = with_server(None, Client::Cursor, CMD).unwrap().unwrap();
        assert_eq!(with_server(Some(&new), Client::Cursor, CMD).unwrap(), None);
        let moved = with_server(Some(&new), Client::Cursor, "/tmp/pusmogo-mcp").unwrap().unwrap();
        assert!(moved.contains("/tmp/pusmogo-mcp") && !moved.contains(CMD));
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(matches!(with_server(Some("{ not json"), Client::Cursor, CMD), Err(ConfigError::Unreadable(_))));
        assert_eq!(with_server(Some("[]"), Client::Cursor, CMD), Err(ConfigError::UnexpectedShape("the top level")));
        assert_eq!(
            with_server(Some(r#"{"mcpServers": []}"#), Client::Cursor, CMD),
            Err(ConfigError::UnexpectedShape("mcpServers"))
        );
        assert_eq!(without_server(Some(r#"{"a": 1}"#)).unwrap(), None);
    }

    #[test]
    fn diff_marks_added_lines_and_folds_far_context() {
        let old = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3,\n  \"d\": 4,\n  \"e\": 5\n}\n";
        let new = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3,\n  \"d\": 4,\n  \"e\": 5,\n  \"f\": 6\n}\n";
        let d = diff(old, new, 1);
        assert_eq!(d, "  …\n    \"d\": 4,\n-   \"e\": 5\n+   \"e\": 5,\n+   \"f\": 6\n  }\n");
        assert_eq!(diff("", "x\n", 3), "+ x\n");
        assert_eq!(diff("a\n", "a\n", 1), "  …\n");
    }

    #[test]
    fn diff_of_a_huge_file_with_a_small_change_stays_small() {
        // 50 000 lines: the old full table would need ~10 GB.
        let body: String = (0..50_000).map(|k| format!("  \"k{k}\": {k},\n")).collect();
        let old = format!("{{\n{body}  \"z\": 0\n}}\n");
        let new = format!("{{\n{body}  \"z\": 0,\n  \"y\": 1\n}}\n");
        let d = diff(&old, &new, 1);
        assert_eq!(d, "  …\n    \"k49999\": 49999,\n-   \"z\": 0\n+   \"z\": 0,\n+   \"y\": 1\n  }\n");
    }

    #[test]
    fn install_backs_up_and_uninstall_removes_only_ours() {
        let home = scratch("roundtrip");
        let path = Client::ClaudeDesktop.config_path(&home);
        // No file yet: the folder is created, no backup is written.
        assert!(preview_install(&home, Client::ClaudeDesktop, CMD).unwrap().unwrap().contains("+     \"pusmogo\""));
        assert!(install(&home, Client::ClaudeDesktop, CMD).unwrap());
        assert_eq!(installed(&home, Client::ClaudeDesktop, CMD).unwrap(), Registration::Current);
        assert!(!backup_path(&path).exists());
        assert!(!install(&home, Client::ClaudeDesktop, CMD).unwrap());
        assert_eq!(preview_install(&home, Client::ClaudeDesktop, CMD).unwrap(), None);

        // The client adds its own server meanwhile; undo keeps it.
        let current = fs::read_to_string(&path).unwrap();
        let edited = current.replacen("\"mcpServers\": {", "\"mcpServers\": {\n    \"theirs\": {\"command\": \"y\"},", 1);
        fs::write(&path, &edited).unwrap();
        assert!(uninstall(&home, Client::ClaudeDesktop).unwrap());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), edited);
        let after: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(after["mcpServers"].get("pusmogo").is_none());
        assert_eq!(after["mcpServers"]["theirs"]["command"], "y");
        assert!(!uninstall(&home, Client::ClaudeDesktop).unwrap());
        assert!(!path.with_file_name("claude_desktop_config.json.pusmogo-tmp").exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn undo_keeps_the_backup_of_the_original() {
        let home = scratch("undo-backup");
        let path = Client::ClaudeDesktop.config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"theirs\": {\"command\": \"y\"}}\n}\n";
        fs::write(&path, original).unwrap();
        assert!(install(&home, Client::ClaudeDesktop, CMD).unwrap());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), original);
        assert!(uninstall(&home, Client::ClaudeDesktop).unwrap());
        assert_eq!(fs::read(backup_path(&path)).unwrap(), original.as_bytes());
        assert!(!fs::read_to_string(&path).unwrap().contains("pusmogo"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn backup_keeps_a_private_files_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let home = scratch("private");
        let path = Client::ClaudeCode.config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\"token\": \"secret\"}\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        // A stale, world-readable backup from earlier is tightened too.
        fs::write(backup_path(&path), "old").unwrap();
        fs::set_permissions(backup_path(&path), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(install(&home, Client::ClaudeCode, CMD).unwrap());
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&backup_path(&path)), 0o600);
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "{\"token\": \"secret\"}\n");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_symlinked_config_keeps_its_link_and_the_target_changes() {
        let home = scratch("symlink");
        let dotfiles = home.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        let target = dotfiles.join("claude.json");
        fs::write(&target, "{\"theme\": \"dark\"}\n").unwrap();
        let path = Client::ClaudeCode.config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();

        assert!(install(&home, Client::ClaudeCode, CMD).unwrap());
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        let text = fs::read_to_string(&target).unwrap();
        assert!(text.contains("pusmogo") && text.contains("theme"), "{text}");
        let target = fs::canonicalize(&target).unwrap();
        assert_eq!(fs::read_to_string(backup_path(&target)).unwrap(), "{\"theme\": \"dark\"}\n");
        assert!(!backup_path(&path).exists());
        assert_eq!(installed(&home, Client::ClaudeCode, CMD).unwrap(), Registration::Current);

        assert!(uninstall(&home, Client::ClaudeCode).unwrap());
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert!(!fs::read_to_string(&target).unwrap().contains("pusmogo"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_broken_file_is_left_alone() {
        let home = scratch("broken");
        let path = Client::Cursor.config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ oops").unwrap();
        assert!(install(&home, Client::Cursor, CMD).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ oops");
        assert!(!backup_path(&path).exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn splices_into_the_original_layout() {
        // Four-space indent, a one-line object and a key order of the client's own.
        let old = "{\n    \"files\": {\"a\": 1, \"b\": [2, 3]},\n    \"mcpServers\": {\n        \"other\": {\"command\": \"x\"}\n    },\n    \"z\": true\n}\n";
        let new = with_server(Some(old), Client::Cursor, "/p/mcp").unwrap().unwrap();
        assert_eq!(
            new,
            "{\n    \"files\": {\"a\": 1, \"b\": [2, 3]},\n    \"mcpServers\": {\n        \"other\": {\"command\": \"x\"},\n        \"pusmogo\": {\n            \"command\": \"/p/mcp\",\n            \"args\": []\n        }\n    },\n    \"z\": true\n}\n"
        );
        // The preview shows only our change.
        let d = diff(old, &new, 0);
        assert_eq!(d.lines().filter(|l| l.starts_with("- ")).count(), 1, "{d}");
        assert!(d.contains("-         \"other\": {\"command\": \"x\"}\n"), "{d}");
        // Undo gives the file back byte for byte.
        assert_eq!(without_server(Some(&new)).unwrap().unwrap(), old);
    }

    #[test]
    fn splices_compact_files_and_empty_or_missing_server_lists() {
        let old = r#"{"a":1,"mcpServers":{"x":{"command":"y"}}}"#;
        let new = with_server(Some(old), Client::Cursor, "/p").unwrap().unwrap();
        assert_eq!(new, r#"{"a":1,"mcpServers":{"x":{"command":"y"}, "pusmogo": {"command":"/p","args":[]}}}"#);
        assert_eq!(without_server(Some(&new)).unwrap().unwrap(), old);

        let old = "{\n  \"mcpServers\": {},\n  \"k\": 1\n}\n";
        let new = with_server(Some(old), Client::Cursor, "/p").unwrap().unwrap();
        assert_eq!(new, "{\n  \"mcpServers\": {\n    \"pusmogo\": {\n      \"command\": \"/p\",\n      \"args\": []\n    }\n  },\n  \"k\": 1\n}\n");
        assert_eq!(without_server(Some(&new)).unwrap().unwrap(), "{\n  \"mcpServers\": {},\n  \"k\": 1\n}\n");

        let old = "{\n\t\"k\": \"v\"\n}";
        let new = with_server(Some(old), Client::Cursor, "/p").unwrap().unwrap();
        assert_eq!(new, "{\n\t\"k\": \"v\",\n\t\"mcpServers\": {\n\t\t\"pusmogo\": {\n\t\t\t\"command\": \"/p\",\n\t\t\t\"args\": []\n\t\t}\n\t}\n}");
    }

    #[test]
    fn an_entry_for_a_moved_app_is_reported_as_stale() {
        let home = scratch("moved");
        let downloads = "/Users/x/Downloads/Pusmogo.app/Contents/MacOS/pusmogo-mcp";
        assert!(install(&home, Client::Cursor, downloads).unwrap());
        assert_eq!(installed(&home, Client::Cursor, CMD).unwrap(), Registration::OtherCommand);
        assert_eq!(installed(&home, Client::Cursor, downloads).unwrap(), Registration::Current);
        // Updating rewrites only the command, after which the entry is current.
        let d = preview_install(&home, Client::Cursor, CMD).unwrap().unwrap();
        assert_eq!(d.lines().filter(|l| l.starts_with('+') || l.starts_with('-')).count(), 2, "{d}");
        assert!(install(&home, Client::Cursor, CMD).unwrap());
        assert_eq!(installed(&home, Client::Cursor, CMD).unwrap(), Registration::Current);
        // An entry without a command string is not ours to call current.
        let odd = "{\"mcpServers\": {\"pusmogo\": {\"url\": \"http://x\"}}}";
        assert_eq!(registration(Some(odd), CMD).unwrap(), Registration::OtherCommand);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_moved_app_changes_only_the_command_line() {
        let first = with_server(Some("{\n  \"a\": {\"b\": 1}\n}\n"), Client::ClaudeCode, "/old/mcp").unwrap().unwrap();
        let moved = with_server(Some(&first), Client::ClaudeCode, "/new/mcp").unwrap().unwrap();
        let d = diff(&first, &moved, 0);
        assert_eq!(d, "  …\n-       \"command\": \"/old/mcp\",\n+       \"command\": \"/new/mcp\",\n  …\n");
        // Our entry first among several: its comma goes with it.
        let old = "{\"mcpServers\": {\"pusmogo\": {\"command\": \"/p\"}, \"b\": {}}}";
        assert_eq!(without_server(Some(old)).unwrap().unwrap(), "{\"mcpServers\": {\"b\": {}}}");
    }

    #[test]
    fn client_ids_round_trip() {
        for c in Client::ALL {
            assert_eq!(Client::from_id(c.id()), Some(c));
        }
        assert_eq!(Client::from_id("other"), None);
    }
}
