// C ABI of pusmogo-core. Keep in sync with core/crates/pusmogo-core/src/ffi.rs.
// Functions marked "Integration API" are not called by the macOS app; they are
// kept, stable, for other hosts and tests.
// A panic inside the core never unwinds into the caller: each function returns
// its documented error value instead (NULL, -1, or 0 where 0 is the safe default).
#ifndef PUSMOGO_H
#define PUSMOGO_H

#include <stddef.h>
#include <stdint.h>

typedef struct PusmogoEngine PusmogoEngine;

// model_path may be NULL or missing on disk: the stub engine is used.
PusmogoEngine *pusmogo_engine_new(const char *model_path);
void pusmogo_engine_free(PusmogoEngine *engine);
char *pusmogo_engine_name(const PusmogoEngine *engine);

// 1 when a speech engine was compiled in (it still needs its model file), 0 in a stub-only build.
int pusmogo_has_speech_engine(void);

// The model `language` ("hu"/"en") should load: 0 turbo, 1 large-v3, 2 Pusmogo HU,
// -1 when none is installed. preference: 0 auto, 1 fast, 2 most accurate, 3 Pusmogo HU;
// physical_memory in bytes. Hungarian defaults to Pusmogo HU, English to turbo;
// falls back to an installed model.
int pusmogo_model_pick(const char *language, int preference, uint64_t physical_memory,
                       int turbo_installed, int large_installed, int hu_installed);
// The model `language` should use when every model is on disk: 0 turbo, 1 large-v3, 2 Pusmogo HU.
int pusmogo_model_wanted(const char *language, int preference, uint64_t physical_memory);

// Runs the model once on synthetic audio (one-time GPU setup). 0 on success, -1 on error.
int pusmogo_engine_warm_up(PusmogoEngine *engine);

// Integration API. Adds from -> to to this engine only (not saved in a profile).
void pusmogo_dictionary_add(PusmogoEngine *engine, const char *from, const char *to);
// How many of the oldest dictionary spellings no longer fit the engine's prompt
// window (their replacements still apply). -1 when unknown (no tokenizer).
int pusmogo_dictionary_prompt_left_out(const PusmogoEngine *engine);
// Text around the cursor in the focused field, read just before a dictation.
// Name-like words from it steer the engine's spelling until the next call;
// NULL or "" clears them. Returns the number of hint words kept, -1 on a NULL
// engine. The text is not stored.
int pusmogo_set_context(PusmogoEngine *engine, const char *text);
// pusmogo_set_context for a code editor or terminal: identifiers in snake or
// constant case ("user_id") and file names with a known extension
// ("index.tsx") count as hints too. Same return value.
int pusmogo_set_code_context(PusmogoEngine *engine, const char *text);
// Opt-in local record of recent dictations: each finished dictation leaves its
// 16 kHz audio (<stamp>.wav) and the engine's raw and cleaned text, prompt and
// hint words (<stamp>.json) in dir; the newest 20 are kept. NULL or "" turns it
// off (the default). Returns 0, or -1 on a NULL engine.
int pusmogo_set_debug_dir(PusmogoEngine *engine, const char *dir);
// Drops spoken self-corrections ("háromkor, nem, négykor" -> "négykor") after
// cleanup: on = 1 (the default) or 0. Returns 0, or -1 on a NULL engine.
int pusmogo_set_self_corrections(PusmogoEngine *engine, int on);
// The last dictation's cleaned text before its self-corrections were dropped,
// for History. NULL when nothing was dropped. Free with pusmogo_string_free.
char *pusmogo_last_spoken(const PusmogoEngine *engine);

// samples: 16 kHz mono float32. lang: "hu", "en" or NULL for auto. NULL on error.
char *pusmogo_transcribe(PusmogoEngine *engine, const float *samples, size_t len, const char *lang);
// Streaming dictation: push audio while recording, step about once a second
// (transcribes a finished chunk, or previews the waiting audio for the
// caption), interrupt then finish on key up.
typedef struct Session PusmogoSession;
PusmogoSession *pusmogo_session_new(const char *lang);
void pusmogo_session_free(PusmogoSession *session);
void pusmogo_session_push(PusmogoSession *session, const float *samples, size_t len);
// Caption previews of audio not yet in a chunk: 1 = on, 0 = off (default).
// A release during a preview waits for it on Metal (docs/bench/caption-01.md).
void pusmogo_session_set_previews(PusmogoSession *session, int on);
// 1 = the caption text changed (chunk or preview), 0 = no change, -1 = error.
int pusmogo_session_step(PusmogoEngine *engine, PusmogoSession *session);
// Any thread: a caption preview in progress gives up and no new one starts
// until the next pusmogo_session_new. Previews never add to the final text.
// A chunk pass in progress gives up too when finish would redo its audio.
void pusmogo_session_interrupt(void);
// Cleaned text for the caption: chunks done so far plus the latest preview.
char *pusmogo_session_text(const PusmogoEngine *engine, const PusmogoSession *session);
// Cleaned text of the whole dictation. NULL on error.
char *pusmogo_session_finish(PusmogoEngine *engine, PusmogoSession *session);

