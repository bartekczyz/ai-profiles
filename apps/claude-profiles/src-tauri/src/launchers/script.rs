use crate::app_kind::AppSpec;

/// Generate the bash script body for `Contents/MacOS/launcher`. Execs
/// `open -n -a "<gui_app_name>"` with `--user-data-dir` pointed at the
/// per-profile gui-data directory (app-neutral path under claude-profiles/).
pub fn launcher_script(profile_id: &str, spec: &AppSpec) -> String {
    format!(
        r#"#!/bin/bash
# claude-profiles launcher — profile id: {profile_id}
DATA_DIR="$HOME/Library/Application Support/claude-profiles/profiles/{profile_id}/gui-data"
exec open -n -a "{app}" --args --user-data-dir="$DATA_DIR"
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
            "DATA_DIR=\"$HOME/Library/Application Support/claude-profiles/profiles/id-1/gui-data\""
        ));
    }

    #[test]
    fn script_starts_with_bash_shebang() {
        assert!(launcher_script("abc", &CLAUDE).starts_with("#!/bin/bash\n"));
    }

    #[test]
    fn script_has_marker_comment_for_safe_overwrite_detection() {
        assert!(launcher_script("abc", &CLAUDE).contains("# claude-profiles launcher"));
    }
}
