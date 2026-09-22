use crate::app_kind::AppSpec;

/// Generate the bash script body for `Contents/MacOS/launcher`. Execs
/// `open -n -a "<gui_app_target>"` with `--user-data-dir` pointed at the
/// per-profile gui-data directory (app-neutral path under ai-profiles/).
///
/// `gui_app_target` is the resolved stock GUI bundle to open — the caller
/// resolves it (see [`crate::paths::resolve_gui_app`]) so this stays a pure
/// function of its arguments rather than reaching into the filesystem
/// itself.
///
/// The script also exports the app's config-home env var
/// ([`AppSpec::cli_config_env`]) at the profile's `cli-config` dir — the same
/// home the CLI wrapper and usage provider use. Without it the app falls back
/// to the stock home: Codex (`CODEX_HOME`) shows the default account
/// regardless of `--user-data-dir`, and Claude's Code tab (`CLAUDE_CONFIG_DIR`)
/// shares `~/.claude`'s history, settings, plugins and MCP servers. `open`
/// propagates the exported environment to the launched GUI app, so the export
/// reaches it.
pub fn launcher_script(profile_id: &str, spec: &AppSpec, gui_app_target: &str) -> String {
    let profiles_base = "$HOME/Library/Application Support/ai-profiles/profiles";
    let config_home_export = format!(
        "CONFIG_DIR=\"{profiles_base}/{profile_id}/cli-config\"\nexport {env}=\"$CONFIG_DIR\"\n",
        env = spec.cli_config_env,
    );
    format!(
        r#"#!/bin/bash
# ai-profiles launcher — profile id: {profile_id}
DATA_DIR="{profiles_base}/{profile_id}/gui-data"
{config_home_export}exec open -n -a "{gui_app_target}" --args --user-data-dir="$DATA_DIR"
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_kind::{CLAUDE, CODEX};

    #[test]
    fn script_uses_open_n_with_the_given_gui_app_target() {
        assert!(
            launcher_script("abc", &CLAUDE, "/Applications/Claude.app").contains(
                r#"exec open -n -a "/Applications/Claude.app" --args --user-data-dir="$DATA_DIR""#
            )
        );
        assert!(
            launcher_script("abc", &CODEX, "/Applications/ChatGPT.app").contains(
                r#"exec open -n -a "/Applications/ChatGPT.app" --args --user-data-dir="$DATA_DIR""#
            )
        );
    }

    #[test]
    fn script_includes_profile_id_in_data_dir() {
        assert!(
            launcher_script("id-1", &CLAUDE, "/Applications/Claude.app").contains(
                "DATA_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/id-1/gui-data\""
            )
        );
    }

    #[test]
    fn script_starts_with_bash_shebang() {
        assert!(launcher_script("abc", &CLAUDE, "/Applications/Claude.app")
            .starts_with("#!/bin/bash\n"));
    }

    #[test]
    fn script_has_marker_comment_for_safe_overwrite_detection() {
        assert!(launcher_script("abc", &CLAUDE, "/Applications/Claude.app")
            .contains("# ai-profiles launcher"));
    }

    #[test]
    fn codex_script_exports_config_home_at_profile_cli_config() {
        let script = launcher_script("abc", &CODEX, "/Applications/ChatGPT.app");
        assert!(script.contains(
            "CONFIG_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/abc/cli-config\""
        ));
        assert!(script.contains(r#"export CODEX_HOME="$CONFIG_DIR""#));
    }

    #[test]
    fn claude_script_exports_config_dir_at_profile_cli_config() {
        // The desktop app's Code tab resolves its config home from
        // CLAUDE_CONFIG_DIR; without it every profile shared ~/.claude.
        let script = launcher_script("abc", &CLAUDE, "/Applications/Claude.app");
        assert!(script.contains(
            "CONFIG_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/abc/cli-config\""
        ));
        assert!(script.contains(r#"export CLAUDE_CONFIG_DIR="$CONFIG_DIR""#));
    }
}
