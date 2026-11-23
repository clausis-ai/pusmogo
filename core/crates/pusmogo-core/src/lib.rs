//! pusmogo-core: everything that is product logic, with no GUI.
//!
//! The Swift app links this as a static library through the C ABI in `ffi`.
//! The MCP server and (later) the bench CLI use it as a normal Rust crate.

pub mod alphabet;
#[cfg(feature = "wav")]
pub mod audio;
pub mod backtrack;
pub mod cleanup;
pub mod code;
pub mod context;
pub mod debug;
pub mod edits;
pub mod engine;
pub mod ffi;
pub mod insights;
mod lang;
pub mod lists;
pub mod mcp_config;
pub mod models;
pub mod profile;
pub mod session;
pub mod snippets;
pub mod spell;
pub mod style;
pub mod testset;
pub mod vad;

pub use cleanup::{clean, Dictionary};
pub use engine::{Engine, EngineError, Language};
pub use profile::Profile;
pub use session::Session;

/// Audio contract between the app and the core: mono, 16 kHz, f32 in [-1, 1].
pub const SAMPLE_RATE: u32 = 16_000;
