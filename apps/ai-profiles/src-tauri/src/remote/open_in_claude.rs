//! Opening a remote session in the Claude app, signed in as its account.
//!
//! A session with Remote Control on has an id on claude.ai (`session_…`), and
//! the Claude desktop app opens `claude://code/<id>`. But it opens it as
//! whoever that app is signed in as, and here each profile's app is signed in
//! as someone else. So the link goes to the Claude profile on this Mac signed
//! in with the remote profile's email: to that profile's own app, by its
//! bundle, which is where LaunchServices delivers it. When several profiles
//! run from one app bundle (the stock Claude, without a Dock icon of their
//! own), LaunchServices can't be told which of them, and the link opens on
//! claude.ai instead; so does one with no matching profile here.

use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::launch;
use crate::launchers::plist;
use crate::paths::{profile_dir, resolve_gui_app, stock_gui_support_dir};
use crate::profiles::{self, Profile};

/// How long a profile started for a link is given to come up.
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a link was opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// The Claude app it opened in, `Claude (Work)`; `None` when it opened
    /// on claude.ai.
    pub app: Option<String>,
    /// The web link, when it opened on claude.ai.
    pub web: Option<String>,
    /// Why it opened on the web although a profile matched.
    pub note: Option<String>,
}

/// Pure: the app and web links for Remote Control id `bridge`, which must be
/// one the Claude app accepts: `session_…` or `cse_…`.
pub fn links(bridge: &str) -> AppResult<(String, String)> {
    let plain = bridge
        .strip_prefix("session_")
        .or_else(|| bridge.strip_prefix("cse_"))
        .is_some_and(|rest| {
            !rest.is_empty()
                && rest.len() <= 128
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        });
    if !plain {
        return Err(AppError::Validation(format!(
            "{bridge:?} isn't a Remote Control session id"
        )));
    }
    Ok((
        format!("claude://code/{bridge}"),
        format!("https://claude.ai/code/{bridge}"),
    ))
}

/// One running Claude app process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppProcess {
    pub pid: i32,
    /// The `.app` it runs from.
    pub bundle: String,
    /// Its `--user-data-dir`, which says whose profile it is; `None` for the
    /// stock app on its own data.
    pub data_dir: Option<String>,
}

/// Pure: the main processes of Claude apps in `ps` output (`pid command`),
/// the helpers left out: `…/X.app/Contents/MacOS/Claude`, or `Claude.bin` for
/// a wrapper, optionally ending in `--user-data-dir=<dir>`.
pub fn app_processes(ps_output: &str, exec: &str) -> Vec<AppProcess> {
    let wrapped = format!("{exec}.bin");
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            let command = command.trim();
            let at = command.find("/Contents/MacOS/")?;
            let bundle = &command[..at];
            let after = &command[at + "/Contents/MacOS/".len()..];
            let (name, args) = after.split_once(' ').unwrap_or((after, ""));
            if !bundle.ends_with(".app") || (name != exec && name != wrapped) {
                return None;
            }
            let args = args.trim();
            let data_dir = if args.is_empty() {
                None
            } else {
                Some(args.strip_prefix("--user-data-dir=")?.to_owned())
            };
            Some(AppProcess {
                pid: pid.parse().ok()?,
                bundle: bundle.to_owned(),
                data_dir,
            })
        })
        .collect()
}

/// Pure: whether `process`, a stock app process, runs on the stock app's own
/// data at `stock_data`: with no `--user-data-dir`, or one naming that same
/// folder (ai-profiles opens the default app that way).
fn is_default_data(process: &AppProcess, stock_data: &str) -> bool {
    process
        .data_dir
        .as_deref()
        .is_none_or(|dir| dir.trim_end_matches('/') == stock_data.trim_end_matches('/'))
}

/// A Claude app on this Mac signed in as some account.
enum Match {
    Profile(Box<Profile>),
    /// The stock app, on its own data.
    Default,
}

/// The Claude app signed in with `email`: a profile with a desktop app, else
/// the stock one.
fn matching(email: &str) -> AppResult<Option<Match>> {
    let same = |id: &str| match crate::accounts::read(id) {
        Ok(crate::accounts::AccountStatus::SignedIn { account }) => account
            .email
            .is_some_and(|found| found.eq_ignore_ascii_case(email)),
        _ => false,
    };
    for profile in profiles::load()? {
        if profile.app == AppKind::Claude && profile.surfaces.gui && same(&profile.id) {
            return Ok(Some(Match::Profile(Box::new(profile))));
        }
    }
    Ok(same("default:claude").then_some(Match::Default))
}

