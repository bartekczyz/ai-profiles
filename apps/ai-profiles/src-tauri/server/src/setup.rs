//! `ai-profiles-server setup`: the steps to a working server, one at a time,
//! each checked first so that what's already done is only confirmed, and
//! only what isn't is asked about.
//!
//! 1. tmux, 3.0 or newer: sessions run in its windows.
//! 2. Claude Code.
//! 3. Profiles: the accounts in `accounts_base`, and whether `~/.claude`
//!    shows as `default`.
//! 4. Network: what the Mac can reach the server over (Tailscale, WireGuard),
//!    and whether to accept a LAN as well.
//! 5. The systemd user service, lingering so it runs while logged out.
//! 6. Pairing: a code to paste into ai-profiles, then waiting for the Mac.
//!
//! What it changes: `config.toml` (only the keys it asks about, keeping the
//! rest of the file as written), the service unit, lingering (through `sudo`,
//! after asking), and a pairing code.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::net::IpAddr;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use if_addrs::IfAddr;
use ipnet::IpNet;

use crate::accounts;
use crate::config::{Config, Paths};
use crate::hostinfo;
use crate::pairing_code;
use crate::service;
use crate::store::Store;
use crate::tunnels;

const STEPS: usize = 6;
/// How long setup waits for the Mac to pair.
const PAIR_WAIT: Duration = Duration::from_secs(10 * 60);