// Integration API. Text cleanup alone: no engine, no dictionary.
char *pusmogo_clean_text(const char *text);
// The text with its lines joined by single spaces, for single-line fields.
char *pusmogo_single_line(const char *text);

/* Writing style per app (style.rs). Category of the frontmost app: 0 other,
   1 personal messages, 2 work messages, 3 email, 4 code (editors, IDEs,
   terminals); window_title is used for
   browsers only; overrides holds "bundle.id\tcategory" lines. Any may be NULL. */
int pusmogo_style_category(const char *bundle_id, const char *window_title, const char *overrides);
/* Applies a style to cleaned text: style 0 formal, 1 casual, 2 very casual;
   lang "hu" or "en". Email also lays out a spoken greeting and sign-off;
   code ignores style (no capital or final stop, straight quotes, spoken case
   commands: "camel case user id" -> userId).
   Free with pusmogo_string_free; NULL when text is NULL. */
char *pusmogo_style_apply(const char *text, int category, int style, const char *lang);
// Learning from edits: "wrong\tright" when the user replaced up to three words of
// the dictated text between the two reads of the field (before, after); else NULL.
char *pusmogo_learned_correction(const char *dictated, const char *before, const char *after);

// Profile: history, dictionary, settings and snippets in one local SQLite file.
// int results: 0 on success, -1 on error. JSON strings are freed with pusmogo_string_free.
typedef struct Profile PusmogoProfile;
// NULL on error (e.g. the file was written by a newer Pusmogo).
PusmogoProfile *pusmogo_profile_open(const char *path);
// Like pusmogo_profile_open, but a file that is not an SQLite database or is
// damaged is moved aside (<name>.broken-<unix secs>.<ext>) and a fresh profile
// is started. *moved_to (moved_to may be NULL) gets the old file's new path,
// freed with pusmogo_string_free, or NULL when nothing was moved. A profile
// from a newer Pusmogo is never moved: the result is NULL.
PusmogoProfile *pusmogo_profile_open_recovering(const char *path, char **moved_to);
// 1 when the file at path is a profile written by a newer Pusmogo (why
// pusmogo_profile_open_recovering returned NULL), else 0. Reads only.
int pusmogo_profile_is_newer(const char *path);
void pusmogo_profile_free(PusmogoProfile *profile);
// Returns the new entry id, or -1. app may be NULL.
long long pusmogo_profile_add_history(PusmogoProfile *profile, const char *lang, const char *text,
                                      long long latency_ms, const char *app);
