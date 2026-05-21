/// Generate the bash script body that lives inside `Contents/MacOS/launcher`.
///
/// The script execs `open -n -a "Claude"` with `--user-data-dir` pointed at the
/// per-profile gui-data directory under `~/Library/Application Support/claude-profiles/`.
pub fn launcher_script(profile_id: &str) -> String {
    format!(
        r#"#!/bin/bash
# claude-profiles launcher — profile id: {profile_id}
DATA_DIR="$HOME/Library/Application Support/claude-profiles/profiles/{profile_id}/gui-data"
exec open -n -a "Claude" --args --user-data-dir="$DATA_DIR"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_starts_with_bash_shebang() {
        let script = launcher_script("abc-123");
        assert!(script.starts_with("#!/bin/bash\n"));
    }

    #[test]
    fn script_includes_profile_id_in_data_dir() {
        let script = launcher_script("11111111-2222-3333-4444-555555555555");
        assert!(script.contains(
            "DATA_DIR=\"$HOME/Library/Application Support/claude-profiles/profiles/11111111-2222-3333-4444-555555555555/gui-data\""
        ));
    }

    #[test]
    fn script_uses_open_n_with_user_data_dir() {
        let script = launcher_script("abc");
        assert!(script.contains(r#"exec open -n -a "Claude" --args --user-data-dir="$DATA_DIR""#));
    }

    #[test]
    fn script_has_marker_comment_for_safe_overwrite_detection() {
        let script = launcher_script("abc");
        assert!(script.contains("# claude-profiles launcher"));
    }
}
