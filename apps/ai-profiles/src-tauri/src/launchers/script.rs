use crate::app_kind::AppSpec;

/// Generate the bash script body for `Contents/MacOS/launcher`. Execs
/// `open -n -a "<gui_app_name>"` with `--user-data-dir` pointed at the
/// per-profile gui-data directory (app-neutral path under ai-profiles/).
///
/// For apps whose GUI reads auth from a config-home env var rather than from
/// the `--user-data-dir` (Codex, via `CODEX_HOME`), the script also exports
/// that env var at the profile's `cli-config` dir — the same home the CLI
/// wrapper and usage provider use. Without it the app falls back to the stock
/// home (`~/.codex`) and shows the default account regardless of
/// `--user-data-dir`. `open` propagates the exported environment to the
/// launched GUI app, so the export reaches it.
pub fn launcher_script(profile_id: &str, spec: &AppSpec) -> String {
    let profiles_base = "$HOME/Library/Application Support/ai-profiles/profiles";
    let config_home_export = if spec.gui_auth_via_config_env {
        format!(
            "CONFIG_DIR=\"{profiles_base}/{profile_id}/cli-config\"\nexport {env}=\"$CONFIG_DIR\"\n",
            env = spec.cli_config_env,
        )
    } else {
        String::new()
    };
    format!(
        r#"#!/bin/bash
# ai-profiles launcher — profile id: {profile_id}
DATA_DIR="{profiles_base}/{profile_id}/gui-data"
{config_home_export}exec open -n -a "{app}" --args --user-data-dir="$DATA_DIR"
"#,
        app = spec.gui_app_name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_kind::{CLAUDE, CODEX};

    #[test]
    fn script_uses_open_n_with_gui_app_name_from_spec() {
        assert!(launcher_script("abc", &CLAUDE)
            .contains(r#"exec open -n -a "Claude" --args --user-data-dir="$DATA_DIR""#));
        assert!(launcher_script("abc", &CODEX)
            .contains(r#"exec open -n -a "Codex" --args --user-data-dir="$DATA_DIR""#));
    }

    #[test]
    fn script_includes_profile_id_in_data_dir() {
        assert!(launcher_script("id-1", &CLAUDE).contains(
            "DATA_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/id-1/gui-data\""
        ));
    }

    #[test]
    fn script_starts_with_bash_shebang() {
        assert!(launcher_script("abc", &CLAUDE).starts_with("#!/bin/bash\n"));
    }

    #[test]
    fn script_has_marker_comment_for_safe_overwrite_detection() {
        assert!(launcher_script("abc", &CLAUDE).contains("# ai-profiles launcher"));
    }

    #[test]
    fn codex_script_exports_config_home_at_profile_cli_config() {
        let script = launcher_script("abc", &CODEX);
        assert!(script.contains(
            "CONFIG_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/abc/cli-config\""
        ));
        assert!(script.contains(r#"export CODEX_HOME="$CONFIG_DIR""#));
    }

    #[test]
    fn claude_script_does_not_export_a_config_home() {
        // Claude keeps GUI auth in --user-data-dir; exporting nothing keeps its
        // launcher byte-identical to the pre-Codex behaviour.
        let script = launcher_script("abc", &CLAUDE);
        assert!(!script.contains("export"));
        assert!(!script.contains("CONFIG_DIR"));
    }
}