/// Walk through setup, asking on this terminal.
pub fn run(paths: &Paths) -> io::Result<()> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::other(
            "setup asks questions: run it in a terminal",
        ));
    }
    println!("Setting up ai-profiles-server. Each step is checked first; press Ctrl-C to stop at any time.");

    step(1, "tmux");
    loop {
        match hostinfo::tmux_version() {
            Some(version) if tmux_is_recent(&version) => {
                ok(&version);
                break;
            }
            Some(version) => missing(&format!(
                "{version} is too old: sessions need tmux 3.0 or newer. Update it: {}",
                install_hint("tmux")
            )),
            None => missing(&format!(
                "tmux isn't installed. Install it: {}",
                install_hint("tmux")
            )),
        }
        wait_to_check_again()?;
    }

    step(2, "Claude Code");
    loop {
        let config = Config::load(paths)?;
        match hostinfo::claude_path(&config) {
            Some(path) => {
                ok(&format!("claude at {}", path.display()));
                break;
            }
            None => missing(
                "claude isn't installed where the server looks (claude_path in config.toml, ~/.local/bin, PATH). Install it: curl -fsSL https://claude.ai/install.sh | bash",
            ),
        }
        wait_to_check_again()?;
    }

    step(3, "Profiles");
    let config = Config::load(paths)?;
    let found = accounts::discover(&config);
    if found.is_empty() {
        println!(
            "  No profiles in {} yet. Add them from ai-profiles once paired: New Profile → Claude CLI Remote.",
            config.accounts_base.display()
        );
    } else {
        ok(&format!(
            "{} in {}: {}",
            found.len(),
            config.accounts_base.display(),
            found
                .iter()
                .map(|account| account.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !config.include_default
        && config.home.join(".claude").is_dir()
        && ask(
            "  ~/.claude is here too (Claude Code without a profile). Show it as the Default profile?",
            false,
        )?
    {
        set_config(paths, |document| {
            document["include_default"] = toml_edit::value(true);
        })?;
        ok("Default profile on");
    }

    step(4, "Network");
    let config = Config::load(paths)?;
    let tailscale: Vec<String> = hostinfo::reachable_hosts()
        .into_iter()
        .filter(|host| {
            host.parse::<IpAddr>()
                .is_ok_and(|ip| crate::config::Config::default_allow_from()[0].contains(&ip))
                || host.ends_with(".ts.net")
        })
        .collect();
    if tailscale.is_empty() {
        println!("  Tailscale: not found.");
    } else {
        ok(&format!("Tailscale: {}", tailscale.join(", ")));
    }
    let tunnels = tunnels::wireguard();
    for tunnel in &tunnels {
        if tunnels::admits_no_peer(tunnel) {
            missing(&format!(
                "WireGuard {}: {} is a single address, so no peer is accepted from it. Add the peers' network to allow_from in {}.",
                tunnel.interface,
                tunnel.network,
                paths.config_file().display()
            ));
        } else {
            ok(&format!(
                "WireGuard {}: {} (accepted)",
                tunnel.interface, tunnel.network
            ));
        }
    }
    let lans: Vec<IpNet> = lan_networks()
        .into_iter()
        .filter(|network| !config.allow_from.contains(network))
        .collect();
    // Each one accepted adds to the list, so a second doesn't drop the first.
    let mut allow_from = config.allow_from.clone();
    for lan in &lans {
        if ask(
            &format!("  Also accept connections from {lan}, a local network here? Anyone on it could then try to pair (they'd still need a code)."),
            false,
        )? {
            allow_from.push(*lan);
            set_config(paths, |document| {
                let mut list = toml_edit::Array::new();
                for network in &allow_from {
                    list.push(network.to_string());
                }
                document["allow_from"] = toml_edit::value(list);
            })?;
            ok(&format!("{lan} accepted"));
        }
    }
    let config = Config::load(paths)?;
    println!(
        "  Accepting connections from: {}",
        config
            .allow_from
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    if tailscale.is_empty() && tunnels.is_empty() && lans.is_empty() {
        missing("No Tailscale, WireGuard or LAN found: only this machine can reach the server. Install Tailscale, or add a network to allow_from.");
    }

    step(5, "Service");
    let unit = service::unit_path(&config.home);
    let running = systemctl_ok(&["is-active", "--quiet", service::UNIT_NAME]);
    let question = if running {
        "  The service is running. Reinstall and restart it (picks up this binary)?"
    } else if unit.exists() {
        "  The service is installed but not running. Start it?"
    } else {
        "  Install the service, so the server starts at login and restarts if it stops?"
    };
    if ask(question, !running)? {
        service::install(&config.home, true)?;
        ok(&format!("installed {} and started", unit.display()));
    }
    if systemctl_ok(&["is-active", "--quiet", service::UNIT_NAME]) {
        ok(&format!("running, listening on {}", config.listen));
    } else {
        missing(&format!(
            "not running. Its log: journalctl --user -u {} -e",
            service::UNIT_NAME
        ));
    }
    match service::lingering() {
        Some(true) => ok("keeps running while you're logged out (lingering on)"),
        Some(false) => {
            let user = std::env::var("USER").unwrap_or_default();
            if ask(
                "  It stops when you log out, and with it every session. Turn on lingering (runs `sudo loginctl enable-linger`, asks for your password)?",
                true,
            )? {
                let status = Command::new("sudo")
                    .args(["loginctl", "enable-linger", &user])
                    .status()?;
                if status.success() {
                    ok("lingering on");
                } else {
                    missing(&format!(
                        "that didn't work. Run it yourself: sudo loginctl enable-linger {user}"
                    ));
                }
            }
        }
        None => println!("  Couldn't tell whether lingering is on (no loginctl)."),
    }

    step(6, "Pair a Mac");
    let store = Store::new(paths.ensure_state_dir()?);
    let before = store.clients()?.len();
    if before > 0 {
        ok(&format!("{before} paired already"));
    }
    if ask("  Pair a Mac now?", before == 0)? {
        let issued = pairing_code::issue(&config, paths, None, Vec::new())?;
        println!("\n  In ai-profiles: Settings → Remote hosts → Pair a host, and paste:\n");
        println!("  {}\n", issued.code);
        println!("  Addresses:   {}", issued.hosts.join(", "));
        println!("  Certificate: {}", issued.fingerprint);
        println!("  (ai-profiles shows the same fingerprint before it pairs: check they match.)\n");
        print!("  Waiting for it to pair…");
        io::stdout().flush()?;
        let deadline = Instant::now() + PAIR_WAIT;
        loop {
            let clients = store.clients()?;
            if clients.len() > before {
                let name = clients
                    .last()
                    .map(|client| client.name.clone())
                    .unwrap_or_default();
                println!();
                ok(&format!("paired with {name}"));
                break;
            }
            if Instant::now() > deadline {
                println!();
                missing("the code has expired. Run `ai-profiles-server pair` for another.");
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    println!("\nDone. `ai-profiles-server doctor` checks all of this again at any time.");
    Ok(())
}

fn step(number: usize, title: &str) {
    println!("\nStep {number} of {STEPS}: {title}");
}

fn ok(text: &str) {
    println!("  ✓ {text}");
}

fn missing(text: &str) {
    println!("  ✗ {text}");
}

/// Ask a yes/no question; Enter takes `default`.
fn ask(question: &str, default: bool) -> io::Result<bool> {
    print!("{question} {} ", if default { "[Y/n]" } else { "[y/N]" });
    io::stdout().flush()?;
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer)? == 0 {
        return Err(io::Error::other("setup stopped"));
    }
    Ok(yes(&answer, default))
}

/// Pure: a yes/no answer, `default` when it's empty.
fn yes(answer: &str, default: bool) -> bool {
    match answer.trim().to_lowercase().as_str() {
        "" => default,
        "y" | "yes" => true,
        _ => false,
    }
}

fn wait_to_check_again() -> io::Result<()> {
    print!("  Press Enter to check again (Ctrl-C to stop). ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line)? == 0 {
        return Err(io::Error::other("setup stopped"));
    }
    Ok(())
}

fn systemctl_ok(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

/// Pure: whether `tmux -V`'s answer (`tmux 3.0a`, `tmux next-3.4`) is 3.0
/// or newer.
fn tmux_is_recent(version: &str) -> bool {
    let number: String = version
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_start_matches("next-")
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = number
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    parts.next().unwrap_or(0) >= 3
}

/// How to install `package` on this distribution, from `/etc/os-release`.
fn install_hint(package: &str) -> String {
    let release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    install_command(&release, package)
}

/// Pure: the install command for `package` on the distribution
/// `os_release` describes.
fn install_command(os_release: &str, package: &str) -> String {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('"').to_lowercase())
            .unwrap_or_default()
    };
    let family = format!("{} {}", field("ID"), field("ID_LIKE"));
    let has = |name: &str| family.split_whitespace().any(|id| id == name);
    if has("debian") || has("ubuntu") {
        format!("sudo apt install {package}")
    } else if has("fedora") || has("rhel") || has("centos") {
        format!("sudo dnf install {package}")
    } else if has("arch") {
        format!("sudo pacman -S {package}")
    } else if has("alpine") {
        format!("sudo apk add {package}")
    } else if has("suse") || has("opensuse") {
        format!("sudo zypper install {package}")
    } else {
        format!("install {package} with your package manager")
    }
}

/// This machine's private networks (a home or office LAN), leaving out
/// loopback, link-local, Tailscale, WireGuard and container bridges.
fn lan_networks() -> Vec<IpNet> {
    let tunnels: Vec<String> = tunnels::wireguard()
        .into_iter()
        .map(|tunnel| tunnel.interface)
        .collect();
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut networks = Vec::new();
    for interface in interfaces {
        if interface.is_loopback()
            || interface.is_link_local()
            || tunnels.contains(&interface.name)
            || !is_lan_interface(&interface.name)
        {
            continue;
        }
        let (address, prefix) = match &interface.addr {
            IfAddr::V4(v4) => (IpAddr::V4(v4.ip), v4.prefixlen),
            IfAddr::V6(v6) => (IpAddr::V6(v6.ip), v6.prefixlen),
        };
        if !is_private(address) {
            continue;
        }
        if let Ok(network) = IpNet::new(address, prefix) {
            let network = network.trunc();
            // A single address (a cloud VM's /32) is no network: no one
            // else is on it.
            if network.prefix_len() == network.max_prefix_len() {
                continue;
            }
            if !networks.contains(&network) {
                networks.push(network);
            }
        }
    }
    networks
}

/// Pure: whether an interface can be a LAN: not Tailscale's, and not a
/// container or VM bridge.
fn is_lan_interface(name: &str) -> bool {
    ![
        "tailscale",
        "docker",
        "br-",
        "veth",
        "virbr",
        "lxc",
        "cni",
        "flannel",
        "podman",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// Pure: a private address (10/8, 172.16/12, 192.168/16, fc00::/7), not in
/// Tailscale's ranges.
fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(v6) => {
            (v6.segments()[0] & 0xfe00) == 0xfc00
                && !"fd7a:115c:a1e0::/48"
                    .parse::<IpNet>()
                    .expect("a valid range")
                    .contains(&IpAddr::V6(v6))
        }
    }
}

/// Change `config.toml` with `change`, keeping everything else in it as
/// written (comments included), and make it if there's none.
fn set_config(paths: &Paths, change: impl FnOnce(&mut toml_edit::Document)) -> io::Result<()> {
    let file = paths.config_file();
    let text = fs::read_to_string(&file).unwrap_or_default();
    let updated = edit_config(&text, change)?;
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    write_atomically(&file, &updated)
}

/// Pure: `text` with `change` made to it.
fn edit_config(text: &str, change: impl FnOnce(&mut toml_edit::Document)) -> io::Result<String> {
    let mut document: toml_edit::Document = text
        .parse()
        .map_err(|err| io::Error::other(format!("config.toml: {err}")))?;
    change(&mut document);
    Ok(document.to_string())
}

fn write_atomically(file: &Path, text: &str) -> io::Result<()> {
    let staged = file.with_extension("toml.tmp");
    fs::write(&staged, text)?;
    fs::rename(&staged, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_tmux_3_or_newer() {
        assert!(tmux_is_recent("tmux 3.0a"));
        assert!(tmux_is_recent("tmux 3.4"));
        assert!(tmux_is_recent("tmux next-3.5"));
        assert!(!tmux_is_recent("tmux 2.9a"));
        assert!(!tmux_is_recent("tmux"));
    }

    #[test]
    fn says_how_to_install_on_each_distribution() {
        assert_eq!(
            install_command("ID=ubuntu\nID_LIKE=debian\n", "tmux"),
            "sudo apt install tmux"
        );
        assert_eq!(
            install_command("ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n", "tmux"),
            "sudo dnf install tmux"
        );
        assert_eq!(install_command("ID=arch\n", "tmux"), "sudo pacman -S tmux");
        assert_eq!(install_command("ID=alpine\n", "tmux"), "sudo apk add tmux");
        assert_eq!(
            install_command("", "tmux"),
            "install tmux with your package manager"
        );
    }

    #[test]
    fn a_lan_is_private_and_not_a_tunnel_or_bridge() {
        assert!(is_private("192.168.1.20".parse().unwrap()));
        assert!(is_private("10.0.0.5".parse().unwrap()));
        assert!(!is_private("100.110.48.97".parse().unwrap()), "Tailscale");
        assert!(!is_private("34.1.2.3".parse().unwrap()));
        assert!(is_private("fd12:3456::1".parse().unwrap()));
        assert!(
            !is_private("fd7a:115c:a1e0::5".parse().unwrap()),
            "Tailscale"
        );
        assert!(is_lan_interface("eth0"));
        assert!(is_lan_interface("enp3s0"));
        assert!(!is_lan_interface("docker0"));
        assert!(!is_lan_interface("br-1a2b"));
        assert!(!is_lan_interface("tailscale0"));
    }

    #[test]
    fn edits_only_the_keys_it_asks_about() {
        let text = "# my server\nlisten = \"0.0.0.0:7443\" # keep\n";
        let updated = edit_config(text, |document| {
            document["include_default"] = toml_edit::value(true);
        })
        .unwrap();
        assert!(updated.starts_with("# my server\nlisten = \"0.0.0.0:7443\" # keep\n"));
        assert!(updated.contains("include_default = true"));
    }

    #[test]
    fn takes_enter_as_the_default_answer() {
        assert!(yes("\n", true));
        assert!(!yes("\n", false));
        assert!(yes("Y\n", false));
        assert!(!yes("no\n", true));
    }
}
