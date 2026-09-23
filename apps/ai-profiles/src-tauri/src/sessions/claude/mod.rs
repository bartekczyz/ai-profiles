//! Claude Code sessions.
//!
//! The CLI half of a session is Claude Code's own: a transcript at
//! `<config>/projects/<slug>/<id>.jsonl`, the folder `<slug>/<id>/` beside it
//! (subagent transcripts, saved tool results) and `<config>/file-history/<id>/`.
//! A running `claude` registers the session it has open in
//! `<config>/sessions/<pid>.json`.

pub mod live;
pub mod transcript;
