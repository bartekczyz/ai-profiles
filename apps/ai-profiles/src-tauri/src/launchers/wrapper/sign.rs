//! Running `codesign`: reading what a vendor app is signed with, and signing and
//! checking the wrapper.

use std::ffi::OsString;
use std::io::Cursor;
use std::path::Path;
use std::process::{Command, Output};

use plist::{Dictionary, Value};

use crate::error::{AppError, AppResult};

const CODESIGN: &str = "/usr/bin/codesign";

/// What `codesign -d` reports about a signed bundle or binary.
pub struct Signature {
    /// The entitlements it is signed with; empty if it has none.
    pub entitlements: Dictionary,
    /// The team that signed it, or `None` for an ad-hoc signature.
    pub team_id: Option<String>,
}

/// Read the entitlements and team of the signature on `path`.
pub fn inspect(path: &Path) -> AppResult<Signature> {
    let output = Command::new(CODESIGN)
        .args(["-d", "--verbose=2", "--entitlements", ":-"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(tool_failure("reading the signature", path, &output));
    }

    let entitlements = if output.stdout.iter().all(u8::is_ascii_whitespace) {
        Dictionary::new()
    } else {
        Value::from_reader(Cursor::new(&output.stdout))
            .ok()
            .and_then(Value::into_dictionary)
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "the entitlements of {} are not a plist dictionary",
                    path.display()
                ))
            })?
    };
    Ok(Signature {
        entitlements,
        team_id: parse_team_identifier(&String::from_utf8_lossy(&output.stderr)),
    })
}

/// Ad-hoc sign `target` with hardened runtime and the entitlements in the file
/// at `entitlements`, replacing whatever signature it has.
pub fn sign(target: &Path, entitlements: &Path) -> AppResult<()> {
    let args = sign_args(target, entitlements);
    if let Some(flag) = forbidden_flag(&args) {
        return Err(AppError::Validation(format!(
            "refusing to run codesign with {flag}: it produces a bundle the kernel kills on launch"
        )));
    }
    let output = Command::new(CODESIGN).args(&args).output()?;
    if !output.status.success() {
        return Err(tool_failure("signing", target, &output));
    }
    Ok(())
}

/// Check that the signature on `target` is intact and that nothing in it is
/// unsealed.
pub fn verify(target: &Path) -> AppResult<()> {
    let output = Command::new(CODESIGN)
        .args(["--verify", "--strict"])
        .arg(target)
        .output()?;
    if !output.status.success() {
        return Err(tool_failure("verifying", target, &output));
    }
    Ok(())
}

/// The `codesign` arguments that sign `target` for use in a wrapper.
fn sign_args(target: &Path, entitlements: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["--force", "--sign", "-", "--options", "runtime"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.push("--entitlements".into());
    args.push(entitlements.into());
    args.push(target.into());
    args
}

/// A flag that must never reach `codesign` here, if `args` has one.
///
/// `--deep` and `--preserve-metadata=requirements` are the two the widely
/// copied recipes reach for. Both produce a bundle that is killed on launch
/// (`exit 137`, no message): the preserved designated requirement names the
/// vendor's certificate, which an ad-hoc signature can never satisfy, and
/// `--deep` re-signs the vendor's nested code.
fn forbidden_flag(args: &[OsString]) -> Option<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .find(|arg| arg == "--deep" || arg.starts_with("--preserve-metadata"))
        .map(|arg| arg.into_owned())
}

/// The team in `codesign -d --verbose=2` output (`TeamIdentifier=ABCDE12345`),
/// or `None` if the signature has none.
fn parse_team_identifier(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|team| !team.is_empty() && *team != "not set")
        .map(str::to_owned)
}

fn tool_failure(action: &str, path: &Path, output: &Output) -> AppError {
    AppError::Io(std::io::Error::other(format!(
        "codesign failed {action} {} ({}): {}",
        path.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn sign_args_are_ad_hoc_hardened_runtime_with_the_entitlements_file() {
        let args = sign_args(
            Path::new("/x/Claude.bin"),
            Path::new("/x/entitlements.plist"),
        );

        assert_eq!(
            args,
            [
                "--force",
                "--sign",
                "-",
                "--options",
                "runtime",
                "--entitlements",
                "/x/entitlements.plist",
                "/x/Claude.bin",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn sign_args_never_include_the_flags_that_get_the_wrapper_killed() {
        let args = sign_args(
            &PathBuf::from("/Applications/Claude (Work).app"),
            &PathBuf::from("/tmp/e.plist"),
        );

        assert_eq!(forbidden_flag(&args), None);
    }

    #[test]
    fn forbidden_flag_catches_deep_and_preserve_metadata() {
        let deep = ["--force", "--deep"].map(OsString::from);
        assert_eq!(forbidden_flag(&deep).as_deref(), Some("--deep"));

        let preserve = ["--preserve-metadata=entitlements,requirements"].map(OsString::from);
        assert_eq!(
            forbidden_flag(&preserve).as_deref(),
            Some("--preserve-metadata=entitlements,requirements")
        );
        let bare = ["--preserve-metadata"].map(OsString::from);
        assert!(forbidden_flag(&bare).is_some());
    }

    #[test]
    fn sign_refuses_to_run_with_a_forbidden_flag_in_its_arguments() {
        // A target that looks like a flag is the one way one could get in.
        let error = sign(Path::new("--deep"), Path::new("/tmp/e.plist")).unwrap_err();

        assert!(error.to_string().contains("--deep"));
    }

    #[test]
    fn team_identifier_is_read_from_codesign_output() {
        let stderr = "Executable=/Applications/Claude.app/Contents/MacOS/Claude\n\
                      Identifier=com.anthropic.claudefordesktop\n\
                      TeamIdentifier=Q6L2SF6YDW\n\
                      Sealed Resources version=2 rules=13 files=100\n";

        assert_eq!(parse_team_identifier(stderr).as_deref(), Some("Q6L2SF6YDW"));
    }

    #[test]
    fn team_identifier_is_none_for_an_ad_hoc_signature() {
        assert_eq!(parse_team_identifier("TeamIdentifier=not set\n"), None);
        assert_eq!(parse_team_identifier("Identifier=x\n"), None);
        assert_eq!(parse_team_identifier(""), None);
    }
}
