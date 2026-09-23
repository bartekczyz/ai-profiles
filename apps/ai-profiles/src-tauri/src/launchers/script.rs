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
/// For apps whose GUI reads auth from a config-home env var rather than from
/// the `--user-data-dir` (Codex, via `CODEX_HOME`), the script also exports
/// that env var at the profile's `cli-config` dir — the same home the CLI
/// wrapper and usage provider use. Without it the app falls back to the stock
/// home (`~/.codex`) and shows the default account regardless of
/// `--user-data-dir`. `open` propagates the exported environment to the
/// launched GUI app, so the export reaches it.
///
/// With `signed_copy`, the script opens the profile's
/// [signed copy](crate::launchers::signed_copy) instead, when there is one. It
/// finds the copy by looking, not by name, so a profile name never reaches the
/// shell. A copy it can't use as it is goes to ai-profiles instead, which
/// replaces it and opens it.
pub fn launcher_script(
    profile_id: &str,
    spec: &AppSpec,
    gui_app_target: &str,
    signed_copy: Option<&SignedCopy<'_>>,
) -> String {
    let profiles_base = "$HOME/Library/Application Support/ai-profiles/profiles";
    let config_home_export = if spec.gui_auth_via_config_env {
        format!(
            "CONFIG_DIR=\"{profiles_base}/{profile_id}/cli-config\"\nexport {env}=\"$CONFIG_DIR\"\n",
            env = spec.cli_config_env,
        )
    } else {
        String::new()
    };
    let target = if let Some(copy) = signed_copy {
        signed_copy_lookup(profiles_base, profile_id, gui_app_target, copy)
    } else {
        format!("APP=\"{gui_app_target}\"\n")
    };
    format!(
        r#"#!/bin/bash
# ai-profiles launcher — profile id: {profile_id}
DATA_DIR="{profiles_base}/{profile_id}/gui-data"
{config_home_export}{target}exec open -n -a "$APP" --args --user-data-dir="$DATA_DIR"
"#,
    )
}

/// What a launcher needs to tell whether a signed copy is fit to open, and
/// where to send one that isn't.
pub struct SignedCopy<'a> {
    /// The profile's color, which the copy's icon is in when it is current.
    /// Validated hex, so safe in the script as it is.
    pub color: &'a str,
    /// The ai-profiles executable, which replaces a stale copy when given
    /// [`profile_shim::OPEN_PROFILE_FLAG`] and the profile's id.
    pub host_binary: &'a str,
}

/// The lines that set `APP` to the profile's signed copy, or to the stock app
/// at `gui_app_target` while there is none. A copy whose icon is of another
/// color, or that lost its icon to an update, is handed to ai-profiles: only it
/// can replace the copy, which it does before opening it itself.
fn signed_copy_lookup(
    profiles_base: &str,
    profile_id: &str,
    gui_app_target: &str,
    copy: &SignedCopy<'_>,
) -> String {
    let dir = crate::launchers::signed_copy::DIR;
    let key_file = crate::launchers::signed_copy::ICON_KEY_FILE;
    let color = copy.color;
    let host = single_quoted(copy.host_binary);
    let flag = profile_shim::OPEN_PROFILE_FLAG;
    format!(
        r#"APP="{gui_app_target}"
COPY_DIR="{profiles_base}/{profile_id}/{dir}"
for COPY in "$COPY_DIR/"*.app; do
  if [ -d "$COPY" ]; then
    if [ "$(cat "$COPY_DIR/{key_file}" 2>/dev/null)" != "{color}" ] || [ ! -e "$COPY/Icon"$'\r' ]; then
      [ -x {host} ] && exec {host} {flag} "{profile_id}"
    fi
    APP="$COPY"
  fi
  break
done
"#
    )
}

