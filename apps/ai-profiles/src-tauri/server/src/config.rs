//! Where the server keeps things, and what it can be told.
//!
//! - Config: `$XDG_CONFIG_HOME/ai-profiles-server/config.toml`, optional.
//! - State: `$XDG_STATE_HOME/ai-profiles-server/` (clients, pending pairings,
//!   the TLS certificate), created 0700.
//!
//! `AI_PROFILES_SERVER_HOME` puts both under one folder, for tests and for
//! running a second instance.

use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ipnet::IpNet;
use serde::Deserialize;

pub const APP_DIR: &str = "ai-profiles-server";
pub const DEFAULT_PORT: u16 = 7443;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> io::Result<Paths> {
        if let Some(root) = std::env::var_os("AI_PROFILES_SERVER_HOME") {
            let root = PathBuf::from(root);
            return Ok(Paths {
                config_dir: root.join("config"),
                state_dir: root.join("state"),
            });
        }
        let home = home()?;
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Ok(Paths {
            config_dir: xdg("XDG_CONFIG_HOME", ".config").join(APP_DIR),
            state_dir: xdg("XDG_STATE_HOME", ".local/state").join(APP_DIR),
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The state folder, made (0700) if it isn't there.
    pub fn ensure_state_dir(&self) -> io::Result<&Path> {
        fs::create_dir_all(&self.state_dir)?;
        fs::set_permissions(&self.state_dir, fs::Permissions::from_mode(0o700))?;
        Ok(&self.state_dir)
    }
}

pub fn home() -> io::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| io::Error::other("could not determine the home folder"))
}

/// `~` and `~/…` against `home`; anything else as it is.
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with('/') => home.join(&rest[1..]),
        _ => PathBuf::from(path),
    }
}

/// What `config.toml` may set. Everything has a default.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    listen: Option<String>,
    allow_from: Option<Vec<IpNet>>,
    accounts_base: Option<String>,
    include_default: Option<bool>,
    tmux_session: Option<String>,
    tmux_socket: Option<String>,
    claude_path: Option<String>,
    folder_roots: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    /// Peers outside these are dropped before the TLS handshake.
    pub allow_from: Vec<IpNet>,
    /// Each non-dot folder in it is an account (claudemulti's layout).
    pub accounts_base: PathBuf,
    /// Also offer `~/.claude` as the account `default`. Off unless set, as in
    /// claudemulti (`CLAUDEMULTI_INCLUDE_DEFAULT`): on a machine set up with
    /// an accounts folder, `~/.claude` is usually a leftover.
    pub include_default: bool,
    /// The tmux session new Claude sessions open in.
    pub tmux_session: String,
    /// A tmux socket name (`tmux -L`), for a tmux server apart from the
    /// user's own. Unset: the user's default server, so the sessions are the
    /// ones `tmux attach` finds.
    pub tmux_socket: Option<String>,
    pub claude_path: Option<PathBuf>,
    /// Folders a client may browse and start sessions in.
    pub folder_roots: Vec<PathBuf>,
    pub home: PathBuf,
}

impl Config {
    /// Tailscale's address ranges, and this machine.
    pub fn default_allow_from() -> Vec<IpNet> {
        [
            "100.64.0.0/10",
            "fd7a:115c:a1e0::/48",
            "127.0.0.0/8",
            "::1/128",
        ]
        .iter()
        .map(|net| net.parse().expect("a valid built-in range"))
        .collect()
    }

