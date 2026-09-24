//! What this machine is: its name, where `claude` is, which tmux, and the
//! addresses a client could reach it at.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ai_profiles_core::api::{ClaudeInfo, HostInfo, TmuxInfo, API_VERSION};

use crate::config::Config;

pub fn hostname() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "localhost".into())
}

/// `claude`, looked for where it is actually installed. A systemd service (and
/// a non-interactive ssh shell) has a bare PATH that misses `~/.local/bin`,
/// which is where the native installer puts it.
pub fn claude_path(config: &Config) -> Option<PathBuf> {
    if let Some(path) = &config.claude_path {
        return path.is_file().then(|| path.clone());
    }
    let installed = config.home.join(".local/bin/claude");
    if installed.is_file() {
        return Some(installed);
    }
    find_on_path("claude")
}

pub fn find_on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

/// Claude's version number alone (`2.1.280` from `2.1.280 (Claude Code)`).
pub fn claude_version(claude: &Path) -> Option<String> {
    first_line(claude, &["--version"])?
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

/// How long a `--version` gets.
const VERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The first line a `--version`-style command prints, if it runs, and
/// within a few seconds: one that hangs mustn't hold up whatever asked.
fn first_line(program: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    let output =
        ai_profiles_core::child::run_within(&mut command, Vec::new(), VERSION_TIMEOUT).ok()??;
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (output.status.success() && !line.is_empty()).then(|| line.to_owned())
}

pub fn tmux_version() -> Option<String> {
    first_line(Path::new("tmux"), &["-V"])
}

pub fn host_info(config: &Config) -> HostInfo {
    HostInfo {
        hostname: hostname(),
        home: config.home.display().to_string(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        api_version: API_VERSION,
        tmux: tmux_version().map(|version| TmuxInfo {
            version,
            session: config.tmux_session.clone(),
        }),
        claude: claude_path(config).map(|path| ClaudeInfo {
            version: first_line(&path, &["--version"]),
            path: path.display().to_string(),
        }),
        accounts_base: config.accounts_base.display().to_string(),
        includes_default: config.include_default,
    }
}

/// Where a client could reach this machine, best first: its Tailscale
/// addresses and MagicDNS name, then its host name.
pub fn reachable_hosts() -> Vec<String> {
    let mut hosts = Vec::new();
    if let Some(output) = Command::new("tailscale")
        .args(["ip", "-4"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
    {
        hosts.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned),
        );
    }
    if let Some(name) = tailscale_dns_name() {
        hosts.push(name);
    }
    // WireGuard's addresses, a peer's way in.
    for tunnel in crate::tunnels::wireguard() {
        let address = tunnel.address.to_string();
        if !hosts.contains(&address) {
            hosts.push(address);
        }
    }
    let name = hostname();
    if !hosts.contains(&name) {
        hosts.push(name);
    }
    hosts
}

/// Pure: `host` as `host:port`, given the server's `port` when it has none. An
/// IPv6 address, bare or in brackets, goes in brackets (`[fd7a::1]:7443`),
/// the only way a port can follow it.
pub fn with_port(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.parse::<std::net::SocketAddr>().is_ok() {
        return host.to_owned();
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return std::net::SocketAddr::new(ip, port).to_string();
    }
    match host.rsplit_once(':') {
        Some((name, given)) if !name.contains(':') && given.parse::<u16>().is_ok() => {
            host.to_owned()
        }
        _ => format!("{host}:{port}"),
    }
}

fn tailscale_dns_name() -> Option<String> {
    let output = Command::new("tailscale")
        .args(["status", "--self", "--json"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let name = status.get("Self")?.get("DNSName")?.as_str()?;
    let name = name.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_each_pairing_address_the_port_ipv6_in_brackets() {
        assert_eq!(with_port("xjopa1", 7443), "xjopa1:7443");
        assert_eq!(with_port("xjopa1:9000", 7443), "xjopa1:9000");
        assert_eq!(with_port("100.110.48.97", 7443), "100.110.48.97:7443");
        assert_eq!(with_port("10.8.0.1:9000", 7443), "10.8.0.1:9000");
        assert_eq!(with_port("fd7a::1", 7443), "[fd7a::1]:7443");
        assert_eq!(with_port("[fd7a::1]", 7443), "[fd7a::1]:7443");
        assert_eq!(with_port("[fd7a::1]:9000", 7443), "[fd7a::1]:9000");
        assert_eq!(with_port(" fe80::1 ", 7443), "[fe80::1]:7443");
    }
}
