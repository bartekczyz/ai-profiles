//! Per-app registry: the single source of truth describing each managed app
//! (Claude, ChatGPT) and every Claude-specific seam generalised behind it.
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    #[default]
    Claude,
    Codex,
}

/// One possible on-disk identity for this app's stock GUI bundle.
///
/// A vendor can rename their app bundle without changing what it does (see
/// `CODEX`'s doc comment) — a spec lists every name ai-profiles should
/// recognise, newest first, instead of hardcoding a single string that goes
/// stale the moment that happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiBundleCandidate {
    /// Stock bundle file name under `/Applications`, e.g. `"Claude.app"`.
    pub bundle_name: &'static str,
    /// Executable inside `Contents/MacOS`, used both to launch the bundle by
    /// absolute path and to single out its main process in `ps` output, e.g.
    /// `"Claude"`.
    pub macos_exec: &'static str,
}

/// Static description of one managed app. All fields are `&'static str` (plus
/// `has_usage`) so a spec lives in a `const` and is referenced without alloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSpec {
    /// Human-facing product name for the GUI/profile-kind identity — shown in
    /// the profile list, create-profile dialog, and any message about the
    /// desktop bundle. E.g. `"Claude"`, or `"ChatGPT"` for ChatGPT profiles: the
    /// unified desktop app Codex now ships inside (see [`GuiBundleCandidate`]).
    pub display_name: &'static str,
    /// Human-facing name for the CLI specifically, e.g. `"Claude"`. Diverges
    /// from `display_name` only for Codex (`"Codex"`), whose desktop app was
    /// folded into ChatGPT while the CLI kept its own identity.
    pub cli_display_name: &'static str,
    /// Possible on-disk identities of the stock GUI bundle, newest first. See
    /// [`GuiBundleCandidate`].
    pub gui_bundle_candidates: &'static [GuiBundleCandidate],
    /// Directory name under `~/Library/Application Support` holding the stock
    /// GUI data, e.g. `"Claude"`.
    pub gui_support_dir_name: &'static str,
    /// Prefix for generated launcher bundles: `"<prefix> (<name>).app"`.
    pub launcher_prefix: &'static str,
    /// Prefixes this app's generated launcher bundles used before a rename of
    /// `launcher_prefix`, so a bundle generated under the old prefix can still
    /// be found and cleaned up on the next `generate()`. Empty unless
    /// `launcher_prefix` has changed historically.
    pub legacy_launcher_prefixes: &'static [&'static str],
    /// Real CLI binary the wrapper execs, e.g. `"claude"`.
    pub cli_binary: &'static str,
    /// Prefix for generated CLI wrappers: `"<prefix>-<slug>"`.
    pub cli_wrapper_prefix: &'static str,
    /// Env var pointing the app at the profile's config home (its `cli-config`
    /// dir), e.g. `"CLAUDE_CONFIG_DIR"`. The CLI wrapper exports it, and so
    /// does the desktop launcher, so the desktop app and the CLI share one
    /// config home.
    ///
    /// Codex needs it for auth: ChatGPT.app reads its login from `CODEX_HOME`,
    /// and `--user-data-dir` alone only isolates the browser layer. Claude keeps
    /// its GUI auth in the `--user-data-dir`, but the Claude Code agent it runs
    /// (the Code tab) resolves its config home from `CLAUDE_CONFIG_DIR`. Without
    /// the export every Claude desktop profile shared the stock `~/.claude`:
    /// one session history, and one set of hooks, permissions, plugins and MCP
    /// servers across accounts.
    pub cli_config_env: &'static str,
    /// Stock CLI config directory name under `$HOME`, e.g. `".claude"`.
    pub cli_stock_config_dir_name: &'static str,
    /// Whether this app exposes account usage/quota stats.
    pub has_usage: bool,
    /// Config-dir entries a profile *inherits* from the stock install
    /// (`~/.claude`, `~/.codex`) instead of isolating, by way of a symlink
    /// created in the profile's `cli-config` dir.
    ///
    /// The isolation boundary is identity, not craft: profiles exist to
    /// separate account, auth, quota and history. These entries are inert
    /// content describing *how* the user works — skills, subagents, slash
    /// commands, rules, global instructions — so duplicating them per profile
    /// is upkeep with no security benefit. Desktop launchers link them too,
    /// since they point the desktop app's agent at the same `cli-config` dir.
    ///
    /// Deliberately excluded: `settings.json` (carries `hooks`, which run
    /// arbitrary shell, and `permissions` allowlists), `hooks/`, `plugins/`
    /// (absolute install paths, project-scoped entries, and mutable state that
    /// concurrently-running profiles would race on) and anything holding
    /// credentials or session history.
    ///
    /// [`cli_config_env`]: AppSpec::cli_config_env
    pub shared_surfaces: &'static [&'static str],
}

pub const CLAUDE: AppSpec = AppSpec {
    display_name: "Claude",
    cli_display_name: "Claude",
    gui_bundle_candidates: &[GuiBundleCandidate {
        bundle_name: "Claude.app",
        macos_exec: "Claude",
    }],
    gui_support_dir_name: "Claude",
    launcher_prefix: "Claude",
    legacy_launcher_prefixes: &[],
    cli_binary: "claude",
    cli_wrapper_prefix: "claude",
    cli_config_env: "CLAUDE_CONFIG_DIR",
    cli_stock_config_dir_name: ".claude",
    has_usage: true,
    shared_surfaces: &[
        "CLAUDE.md",
        "agents",
        "commands",
        "output-styles",
        "rules",
        "skills",
        "workflows",
    ],
};

pub const CODEX: AppSpec = AppSpec {
    // OpenAI folded the standalone Codex desktop app into ChatGPT (same
    // CFBundleIdentifier, renamed bundle/executable, confirmed no separate
    // Codex app is distributed any more — see learn.chatgpt.com/docs/app).
    // The CLI is unaffected and still called Codex, hence the display-name
    // split below.
    display_name: "ChatGPT",
    cli_display_name: "Codex",
    // Newest first so a not-yet-updated install of the old standalone app is
    // still recognised.
    gui_bundle_candidates: &[
        GuiBundleCandidate {
            bundle_name: "ChatGPT.app",
            macos_exec: "ChatGPT",
        },
        GuiBundleCandidate {
            bundle_name: "Codex.app",
            macos_exec: "Codex",
        },
    ],
    gui_support_dir_name: "Codex",
    launcher_prefix: "ChatGPT",
    legacy_launcher_prefixes: &["Codex"],
    cli_binary: "codex",
    cli_wrapper_prefix: "codex",
    cli_config_env: "CODEX_HOME",
    cli_stock_config_dir_name: ".codex",
    has_usage: true,
    shared_surfaces: &["AGENTS.md", "rules", "skills"],
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
        assert_eq!(
            codex.gui_bundle_candidates,
            &[
                GuiBundleCandidate {
                    bundle_name: "ChatGPT.app",
                    macos_exec: "ChatGPT",
                },
                GuiBundleCandidate {
                    bundle_name: "Codex.app",
                    macos_exec: "Codex",
                },
            ]
        );
        assert_eq!(codex.display_name, "ChatGPT");
        assert_eq!(codex.cli_display_name, "Codex");
        assert_eq!(codex.launcher_prefix, "ChatGPT");
        assert_eq!(codex.legacy_launcher_prefixes, &["Codex"]);
        assert_eq!(codex.cli_binary, "codex");
        assert_eq!(codex.cli_config_env, "CODEX_HOME");
        assert_eq!(codex.cli_stock_config_dir_name, ".codex");
        assert!(codex.has_usage);
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