    pub fn load(paths: &Paths) -> io::Result<Config> {
        let text = match fs::read_to_string(paths.config_file()) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err),
        };
        // WireGuard peers are accepted by default, as Tailscale's are.
        let tunnels: Vec<IpNet> = crate::tunnels::wireguard()
            .into_iter()
            .map(|tunnel| tunnel.network)
            .collect();
        Config::from_toml_with(&text, &home()?, &tunnels)
    }

    pub fn from_toml(text: &str, home: &Path) -> io::Result<Config> {
        Config::from_toml_with(text, home, &[])
    }

    /// As [`Config::from_toml`], with `more_defaults` accepted too when the
    /// file doesn't say which networks to accept: those of the machine's
    /// WireGuard tunnels. A file that lists `allow_from` gets only those.
    pub fn from_toml_with(text: &str, home: &Path, more_defaults: &[IpNet]) -> io::Result<Config> {
        let file: File = toml::from_str(text)
            .map_err(|err| io::Error::other(format!("config.toml: {}", err.message())))?;
        let listen = file
            .listen
            .as_deref()
            .unwrap_or("0.0.0.0:7443")
            .parse()
            .map_err(|_| io::Error::other("config.toml: `listen` must be an address:port"))?;
        let accounts_base = file
            .accounts_base
            .map(|base| expand_home(&base, home))
            .or_else(|| std::env::var_os("CLAUDE_PROFILES_BASE").map(PathBuf::from))
            .unwrap_or_else(|| home.join(".claude-accounts"));
        let tmux_session = file.tmux_session.unwrap_or_else(|| "ai".into());
        if tmux_session.is_empty()
            || !tmux_session
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(io::Error::other(
                "config.toml: `tmux_session` may use letters, digits, - and _ only",
            ));
        }
        let config = Config {
            listen,
            allow_from: file.allow_from.unwrap_or_else(|| {
                let mut networks = Config::default_allow_from();
                for network in more_defaults {
                    if !networks.contains(network) {
                        networks.push(*network);
                    }
                }
                networks
            }),
            accounts_base,
            include_default: file.include_default.unwrap_or(false),
            tmux_session,
            tmux_socket: file.tmux_socket.filter(|socket| !socket.is_empty()),
            claude_path: file.claude_path.map(|path| expand_home(&path, home)),
            folder_roots: file
                .folder_roots
                .unwrap_or_else(|| vec!["~".into()])
                .iter()
                .map(|root| expand_home(root, home))
                .collect(),
            home: home.to_path_buf(),
        };
        if !config.listen.ip().is_loopback() && config.allow_from.is_empty() {
            return Err(io::Error::other(
                "config.toml: `allow_from` is empty, so anyone who can reach the port could try to pair; list the networks to accept",
            ));
        }
        Ok(config)
    }

    /// Whether a peer may connect at all.
    pub fn allows(&self, peer: std::net::IpAddr) -> bool {
        // An IPv4 client on a dual-stack socket arrives as ::ffff:a.b.c.d.
        let peer = match peer {
            std::net::IpAddr::V6(v6) => v6
                .to_ipv4_mapped()
                .map(std::net::IpAddr::V4)
                .unwrap_or(peer),
            v4 => v4,
        };
        self.allow_from.iter().any(|net| net.contains(&peer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_wireguard_peers_by_default_but_not_past_an_allow_from() {
        let home = Path::new("/home/m");
        let wg: IpNet = "10.8.0.0/24".parse().unwrap();
        let config = Config::from_toml_with("", home, &[wg]).unwrap();
        assert!(config.allows("10.8.0.5".parse().unwrap()));
        assert!(
            config.allows("100.101.102.103".parse().unwrap()),
            "Tailscale still"
        );
        assert!(!config.allows("192.168.1.5".parse().unwrap()), "not a LAN");

        let listed =
            Config::from_toml_with("allow_from = [\"192.168.1.0/24\"]", home, &[wg]).unwrap();
        assert!(listed.allows("192.168.1.5".parse().unwrap()));
        assert!(
            !listed.allows("10.8.0.5".parse().unwrap()),
            "a listed allow_from is the whole list"
        );
    }

    #[test]
    fn defaults_to_tailscale_only_on_port_7443() {
        let config = Config::from_toml("", Path::new("/home/m")).unwrap();
        assert_eq!(config.listen.port(), DEFAULT_PORT);
        assert_eq!(config.tmux_session, "ai");
        assert_eq!(config.folder_roots, vec![PathBuf::from("/home/m")]);
        assert!(!config.include_default);
        assert!(config.allows("100.110.48.97".parse().unwrap()));
        assert!(config.allows("127.0.0.1".parse().unwrap()));
        assert!(config.allows("::ffff:100.64.1.2".parse().unwrap()));
        assert!(!config.allows("192.168.1.20".parse().unwrap()));
    }

    #[test]
    fn reads_what_is_set_and_expands_home() {
        let config = Config::from_toml(
            r#"
            listen = "127.0.0.1:9000"
            allow_from = ["192.168.0.0/16"]
            accounts_base = "~/accts"
            include_default = true
            tmux_session = "claude-work"
            claude_path = "~/.local/bin/claude"
            folder_roots = ["~/code", "/srv"]
            "#,
            Path::new("/home/m"),
        )
        .unwrap();
        assert_eq!(config.listen.to_string(), "127.0.0.1:9000");
        assert_eq!(config.accounts_base, PathBuf::from("/home/m/accts"));
        assert!(config.include_default);
        assert_eq!(
            config.claude_path,
            Some(PathBuf::from("/home/m/.local/bin/claude"))
        );
        assert_eq!(
            config.folder_roots,
            vec![PathBuf::from("/home/m/code"), PathBuf::from("/srv")]
        );
        assert!(config.allows("192.168.4.4".parse().unwrap()));
        assert!(!config.allows("100.110.48.97".parse().unwrap()));
    }

    #[test]
    fn refuses_settings_that_would_be_unsafe_or_break_tmux() {
        let home = Path::new("/home/m");
        assert!(Config::from_toml("allow_from = []", home).is_err());
        assert!(Config::from_toml("listen = \"127.0.0.1:1\"\nallow_from = []", home).is_ok());
        assert!(Config::from_toml("tmux_session = \"a:b\"", home).is_err());
        assert!(Config::from_toml("unknown = 1", home).is_err());
    }

    #[test]
    fn expands_only_a_leading_tilde() {
        let home = Path::new("/h");
        assert_eq!(expand_home("~", home), PathBuf::from("/h"));
        assert_eq!(expand_home("~/x", home), PathBuf::from("/h/x"));
        assert_eq!(expand_home("~other/x", home), PathBuf::from("~other/x"));
        assert_eq!(expand_home("/a/~", home), PathBuf::from("/a/~"));
    }
}
