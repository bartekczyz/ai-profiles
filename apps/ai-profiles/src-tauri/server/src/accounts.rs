//! The Claude accounts on this machine, found the way claudemulti finds them:
//! every folder under the accounts base (`~/.claude-accounts` unless
//! configured) is one `CLAUDE_CONFIG_DIR`, dot-folders excepted, and
//! `~/.claude` is the account `default` when it exists and is enabled.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ai_profiles_core::account::{account_from_claude_json, ProfileAccount};
use serde_json::Value;

use crate::config::Config;

pub const DEFAULT_NAME: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDir {
    pub name: String,
    pub dir: PathBuf,
    pub is_default: bool,
}

impl AccountDir {
    /// `.claude.json`: inside the config dir for an account, in `$HOME` for the
    /// stock install (which keeps a copy inside too on some versions).
    fn claude_json_candidates(&self, home: &Path) -> Vec<PathBuf> {
        let mut candidates = vec![self.dir.join(".claude.json")];
        if self.is_default {
            candidates.push(home.join(".claude.json"));
        }
        candidates
    }

    /// Whose account this is, from what Claude recorded when it signed in.
    pub fn account(&self, home: &Path) -> Option<ProfileAccount> {
        self.claude_json_candidates(home).iter().find_map(|path| {
            let text = fs::read_to_string(path).ok()?;
            account_from_claude_json(&serde_json::from_str::<Value>(&text).ok()?)
        })
    }

    /// It holds a login that hasn't run out: the file Claude keeps its OAuth
    /// tokens in, unless the sign-in it records has expired.
    pub fn signed_in(&self) -> bool {
        self.dir.join(".credentials.json").is_file()
            && self
                .signed_in_until()
                .is_none_or(|until| until > now_millis())
    }

    /// When the sign-in runs out, in milliseconds since the epoch: the expiry
    /// of the refresh token Claude renews its access token with, as
    /// `.credentials.json` records it. The access token itself lasts hours
    /// and renews while Claude runs; this is how long that can go on.
    pub fn signed_in_until(&self) -> Option<u64> {
        let text = fs::read_to_string(self.dir.join(".credentials.json")).ok()?;
        serde_json::from_str::<Value>(&text)
            .ok()?
            .get("claudeAiOauth")?
            .get("refreshTokenExpiresAt")?
            .as_u64()
    }

    /// Mark a signed-in account's first-run setup as done, as it is once
    /// someone has been through it in a terminal.
    ///
    /// `claude auth login` stores the login but leaves the setup undone, and
    /// the setup (theme, then a login method, whatever is stored) comes up in
    /// the account's first session: in a tmux window, where nobody is
    /// looking, and ahead of Remote Control. Only an account that is signed
    /// in and hasn't got `hasCompletedOnboarding` is touched (everything else
    /// in its `.claude.json` is kept), and never `default`, which is the
    /// stock install someone set up by hand. Returns whether it changed.
    pub fn finish_setup(
        &self,
        claude_version: impl FnOnce() -> Option<String>,
    ) -> io::Result<bool> {
        use std::os::unix::fs::PermissionsExt;
        if self.is_default || !self.signed_in() {
            return Ok(false);
        }
        let path = self.dir.join(".claude.json");
        let mut config = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<Value>(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Value::Object(Default::default()),
            Err(err) => return Err(err),
        };
        let Some(fields) = config.as_object_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                ".claude.json isn't an object",
            ));
        };
        if fields.get("hasCompletedOnboarding") == Some(&Value::Bool(true)) {
            return Ok(false);
        }
        fields.insert("hasCompletedOnboarding".into(), Value::Bool(true));
        if let (false, Some(version)) = (
            fields.contains_key("lastOnboardingVersion"),
            claude_version(),
        ) {
            fields.insert("lastOnboardingVersion".into(), Value::String(version));
        }
        let text = serde_json::to_string_pretty(&config).map_err(io::Error::other)?;
        let temp = self
            .dir
            .join(format!(".claude.json.aip-{}", std::process::id()));
        fs::write(&temp, text)?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temp, &path)?;
        Ok(true)
    }
}

