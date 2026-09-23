//! Local coding sessions kept by each profile: Claude Code's and Codex's.
//!
//! A [`Home`] is one place sessions live, a managed profile or an app's stock
//! install ("Default"). The per-app submodules read the sessions a home holds
//! from the apps' own files, which are undocumented internals that can change
//! between versions, so everything here reads leniently and skips what it
//! cannot make sense of rather than failing the whole listing.

mod claude;
mod home;

use std::path::PathBuf;

use crate::app_kind::AppKind;

#[allow(unused_imports)] // Consumed by the sessions commands.
pub use home::{home_for, homes_of};

/// Where one profile, or one app's stock install, keeps its sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    /// The profile's id, or `default:<app>` for the stock install.
    pub id: String,
    /// The app whose sessions live here.
    pub app: AppKind,
    /// The profile's name, or the stock install's display name (the one the
    /// user gave it, else "Default").
    pub label: String,
    /// The CLI's config dir: `CLAUDE_CONFIG_DIR` for Claude, `CODEX_HOME` for
    /// Codex. The CLI half of every session lives here.
    pub config_dir: PathBuf,
    /// The desktop app's `--user-data-dir`, or its stock Application Support
    /// dir for the stock install.
    pub gui_data_dir: PathBuf,
    /// The stock install rather than a managed profile.
    pub stock: bool,
}