/// `text` as one single-quoted shell word.
fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_kind::{CLAUDE, CODEX};

    #[test]
    fn script_uses_open_n_with_the_given_gui_app_target() {
        assert!(
            launcher_script("abc", &CLAUDE, "/Applications/Claude.app", None).contains(
                r#"APP="/Applications/Claude.app"
exec open -n -a "$APP" --args --user-data-dir="$DATA_DIR""#
            )
        );
        assert!(
            launcher_script("abc", &CODEX, "/Applications/ChatGPT.app", None).contains(
                r#"APP="/Applications/ChatGPT.app"
exec open -n -a "$APP" --args --user-data-dir="$DATA_DIR""#
            )
        );
    }

    #[test]
    fn script_includes_profile_id_in_data_dir() {
        assert!(
            launcher_script("id-1", &CLAUDE, "/Applications/Claude.app", None).contains(
                "DATA_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/id-1/gui-data\""
            )
        );
    }

    #[test]
    fn script_starts_with_bash_shebang() {
        assert!(
            launcher_script("abc", &CLAUDE, "/Applications/Claude.app", None)
                .starts_with("#!/bin/bash\n")
        );
    }

    #[test]
    fn script_has_marker_comment_for_safe_overwrite_detection() {
        assert!(
            launcher_script("abc", &CLAUDE, "/Applications/Claude.app", None)
                .contains("# ai-profiles launcher")
        );
    }

    #[test]
    fn codex_script_exports_config_home_at_profile_cli_config() {
        let script = launcher_script("abc", &CODEX, "/Applications/ChatGPT.app", None);
        assert!(script.contains(
            "CONFIG_DIR=\"$HOME/Library/Application Support/ai-profiles/profiles/abc/cli-config\""
        ));
        assert!(script.contains(r#"export CODEX_HOME="$CONFIG_DIR""#));
    }

    #[test]
    fn claude_script_does_not_export_a_config_home() {
        // Claude keeps GUI auth in --user-data-dir; exporting nothing keeps its
        // launcher byte-identical to the pre-Codex behaviour.
        let script = launcher_script("abc", &CLAUDE, "/Applications/Claude.app", None);
        assert!(!script.contains("export"));
        assert!(!script.contains("CONFIG_DIR"));
    }

    const COPY: SignedCopy<'static> = SignedCopy {
        color: "#7C3AED",
        host_binary: "/Applications/ai-profiles.app/Contents/MacOS/ai-profiles",
    };

    #[test]
    fn signed_copy_script_hands_a_copy_of_another_color_or_without_an_icon_to_ai_profiles() {
        let script = launcher_script("abc", &CLAUDE, "/Applications/Claude.app", Some(&COPY));
        assert!(script.contains(r##"[ "$(cat "$COPY_DIR/.icon" 2>/dev/null)" != "#7C3AED" ]"##));
        assert!(script.contains(r#"[ ! -e "$COPY/Icon"$'\r' ]"#));
        assert!(script.contains(
            r#"[ -x '/Applications/ai-profiles.app/Contents/MacOS/ai-profiles' ] && exec '/Applications/ai-profiles.app/Contents/MacOS/ai-profiles' --open-profile "abc""#
        ));
    }

    #[test]
    fn single_quoted_survives_a_quote_in_the_path() {
        assert_eq!(single_quoted("/a/it's/b"), r"'/a/it'\''s/b'");
    }

    #[test]
    fn signed_copy_script_opens_the_copy_it_finds_and_falls_back_to_the_stock_app() {
        let script = launcher_script("abc", &CLAUDE, "/Applications/Claude.app", Some(&COPY));
        assert!(script.contains(r#"APP="/Applications/Claude.app""#));
        assert!(script.contains(
            r#"COPY_DIR="$HOME/Library/Application Support/ai-profiles/profiles/abc/app.noindex""#
        ));
        assert!(script.contains(r#"APP="$COPY""#));
        assert!(script.contains(r#"exec open -n -a "$APP" --args --user-data-dir="$DATA_DIR""#));
    }

    #[test]
    fn stock_script_has_no_copy_lookup() {
        assert!(
            !launcher_script("abc", &CLAUDE, "/Applications/Claude.app", None).contains("COPY")
        );
    }
}