/// Every account, `default` first, then by name.
pub fn discover(config: &Config) -> Vec<AccountDir> {
    let mut found = Vec::new();
    let stock = config.home.join(".claude");
    if config.include_default && stock.is_dir() {
        found.push(AccountDir {
            name: DEFAULT_NAME.into(),
            dir: stock,
            is_default: true,
        });
    }
    let mut named: Vec<AccountDir> = fs::read_dir(&config.accounts_base)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            // Only a name an account could have been given: not a lock
            // folder (`marcus2.lock`), a hidden one, or anything a path
            // built from it could trip on.
            valid_new_name(&name).then(|| AccountDir {
                name,
                dir: entry.path(),
                is_default: false,
            })
        })
        .collect();
    named.sort_by(|a, b| a.name.cmp(&b.name));
    found.extend(named);
    found
}

/// Whether `name` may be a new account's folder name: a letter or digit, then
/// letters, digits, `-` and `_`, 64 at most. `default` means `~/.claude`.
pub fn valid_new_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64
        && name != DEFAULT_NAME
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, PartialEq, Eq)]
pub enum CreateError {
    InvalidName,
    Taken,
    Io(String),
}

/// Make an empty account (its config dir, 0700). Claude fills it in when it
/// first runs or signs in there.
pub fn create(config: &Config, name: &str) -> Result<AccountDir, CreateError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if !valid_new_name(name) {
        return Err(CreateError::InvalidName);
    }
    if taken(config, name, None) {
        return Err(CreateError::Taken);
    }
    fs::create_dir_all(&config.accounts_base).map_err(|err| CreateError::Io(err.to_string()))?;
    let dir = config.accounts_base.join(name);
    match fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(CreateError::Taken)
        }
        Err(err) => return Err(CreateError::Io(err.to_string())),
    }
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    Ok(AccountDir {
        name: name.to_owned(),
        dir,
        is_default: false,
    })
}