/// Open the remote session with Remote Control id `bridge` in the Claude app
/// signed in as `email`, starting that profile first if it isn't running; on
/// claude.ai when there's no such app, or it can't be told apart from others.
/// `version` is this app's, for starting a profile.
pub fn open_in_claude(email: Option<&str>, bridge: &str, version: &str) -> AppResult<Opened> {
    let (app_link, web_link) = links(bridge)?;
    let web = |note: Option<String>| -> AppResult<Opened> {
        open(None, &web_link)?;
        Ok(Opened {
            app: None,
            web: Some(web_link.clone()),
            note,
        })
    };
    let Some(found) = email.map(matching).transpose()?.flatten() else {
        return web(None);
    };
    let exec = AppKind::Claude.spec().gui_bundle_candidates[0].macos_exec;

    match found {
        Match::Profile(profile) => {
            let data_dir = profile_dir(&profile.id)?
                .join("gui-data")
                .display()
                .to_string();
            let label = plist::display_name(&profile);
            let mine = |processes: &[AppProcess]| {
                processes
                    .iter()
                    .find(|process| process.data_dir.as_deref() == Some(data_dir.as_str()))
                    .cloned()
            };
            let mut processes = app_processes(&launch::process_list()?, exec);
            if mine(&processes).is_none() {
                launch::open_profile(&profile, version, |_| {})?;
                let deadline = Instant::now() + START_TIMEOUT;
                while mine(&processes).is_none() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(300));
                    processes = app_processes(&launch::process_list()?, exec);
                }
            }
            let Some(process) = mine(&processes) else {
                return web(Some(format!(
                    "{label} didn't start, so it opened on claude.ai."
                )));
            };
            let sharing = processes
                .iter()
                .filter(|other| other.bundle == process.bundle)
                .count();
            if sharing > 1 {
                return web(Some(format!(
                    "{label} runs from the same app as another profile, so the link can't go to it alone. Turning on its own Dock icon gives it an app of its own."
                )));
            }
            open(Some(Path::new(&process.bundle)), &app_link)?;
            Ok(Opened {
                app: Some(label),
                web: None,
                note: None,
            })
        }
        Match::Default => {
            let stock = resolve_gui_app(AppKind::Claude.spec())
                .ok_or_else(|| AppError::Validation("Claude isn't installed".into()))?;
            let bundle = stock.bundle_path.display().to_string();
            let processes = app_processes(&launch::process_list()?, exec);
            let from_stock: Vec<&AppProcess> = processes
                .iter()
                .filter(|process| process.bundle == bundle)
                .collect();
            let stock_data = stock_gui_support_dir(AppKind::Claude.spec())?
                .display()
                .to_string();
            let only_default = from_stock
                .iter()
                .all(|process| is_default_data(process, &stock_data));
            if from_stock.len() > 1 || !only_default {
                return web(Some(
                    "Claude runs as more than one profile from the same app, so the link can't go to the default one alone.".to_owned(),
                ));
            }
            open(Some(&stock.bundle_path), &app_link)?;
            Ok(Opened {
                app: Some("Claude".to_owned()),
                web: None,
                note: None,
            })
        }
    }
}

/// `open [-a <app>] <url>`.
fn open(app: Option<&Path>, url: &str) -> AppResult<()> {
    let mut command = Command::new("/usr/bin/open");
    // Starting the app for the link, `open` would hand it ai-profiles' own
    // environment, and with it any config home ai-profiles was started with:
    // the stock app would run its Code tab on another profile's config.
    for kind in [AppKind::Claude, AppKind::Codex] {
        command.env_remove(kind.spec().cli_config_env);
    }
    if let Some(app) = app {
        command.arg("-a").arg(app);
    }
    let output = command.arg(url).output().map_err(AppError::Io)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "open couldn't open {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_only_the_ids_the_claude_app_takes() {
        assert_eq!(
            links("session_01CaBC-d_e").unwrap(),
            (
                "claude://code/session_01CaBC-d_e".to_owned(),
                "https://claude.ai/code/session_01CaBC-d_e".to_owned()
            )
        );
        assert!(links("cse_abc").is_ok());
        for bad in [
            "",
            "session_",
            "10ed5503-6b3a",
            "session_a/../b",
            "session_a?x=1",
            "local_abc",
        ] {
            assert!(links(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_default_app_is_the_one_on_the_stock_data_even_when_named() {
        let stock = "/Users/ada/Library/Application Support/Claude";
        let with = |dir: Option<&str>| AppProcess {
            pid: 1,
            bundle: "/Applications/Claude.app".into(),
            data_dir: dir.map(str::to_owned),
        };
        assert!(is_default_data(&with(None), stock));
        assert!(is_default_data(&with(Some(stock)), stock));
        assert!(is_default_data(
            &with(Some("/Users/ada/Library/Application Support/Claude/")),
            stock
        ));
        assert!(!is_default_data(&with(Some("/p/one/gui-data")), stock));
    }

    #[test]
    fn finds_each_apps_main_process_and_whose_it_is() {
        let ps = "\
  101 /Applications/Claude.app/Contents/MacOS/Claude
  102 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir=/p/one/gui-data
  103 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu --user-data-dir=/p/one/gui-data
  104 /p/two/app.noindex/Claude (Two).app/Contents/MacOS/Claude --user-data-dir=/p/two/Application Support/gui-data
  105 /Applications/Claude (Three).app/Contents/MacOS/Claude.bin --user-data-dir=/p/three/gui-data
  106 /bin/zsh
";
        let found = app_processes(ps, "Claude");
        assert_eq!(
            found,
            vec![
                AppProcess {
                    pid: 101,
                    bundle: "/Applications/Claude.app".into(),
                    data_dir: None
                },
                AppProcess {
                    pid: 102,
                    bundle: "/Applications/Claude.app".into(),
                    data_dir: Some("/p/one/gui-data".into())
                },
                AppProcess {
                    pid: 104,
                    bundle: "/p/two/app.noindex/Claude (Two).app".into(),
                    data_dir: Some("/p/two/Application Support/gui-data".into())
                },
                AppProcess {
                    pid: 105,
                    bundle: "/Applications/Claude (Three).app".into(),
                    data_dir: Some("/p/three/gui-data".into())
                },
            ]
        );
    }
}
