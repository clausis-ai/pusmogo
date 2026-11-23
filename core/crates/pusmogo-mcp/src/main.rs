//! pusmogo-mcp: a stdio MCP server on top of pusmogo-core.
//!
//! Install in Claude Code:
//!   claude mcp add pusmogo -- /path/to/pusmogo-mcp
//! Model: set PUSMOGO_MODEL=/path/to/ggml-large-v3-turbo-q5_0.bin (build with --features whisper).
//!
//! Profile tools (`search_history`, `get_dictionary`, `list_snippets`) read the
//! app's local profile read-only (`PUSMOGO_PROFILE` overrides the path) and only
//! after the user turned sharing on in Pusmogo; both switches are off by default.
//!
//! Notes: Pusmogo has no notes feature yet, so no notes tool is listed and
//! nothing about notes is shared. When notes exist, their tool gets its own
//! share setting, off by default, like the two above.
//!
//! Hand-rolled JSON-RPC (newline-delimited) to stay tiny: one dependency (serde_json).

use std::io::{self, BufRead, Write};

use pusmogo_core::profile::{self, Profile};
use pusmogo_core::{audio, clean, engine, Dictionary, Engine, Language};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-06-18";

fn main() {
    let model = std::env::var("PUSMOGO_MODEL").ok();
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    serve(stdin.lock(), &mut stdout, &model);
}

/// The JSON-RPC read loop: one message per line until end of input. Lines are
/// read as bytes, so a line that is not UTF-8 gets a parse error and the server
/// keeps going instead of stopping silently.
fn serve(mut input: impl BufRead, out: &mut impl Write, model: &Option<String>) {
    let mut engine: Option<Box<dyn Engine>> = None; // loaded lazily on first transcription
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = match std::str::from_utf8(&buf) {
            Ok(l) => l,
            Err(e) => {
                write_msg(out, &error(Value::Null, -32700, &format!("parse error: not UTF-8 ({e})")));
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                write_msg(out, &error(Value::Null, -32700, &format!("parse error: {e}")));
                continue;
            }
        };
        // Notifications have no id and get no response.
        let Some(id) = msg.get("id").cloned() else { continue };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let reply = match method {
            "initialize" => ok(id, json!({
                "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "pusmogo", "version": env!("CARGO_PKG_VERSION") }
            })),
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match call_tool(name, &args, model, &mut engine) {
                    Ok(text) => ok(id, json!({ "content": [{ "type": "text", "text": text }] })),
                    Err(e) => ok(id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
                }
            }
            _ => error(id, -32601, &format!("method not found: {method}")),
        };
        write_msg(out, &reply);
    }
}

/// The tool list. A build without the speech engine (the copy bundled in the
/// app, kept small for its size budget) leaves out `transcribe_file`, which
/// would otherwise answer with the stub engine's placeholder text.
fn tools() -> Value {
    let mut list = all_tools();
    if !cfg!(feature = "whisper") {
        if let Value::Array(items) = &mut list {
            items.retain(|t| t["name"] != "transcribe_file");
        }
    }
    list
}