/// Whether an account other than `except` is called `name`, ignoring case:
/// `Work` and `work` would be two folders on Linux, but the same name to
/// anyone picking one from a list.
pub fn taken(config: &Config, name: &str, except: Option<&str>) -> bool {
    discover(config).iter().any(|account| {
        account.name.eq_ignore_ascii_case(name) && Some(account.name.as_str()) != except
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenameError {
    /// `default` is `~/.claude`, which isn't the server's to rename.
    Default,
    InvalidName,
    Taken,
    Io(String),
}

/// Rename `account`'s folder to `name`, and the paths into it that Claude
/// keeps in its plugin records (`plugins/*.json`, where marketplaces are
/// recorded by where they're installed), which would otherwise still point
/// at the old folder. Nothing may be running in it.
pub fn rename(
    config: &Config,
    account: &AccountDir,
    name: &str,
) -> Result<AccountDir, RenameError> {
    if account.is_default {
        return Err(RenameError::Default);
    }
    if !valid_new_name(name) {
        return Err(RenameError::InvalidName);
    }
    if name == account.name {
        return Ok(account.clone());
    }
    if taken(config, name, Some(&account.name)) {
        return Err(RenameError::Taken);
    }
    let dir = config.accounts_base.join(name);
    // A change of case only is a rename to a name the folder already answers
    // to on a case-insensitive file system; there's nothing else there.
    if dir.exists() && !name.eq_ignore_ascii_case(&account.name) {
        return Err(RenameError::Taken);
    }
    fs::rename(&account.dir, &dir).map_err(|err| RenameError::Io(err.to_string()))?;
    let (from, to) = (account.dir.display().to_string(), dir.display().to_string());
    if let Ok(entries) = fs::read_dir(dir.join("plugins")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if text.contains(&from) {
                let _ = fs::write(&path, text.replace(&from, &to));
            }
        }
    }
    Ok(AccountDir {
        name: name.to_owned(),
        dir,
        is_default: false,
    })
}

/// Move an account to `<base>/.trash/<name>-<time>`, where discovery doesn't
/// look (it skips dot-folders). Nothing is deleted: moving it back restores
/// it. Returns where it went.
pub fn trash(config: &Config, account: &AccountDir) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let trash = config.accounts_base.join(".trash");
    fs::create_dir_all(&trash)?;
    fs::set_permissions(&trash, fs::Permissions::from_mode(0o700))?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut target = trash.join(format!("{}-{stamp}", account.name));
    let mut attempt = 1;
    while target.exists() {
        attempt += 1;
        target = trash.join(format!("{}-{stamp}-{attempt}", account.name));
    }
    fs::rename(&account.dir, &target)?;
    Ok(target)
}

pub fn find(config: &Config, name: &str) -> Option<AccountDir> {
    discover(config)
        .into_iter()
        .find(|account| account.name == name)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(home: &Path) -> Config {
        let mut config = Config::from_toml("include_default = true", home).unwrap();
        config.accounts_base = home.join(".claude-accounts");
        config
    }

    #[test]
    fn lists_default_first_then_named_accounts_skipping_dot_folders() {
        let home = tempfile::tempdir().unwrap();
        let base = home.path().join(".claude-accounts");
        for name in [
            "work",
            "alpha",
            ".trash",
            ".claudemulti",
            "work.lock",
            "odd name",
        ] {
            fs::create_dir_all(base.join(name)).unwrap();
        }
        fs::write(base.join("not-a-dir"), "").unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();

        let names: Vec<String> = discover(&config(home.path()))
            .into_iter()
            .map(|account| account.name)
            .collect();
        assert_eq!(names, vec!["default", "alpha", "work"]);

        let mut without_default = config(home.path());
        without_default.include_default = false;
        assert_eq!(discover(&without_default).len(), 2);
    }

    #[test]
    fn reads_who_an_account_belongs_to() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".claude-accounts/work");
        fs::create_dir_all(&dir).unwrap();
        let account = AccountDir {
            name: "work".into(),
            dir: dir.clone(),
            is_default: false,
        };
        assert_eq!(account.account(home.path()), None);
        assert!(!account.signed_in());

        fs::write(
            dir.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"ada@example.com","organizationType":"claude_max"}}"#,
        )
        .unwrap();
        fs::write(dir.join(".credentials.json"), "{}").unwrap();
        let found = account.account(home.path()).unwrap();
        assert_eq!(found.email.as_deref(), Some("ada@example.com"));
        assert_eq!(found.plan.as_deref(), Some("Max"));
        assert!(account.signed_in());
    }

    #[test]
    fn a_sign_in_lasts_until_its_refresh_token_runs_out() {
        let home = tempfile::tempdir().unwrap();
        let account = AccountDir {
            name: "work".into(),
            dir: home.path().to_path_buf(),
            is_default: false,
        };
        let later = now_millis() + 86_400_000;
        fs::write(
            home.path().join(".credentials.json"),
            format!(
                r#"{{"claudeAiOauth":{{"refreshToken":"x","refreshTokenExpiresAt":{later}}}}}"#
            ),
        )
        .unwrap();
        assert!(account.signed_in());
        assert_eq!(account.signed_in_until(), Some(later));
        fs::write(
            home.path().join(".credentials.json"),
            r#"{"claudeAiOauth":{"refreshToken":"x","refreshTokenExpiresAt":1000}}"#,
        )
        .unwrap();
        assert!(!account.signed_in(), "ran out");
    }

    #[test]
    fn new_account_names_are_plain_folder_names() {
        for good in ["work", "marcus2", "a", "Work_2-b"] {
            assert!(valid_new_name(good), "{good}");
        }
        for bad in [
            "",
            "default",
            ".hidden",
            "-x",
            "_x",
            "a b",
            "a/b",
            "..",
            "é",
            &"x".repeat(65),
        ] {
            assert!(!valid_new_name(bad), "{bad}");
        }
    }

    #[test]
    fn marks_a_signed_in_account_set_up_keeping_the_rest() {
        let home = tempfile::tempdir().unwrap();
        let config = config(home.path());
        let made = create(&config, "work").unwrap();
        assert!(
            !made.finish_setup(|| Some("2.1.280".into())).unwrap(),
            "not signed in"
        );

        fs::write(made.dir.join(".credentials.json"), "{}").unwrap();
        fs::write(
            made.dir.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"a@b.c"},"userID":"u"}"#,
        )
        .unwrap();
        assert!(made.finish_setup(|| Some("2.1.280".into())).unwrap());
        let written: Value =
            serde_json::from_str(&fs::read_to_string(made.dir.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(written["hasCompletedOnboarding"], true);
        assert_eq!(written["lastOnboardingVersion"], "2.1.280");
        assert_eq!(written["oauthAccount"]["emailAddress"], "a@b.c");
        assert_eq!(written["userID"], "u");
        assert!(!made.finish_setup(|| panic!("not asked again")).unwrap());
        let names: Vec<_> = fs::read_dir(&made.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(!names.iter().any(|name| name.contains("aip-")), "{names:?}");

        let default = AccountDir {
            name: DEFAULT_NAME.into(),
            dir: made.dir.clone(),
            is_default: true,
        };
        fs::write(made.dir.join(".claude.json"), "{}").unwrap();
        assert!(
            !default.finish_setup(|| None).unwrap(),
            "default is left alone"
        );
    }

    #[test]
    fn creates_an_account_once_and_trashes_it_without_deleting() {
        let home = tempfile::tempdir().unwrap();
        let config = config(home.path());
        let made = create(&config, "work").unwrap();
        assert!(made.dir.is_dir());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&made.dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(create(&config, "work"), Err(CreateError::Taken));
        assert_eq!(create(&config, "../x"), Err(CreateError::InvalidName));

        fs::write(made.dir.join(".credentials.json"), "{}").unwrap();
        let trashed = trash(&config, &made).unwrap();
        assert!(!made.dir.exists());
        assert!(trashed.join(".credentials.json").is_file());
        assert!(trashed.starts_with(config.accounts_base.join(".trash")));
        assert!(
            find(&config, "work").is_none(),
            "a trashed account isn't listed"
        );

        let again = create(&config, "work").unwrap();
        let second = trash(&config, &again).unwrap();
        assert_ne!(trashed, second);
    }

    #[test]
    fn the_default_account_reads_claude_json_from_home() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(
            home.path().join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"me@example.com"}}"#,
        )
        .unwrap();
        let account = find(&config(home.path()), "default").unwrap();
        assert_eq!(
            account.account(home.path()).unwrap().email.as_deref(),
            Some("me@example.com")
        );
    }

    fn config_in(base: &Path) -> Config {
        let mut config = Config::from_toml(r#"listen = "127.0.0.1:0""#, base).unwrap();
        config.accounts_base = base.join(".claude-accounts");
        config.include_default = false;
        config
    }

    #[test]
    fn names_are_unique_ignoring_case() {
        let home = tempfile::tempdir().unwrap();
        let config = config_in(home.path());
        create(&config, "work").unwrap();
        assert_eq!(create(&config, "Work"), Err(CreateError::Taken));
        assert_eq!(create(&config, "work"), Err(CreateError::Taken));
        assert!(taken(&config, "WORK", None));
        assert!(!taken(&config, "work", Some("work")), "its own name");
    }

    #[test]
    fn renames_the_folder_and_the_plugin_records_pointing_into_it() {
        let home = tempfile::tempdir().unwrap();
        let config = config_in(home.path());
        let work = create(&config, "work").unwrap();
        create(&config, "home").unwrap();
        let marketplaces = work.dir.join("plugins/known_marketplaces.json");
        fs::create_dir_all(marketplaces.parent().unwrap()).unwrap();
        fs::write(
            &marketplaces,
            format!(
                r#"{{"m":{{"installLocation":"{}/plugins/marketplaces/m"}}}}"#,
                work.dir.display()
            ),
        )
        .unwrap();

        assert_eq!(rename(&config, &work, "Home"), Err(RenameError::Taken));
        assert_eq!(
            rename(&config, &work, "../x"),
            Err(RenameError::InvalidName)
        );
        let renamed = rename(&config, &work, "office").unwrap();
        assert_eq!(renamed.name, "office");
        assert!(!work.dir.exists());
        let text = fs::read_to_string(renamed.dir.join("plugins/known_marketplaces.json")).unwrap();
        assert!(
            text.contains(&format!("{}/plugins/marketplaces/m", renamed.dir.display())),
            "{text}"
        );
        assert!(!text.contains("/work/"), "{text}");
        assert!(find(&config, "office").is_some());
    }
}
