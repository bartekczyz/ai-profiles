//! Per-app registry: the single source of truth describing each managed app
//! (Claude, Codex) and every Claude-specific seam generalised behind it.
//!
//! Every path, launcher, and launch incantation that used to hardcode
//! "Claude" reads from the `AppSpec` returned by [`spec`]. Adding a new app
//! becomes a const here plus a variant on [`AppKind`], not a copy of call sites.

use serde::{Deserialize, Serialize};

/// Which managed application a profile (or synthetic default entry) belongs to.
///
/// Serialises lowercase (`"claude"`, `"codex"`) to match the TypeScript
/// `AppId` union and the `default:<app>` synthetic-id convention. Defaults to
/// [`AppKind::Claude`] so `profiles.json` documents predating the field
/// deserialise as Claude.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    #[default]
    Claude,
    Codex,
}

/// Static description of one managed app. All fields are `&'static str` (plus
/// `has_usage`) so a spec lives in a `const` and is referenced without alloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSpec {
    /// Human-facing product name, e.g. `"Claude"`.
    pub display_name: &'static str,
    /// Name passed to `open -a <name>`, e.g. `"Claude"`.
    pub gui_app_name: &'static str,
    /// Stock bundle file name under `/Applications`, e.g. `"Claude.app"`.
    pub gui_bundle_name: &'static str,
    /// Directory name under `~/Library/Application Support` holding the stock
    /// GUI data, e.g. `"Claude"`.
    pub gui_support_dir_name: &'static str,
    /// Executable inside `Contents/MacOS`, used to single out the main process
    /// in `ps` output, e.g. `"Claude"`.
    pub gui_macos_exec: &'static str,
    /// Prefix for generated launcher bundles: `"<prefix> (<name>).app"`.
    pub launcher_prefix: &'static str,
    /// Real CLI binary the wrapper execs, e.g. `"claude"`.
    pub cli_binary: &'static str,
    /// Prefix for generated CLI wrappers: `"<prefix>-<slug>"`.
    pub cli_wrapper_prefix: &'static str,
    /// Env var the wrapper exports to point the CLI at the per-profile config
    /// dir, e.g. `"CLAUDE_CONFIG_DIR"`.
    pub cli_config_env: &'static str,
    /// Stock CLI config directory name under `$HOME`, e.g. `".claude"`.
    pub cli_stock_config_dir_name: &'static str,
    /// Whether this app exposes account usage/quota stats.
    pub has_usage: bool,
    /// Whether the GUI app reads its account/auth from the [`cli_config_env`]
    /// config home rather than from its Chromium `--user-data-dir`. Codex keeps
    /// auth in `CODEX_HOME` (default `~/.codex`), so isolating its GUI per
    /// profile requires the launcher to export that env var at the profile's
    /// `cli-config` dir; `--user-data-dir` alone only isolates the browser
    /// layer. Claude stores its GUI auth inside the `--user-data-dir`, so it
    /// exports nothing.
    ///
    /// [`cli_config_env`]: AppSpec::cli_config_env
    pub gui_auth_via_config_env: bool,
}

pub const CLAUDE: AppSpec = AppSpec {
    display_name: "Claude",
    gui_app_name: "Claude",
    gui_bundle_name: "Claude.app",
    gui_support_dir_name: "Claude",
    gui_macos_exec: "Claude",
    launcher_prefix: "Claude",
    cli_binary: "claude",
    cli_wrapper_prefix: "claude",
    cli_config_env: "CLAUDE_CONFIG_DIR",
    cli_stock_config_dir_name: ".claude",
    has_usage: true,
    gui_auth_via_config_env: false,
};

pub const CODEX: AppSpec = AppSpec {
    display_name: "Codex",
    gui_app_name: "Codex",
    gui_bundle_name: "Codex.app",
    gui_support_dir_name: "Codex",
    gui_macos_exec: "Codex",
    launcher_prefix: "Codex",
    cli_binary: "codex",
    cli_wrapper_prefix: "codex",
    cli_config_env: "CODEX_HOME",
    cli_stock_config_dir_name: ".codex",
    has_usage: true,
    gui_auth_via_config_env: true,
};

/// Borrow the static [`AppSpec`] for a kind.
pub fn spec(kind: AppKind) -> &'static AppSpec {
    match kind {
        AppKind::Claude => &CLAUDE,
        AppKind::Codex => &CODEX,
    }
}

impl AppKind {
    /// Convenience accessor for this kind's [`AppSpec`].
    pub fn spec(self) -> &'static AppSpec {
        spec(self)
    }

    /// Lowercase token used in serialisation and synthetic ids.
    pub fn as_str(self) -> &'static str {
        match self {
            AppKind::Claude => "claude",
            AppKind::Codex => "codex",
        }
    }

    /// Parse a lowercase app token (`"claude"`/`"codex"`).
    pub fn from_token(token: &str) -> Option<AppKind> {
        match token {
            "claude" => Some(AppKind::Claude),
            "codex" => Some(AppKind::Codex),
            _ => None,
        }
    }

    /// Parse a synthetic default-entry id of the form `default:<app>`. Returns
    /// `None` for managed ids or unknown apps.
    pub fn from_default_id(id: &str) -> Option<AppKind> {
        id.strip_prefix("default:").and_then(AppKind::from_token)
    }
}

/// Format the synthetic default-entry id for a kind, e.g. `"default:claude"`.
// Consumed by later phases (per-app default entries); the round-trip test
// covers it today.
#[allow(dead_code)]
pub fn default_id(kind: AppKind) -> String {
    format!("default:{}", kind.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kind_is_claude() {
        assert_eq!(AppKind::default(), AppKind::Claude);
    }

    #[test]
    fn codex_spec_exposes_codex_surface() {
        let codex = spec(AppKind::Codex);
        assert_eq!(codex.gui_bundle_name, "Codex.app");
        assert_eq!(codex.cli_binary, "codex");
        assert_eq!(codex.cli_config_env, "CODEX_HOME");
        assert_eq!(codex.cli_stock_config_dir_name, ".codex");
        assert!(codex.has_usage);
        // Codex auth lives in CODEX_HOME, so its GUI launcher must export it;
        // Claude keeps GUI auth in the user-data-dir and exports nothing.
        assert!(codex.gui_auth_via_config_env);
        assert!(!CLAUDE.gui_auth_via_config_env);
    }

    #[test]
    fn default_id_round_trips() {
        for kind in [AppKind::Claude, AppKind::Codex] {
            assert_eq!(AppKind::from_default_id(&default_id(kind)), Some(kind));
        }
    }

    #[test]
    fn from_default_id_rejects_non_default_and_unknown() {
        assert_eq!(AppKind::from_default_id("some-managed-uuid"), None);
        assert_eq!(AppKind::from_default_id("default:gemini"), None);
        assert_eq!(AppKind::from_default_id("claude"), None);
    }

    #[test]
    fn app_kind_serialises_lowercase() {
        assert_eq!(serde_json::to_string(&AppKind::Codex).unwrap(), "\"codex\"");
        let parsed: AppKind = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(parsed, AppKind::Codex);
    }
}
