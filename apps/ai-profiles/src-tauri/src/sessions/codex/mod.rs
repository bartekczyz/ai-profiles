//! Codex sessions: the threads a `CODEX_HOME` holds, listed, archived,
//! restored and moved through `codex app-server`.
//!
//! Each thread is a rollout file, `<CODEX_HOME>/sessions/…/rollout-…-<id>.jsonl`
//! (or under `archived_sessions/` once archived), whose first line is a
//! `session_meta` record naming the client that started it. A process writing
//! to a thread holds `<CODEX_HOME>/thread-writer-locks/<id>.lock`.

mod actions;
mod list;
pub mod transfer;

#[cfg(test)]
mod fakes;

pub use actions::{apply, check};
pub use list::list;