fn all_tools() -> Value {
    json!([
        {
            "name": "transcribe_file",
            "description": "Transcribe a WAV file (Hungarian and/or English) locally with Pusmogo and return cleaned text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to a .wav file" },
                    "language": { "type": "string", "enum": ["auto", "hu", "en"], "default": "auto" },
                    "dictionary": { "type": "string", "description": "Optional Pusmogo dictionary applied during cleanup: one entry per line, \"Spelling\" or \"spoken<TAB>Spelling\"" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "clean_text",
            "description": "Clean dictated Hungarian/English text the way Pusmogo does before typing it: remove hesitation sounds (ö, öö, um, uh), turn spoken punctuation and line-break commands (pont, vessző, kérdőjel, új sor / period, comma, new line) into marks, apply the optional dictionary spellings, and fix spacing and capitalisation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "dictionary": { "type": "string", "description": "Optional Pusmogo dictionary applied during cleanup: one entry per line, \"Spelling\" or \"spoken<TAB>Spelling\"" },
                    "backtrack": { "type": "boolean", "default": false, "description": "Also drop spoken self-corrections after cleanup (\"háromkor, nem, négykor\" -> \"négykor\"; the app does this by default)" },
                    "app": { "type": "string", "enum": ["other", "personal", "work", "email", "code"], "description": "Optional app category: apply that category's writing style after cleanup (email also lays out a spoken greeting and sign-off; code builds identifiers from spoken case commands)" },
                    "style": { "type": "string", "enum": ["formal", "casual", "very_casual"], "description": "Writing style for 'app' (default: the category's default)" },
                    "language": { "type": "string", "enum": ["hu", "en"], "default": "hu", "description": "Language for the style rules" }
                },
                "required": ["text"]
            }
        },
        {
            "name": "search_history",
            "description": "Search the user's recent Pusmogo dictations on this Mac, newest first (read-only). Works only when the user turned on sharing dictation history with AI apps in Pusmogo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "default": "", "description": "Text to look for (case-insensitive, accents kept); empty lists the newest" },
                    "limit": { "type": "integer", "default": 20, "minimum": 1, "maximum": MAX_HISTORY }
                }
            }
        },
        {
            "name": "get_dictionary",
            "description": "The user's Pusmogo dictionary: spellings it applies to dictated text, as {from, to} pairs (read-only). Works only when the user turned on sharing the dictionary and snippets with AI apps in Pusmogo.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "list_snippets",
            "description": "The user's Pusmogo snippets: a spoken trigger and the text it inserts (read-only). Works only when the user turned on sharing the dictionary and snippets with AI apps in Pusmogo.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn call_tool(
    name: &str,
    args: &Value,
    model: &Option<String>,
    engine_slot: &mut Option<Box<dyn Engine>>,
) -> Result<String, String> {
    match name {
        "clean_text" => {
            let text = args.get("text").and_then(Value::as_str).ok_or("missing 'text'")?;
            let mut cleaned = clean(text, &dictionary_arg(args));
            if args.get("backtrack").and_then(Value::as_bool).unwrap_or(false) {
                cleaned = pusmogo_core::backtrack::drop_self_corrections(&cleaned);
            }
            if let Some(app) = args.get("app").and_then(Value::as_str) {
                use pusmogo_core::style::{apply, Category, Style};
                let category = Category::from_name(app).ok_or("unknown 'app'")?;
                let style = match args.get("style").and_then(Value::as_str) {
                    Some(s) => Style::from_name(s).ok_or("unknown 'style'")?,
                    None => category.default_style(),
                };
                let lang = Language::from_code(args.get("language").and_then(Value::as_str).unwrap_or("hu"));
                cleaned = apply(&cleaned, category, style, lang);
            }
            Ok(cleaned)
        }
        "transcribe_file" if !cfg!(feature = "whisper") => {
            Err("transcribe_file needs a pusmogo-mcp built with the speech engine (cargo build --release -p pusmogo-mcp --features whisper)".into())
        }
        "transcribe_file" => {
            let path = args.get("path").and_then(Value::as_str).ok_or("missing 'path'")?;
            let lang = Language::from_code(args.get("language").and_then(Value::as_str).unwrap_or("auto"));
            let samples = audio::read_wav_16k_mono(path)?;
            if engine_slot.is_none() {
                *engine_slot = Some(engine::load(model.as_deref()).map_err(|e| e.to_string())?);
            }
            let engine = engine_slot.as_mut().expect("engine loaded above");
            let raw = pusmogo_core::session::transcribe_all(engine.as_mut(), &samples, lang, "").map_err(|e| e.to_string())?;
            Ok(clean(&raw, &dictionary_arg(args)))
        }
        "search_history" => {
            let p = shared_profile(Profile::SHARE_HISTORY_KEY, "history")?;
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, MAX_HISTORY) as usize;
            // Snippet expansions sit in the history as typed; they stay private
            // unless the dictionary and snippets are shared too.
            let rows = if p.shares(Profile::SHARE_DICTIONARY_KEY) {
                p.search_history(query, limit)
            } else {
                p.search_spoken_history(query, limit)
            }
            .map_err(|e| e.to_string())?;
            Ok(profile::history_json(&rows))
        }
        "get_dictionary" => {
            let p = shared_profile(Profile::SHARE_DICTIONARY_KEY, "dictionary and snippets")?;
            Ok(profile::dictionary_json(&p.dictionary_entries().map_err(|e| e.to_string())?))
        }
        "list_snippets" => {
            let p = shared_profile(Profile::SHARE_DICTIONARY_KEY, "dictionary and snippets")?;
            Ok(profile::snippets_json(&p.snippets().map_err(|e| e.to_string())?))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

/// Most history rows one call returns.
const MAX_HISTORY: u64 = 200;

/// Where the app keeps its profile; `PUSMOGO_PROFILE` overrides it.
fn profile_path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("PUSMOGO_PROFILE") {
        return Some(p.into());
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join("Library/Application Support/Pusmogo/profile.sqlite"))
}

/// Opens the profile read-only (opened per call, so a switch turned off in the
/// app applies to the next call) and checks the user shares `what`.
fn shared_profile(key: &str, what: &str) -> Result<Profile, String> {
    let path = profile_path().ok_or("no home folder")?;
    let p = Profile::open_read_only(&path)
        .map_err(|_| "Pusmogo's profile could not be read (not created yet, or written by a different Pusmogo version)".to_string())?;
    if !p.shares(key) {
        return Err(format!("Sharing the {what} with AI apps is off. The user can turn it on in Pusmogo Settings."));
    }
    Ok(p)
}

/// Reads the optional `dictionary` argument.
fn dictionary_arg(args: &Value) -> Dictionary {
    parse_dictionary(args.get("dictionary").and_then(Value::as_str).unwrap_or(""))
}

/// Parses dictionary lines in the same format as `pusmogo-bench --dict-file`:
/// "Spelling" or "spoken<TAB>Spelling", blank lines ignored.
fn parse_dictionary(text: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (from, to) = line.split_once('\t').unwrap_or((line, line));
        dict.add(from.trim(), to.trim());
    }
    dict
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn write_msg(out: &mut impl Write, msg: &Value) {
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_is_listed_only_with_the_speech_engine() {
        let names: Vec<String> = tools().as_array().unwrap().iter()
            .map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().any(|n| n == "clean_text"));
        assert!(names.iter().any(|n| n == "search_history"));
        assert_eq!(names.iter().any(|n| n == "transcribe_file"), cfg!(feature = "whisper"));
        if !cfg!(feature = "whisper") {
            let err = call_tool("transcribe_file", &json!({"path": "/nonexistent.wav"}), &None, &mut None).unwrap_err();
            assert!(err.contains("speech engine"), "{err}");
        }
    }

    /// Notes are a placeholder: no tool may offer them before the feature and
    /// its own share switch exist.
    #[test]
    fn no_notes_tool_until_notes_exist() {
        for tool in all_tools().as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            assert!(!name.contains("note"), "{name}");
        }
        assert!(call_tool("list_notes", &json!({}), &None, &mut None).unwrap_err().contains("unknown tool"));
    }

    #[test]
    fn dictionary_argument_applies_during_cleanup() {
        let dict = parse_dictionary("acme inc\tAcme Inc.\n\nvs code\tVS Code\n");
        assert_eq!(clean("acme inc", &dict), "Acme Inc.");
        assert_eq!(clean("nyisd meg a vs code-ot", &dict), "Nyisd meg a VS Code-ot");
    }

    /// The profile tools against a scratch profile; one test, because they
    /// read `PUSMOGO_PROFILE` from the shared process environment.
    #[test]
    fn profile_tools_read_only_what_the_user_shares() {
        let path = std::env::temp_dir().join(format!("pusmogo-mcp-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("PUSMOGO_PROFILE", &path);
        let call = |name: &str, args: Value| call_tool(name, &args, &None, &mut None);

        assert!(call("search_history", json!({})).unwrap_err().contains("could not be read"));
        assert!(!path.exists(), "a missing profile is not created");

        let app = Profile::open(&path).unwrap();
        app.add_history_at(100, "hu", "Holnap háromkor találkozunk.", 800, "com.apple.TextEdit").unwrap();
        app.add_history_at(200, "en", "See you \"tomorrow\".", 700, "").unwrap();
        app.set_dictionary_entry("vs code", "VS Code").unwrap();
        app.set_snippet("címem", "1051 Budapest, Fő utca 1.").unwrap();
        app.add_history_at(150, "hu", "1051 Budapest, Fő utca 1.", 0, "com.apple.TextEdit").unwrap();

        for tool in ["search_history", "get_dictionary", "list_snippets"] {
            assert!(call(tool, json!({})).unwrap_err().contains("is off"), "{tool} is off by default");
        }

        app.set_setting(Profile::SHARE_HISTORY_KEY, "1").unwrap();
        let all: Value = serde_json::from_str(&call("search_history", json!({})).unwrap()).unwrap();
        assert_eq!(all.as_array().unwrap().len(), 2);
        assert_eq!(all[0]["text"], "See you \"tomorrow\".", "newest first, quotes survive JSON");
        let hit: Value = serde_json::from_str(&call("search_history", json!({ "query": "holnap", "limit": 5 })).unwrap()).unwrap();
        assert_eq!(hit[0]["app"], "com.apple.TextEdit");
        let one: Value = serde_json::from_str(&call("search_history", json!({ "limit": 0 })).unwrap()).unwrap();
        assert_eq!(one.as_array().unwrap().len(), 1, "limit is clamped to at least 1");
        assert!(call("get_dictionary", json!({})).is_err(), "history sharing does not open the dictionary");
        let addr: Value = serde_json::from_str(&call("search_history", json!({ "query": "budapest" })).unwrap()).unwrap();
        assert!(addr.as_array().unwrap().is_empty(), "a snippet expansion stays private while snippets are not shared");

        app.set_setting(Profile::SHARE_DICTIONARY_KEY, "1").unwrap();
        let dict: Value = serde_json::from_str(&call("get_dictionary", json!({})).unwrap()).unwrap();
        assert_eq!(dict, json!([{ "from": "vs code", "to": "VS Code" }]));
        let snips: Value = serde_json::from_str(&call("list_snippets", json!({})).unwrap()).unwrap();
        assert_eq!(snips[0]["text"], "1051 Budapest, Fő utca 1.");
        let all: Value = serde_json::from_str(&call("search_history", json!({})).unwrap()).unwrap();
        assert_eq!(all.as_array().unwrap().len(), 3, "with snippets shared, their expansions show in the history too");

        app.set_setting(Profile::SHARE_HISTORY_KEY, "0").unwrap();
        assert!(call("search_history", json!({})).is_err(), "turning the switch off applies to the next call");

        drop(app);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn a_line_that_is_not_utf8_gets_a_parse_error_and_the_server_goes_on() {
        let mut input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec();
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"clean_text\",\"arguments\":{\"text\":\"caf\xE9\"}}}\n");
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}");
        let mut out = Vec::new();
        serve(&input[..], &mut out, &None);
        let replies: Vec<Value> = String::from_utf8(out).unwrap().lines()
            .map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(replies.len(), 3, "{replies:?}");
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[1]["error"]["code"], -32700);
        assert_eq!(replies[1]["id"], Value::Null);
        assert_eq!(replies[2]["id"], 3, "a last line without a newline is still answered");
    }

    #[test]
    fn missing_dictionary_argument_is_empty() {
        let args = json!({ "text": "acme inc" });
        assert_eq!(clean("acme inc", &dictionary_arg(&args)), "Acme inc");
    }
}