// Newest first: [{"id","created_at","lang","text","latency_ms","app"}]. query NULL or "" = all.
char *pusmogo_profile_search_history(PusmogoProfile *profile, const char *query, size_t limit);
int pusmogo_profile_delete_history(PusmogoProfile *profile, long long id);
int pusmogo_profile_clear_history(PusmogoProfile *profile);
// Drops entries older than keep_days (0 keeps all). Rows removed, or -1.
long long pusmogo_profile_prune_history(PusmogoProfile *profile, long long keep_days);
// Integration API. {"dictations","words","median_latency_ms"}
char *pusmogo_profile_stats(PusmogoProfile *profile);
// Recording length of history entry id, for words per minute. 0, or -1.
int pusmogo_profile_set_audio_ms(PusmogoProfile *profile, long long id, long long audio_ms);
// The text of history entry id as spoken, before self-corrections. 0, or -1.
int pusmogo_profile_set_spoken(PusmogoProfile *profile, long long id, const char *spoken);
// An accepted correction wrong -> right, for the most-corrected insight. 0, or -1.
int pusmogo_profile_add_correction(PusmogoProfile *profile, const char *wrong, const char *right);
// A fix wrong -> right the user made after a dictation (not accepted). 1 when the
// same fix was seen before and right was just added to the dictionary, 0 when only
// recorded (or the dictionary already makes it), -1 on error.
int pusmogo_profile_note_correction(PusmogoProfile *profile, const char *wrong, const char *right);
// {"dictations","words","words_per_minute","current_streak","longest_streak","top_word",
//  "top_correction","peak_hour","top_app","title"}; days and hours in the system time zone
//  at each dictation's own date (summer time included); utc_offset_s is only a fallback.
char *pusmogo_profile_insights(PusmogoProfile *profile, long long utc_offset_s);
int pusmogo_profile_set_dictionary(PusmogoProfile *profile, const char *from, const char *to);
int pusmogo_profile_remove_dictionary(PusmogoProfile *profile, const char *from);
// [{"from","to"}] in the order added.
char *pusmogo_profile_dictionary(PusmogoProfile *profile);
// Replaces the engine's dictionary with the profile's (after load, reload and edits).
int pusmogo_profile_apply_dictionary(PusmogoProfile *profile, PusmogoEngine *engine);
// Snippets: a spoken trigger that inserts a saved text verbatim. Triggers compare
// without case, accents or punctuation. set: new or same-trigger id, or -1.
long long pusmogo_profile_set_snippet(PusmogoProfile *profile, const char *trigger, const char *text);
// -1 if the trigger is empty or belongs to another snippet.
int pusmogo_profile_update_snippet(PusmogoProfile *profile, long long id, const char *trigger, const char *text);
int pusmogo_profile_delete_snippet(PusmogoProfile *profile, long long id);
// [{"id","trigger","text","created_at"}] in the order added.
char *pusmogo_profile_snippets(PusmogoProfile *profile);
// Text to insert verbatim when the whole cleaned utterance is a trigger; NULL otherwise.
char *pusmogo_profile_snippet_for(PusmogoProfile *profile, const char *utterance);
// NULL if unset.
char *pusmogo_profile_setting(PusmogoProfile *profile, const char *key);
int pusmogo_profile_set_setting(PusmogoProfile *profile, const char *key, const char *value);
// Export writes a copy to a new file; import replaces the contents (nothing changes on error).
int pusmogo_profile_export(PusmogoProfile *profile, const char *path);
int pusmogo_profile_import(PusmogoProfile *profile, const char *path);

// Test-set recording mode (testset.rs): the built-in reading script, one take
// per line read normally, then the whisper lines whispered. Nothing is uploaded.
// Speaker folder under root (name reduced to letters, digits, '-', '_').
char *pusmogo_testset_speaker_dir(const char *root, const char *name);
// {"consent":null|"private"|"release","next":n,"takes":[{"id","lang","category","whispered","text","done"}]}
char *pusmogo_testset_status(const char *dir);
// Saves <id>.wav or <id>-whisper.wav (16 kHz mono) and its reference .txt.
// 0 saved, -1 bad argument or unknown take, -2 shorter than 0.3 s, -3 write failed.
int pusmogo_testset_save(const char *dir, const char *id, int whispered, const float *samples, size_t len);
// Writes consent.txt: level "private" or "release", date, initials or pseudonym. 0 or -1.
int pusmogo_testset_consent(const char *dir, const char *level, const char *date, const char *who);

// MCP config for local AI apps (mcp_config.rs). home = the user's home folder
// (a scratch folder in tests); client = "claude-desktop", "claude-code", "cursor".
// Only the "pusmogo" entry under mcpServers is touched; files are backed up first.
char *pusmogo_mcp_config_path(const char *home, const char *client);
// 1 installed for `command` (the bundled pusmogo-mcp), 2 installed for another
// path (a moved or deleted copy), 0 not, -1 bad argument, -2 unreadable JSON,
// -3 read failed.
int pusmogo_mcp_installed(const char *home, const char *client, const char *command);
// Unified diff of the install, "" when already installed, NULL on error.
char *pusmogo_mcp_preview(const char *home, const char *client, const char *command);
// 1 changed, 0 nothing to do, -1 bad argument, -2 unreadable file left as is, -3 write failed.
int pusmogo_mcp_install(const char *home, const char *client, const char *command);
int pusmogo_mcp_uninstall(const char *home, const char *client);

void pusmogo_string_free(char *s);

#endif
