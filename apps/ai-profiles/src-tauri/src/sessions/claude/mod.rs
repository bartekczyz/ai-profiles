//! Claude Code sessions.
//!
//! The CLI half of a session is Claude Code's own: a transcript at
//! `<config>/projects/<slug>/<id>.jsonl`, the folder `<slug>/<id>/` beside it
//! (subagent transcripts, saved tool results) and `<config>/file-history/<id>/`.
//! A running `claude` registers the session it has open in
//! `<config>/sessions/<pid>.json`. The desktop app's Code tab keeps a record
//! of each session it started, which names the transcript it continues.

pub mod archive;
pub mod archive_store;
pub mod desktop;
pub mod live;
pub mod markup;
pub mod ownership;
pub mod transcript;

use super::non_blank;
