use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use ai_profiles_core::pairing::display_fingerprint;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::{Config, Paths};
use ai_profiles_server::revive;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::store::{Store, PAIRING_TTL};
use ai_profiles_server::{accounts, hostinfo, moves, pairing_code, serve, service, setup};
use clap::{Parser, Subcommand};

/// Lets ai-profiles on your Mac see and start the Claude sessions on this machine.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server (what the systemd service runs).
    Serve {
        /// Listen here instead of the configured address, e.g. 127.0.0.1:7443.
        #[arg(long)]
        listen: Option<std::net::SocketAddr>,
    },
    /// Print a one-time code to paste into ai-profiles (Settings → Remote hosts).
    Pair {
        /// Name the client this code will pair, e.g. "Marcus's MacBook".
        #[arg(long)]
        label: Option<String>,
        /// Address the Mac should use, host:port or host. Repeat for several;
        /// by default the Tailscale addresses and host name are offered.
        #[arg(long = "host")]
        hosts: Vec<String>,
    },
    /// List paired clients.
    Clients,
    /// Stop a client's access, by id or name.
    Revoke { client: String },
    /// Install and start a systemd user service running `serve`.
    InstallService {
        /// Write and enable the unit, but don't start it now.
        #[arg(long)]
        no_start: bool,
    },
    /// Check that everything the server needs is in place.
    Doctor,
    /// Set the server up step by step: tmux, Claude Code, profiles, network,
    /// the service, and pairing a Mac.
    Setup,
    /// Print the TLS certificate's fingerprint.
    Fingerprint,
}

fn main() -> ExitCode {
    // Before any thread starts: environment variables are process-wide.
    for name in inherited_session_variables(std::env::vars_os().map(|(name, _)| name)) {
        std::env::remove_var(name);
    }
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ai-profiles-server: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Pure: the variables among `names` that say this process was started from
/// inside a Claude Code session, or a tmux pane. Started from one (by hand,
/// or by a Claude that set the server up), the server would hand them to
/// every `claude` it runs, which then acts as that session's child: a
/// sign-in never prints its link, and a session it starts is nested in
/// another. `TMUX` would also send every tmux command to that pane's server
/// rather than the server's own.
fn inherited_session_variables(
    names: impl Iterator<Item = std::ffi::OsString>,
) -> Vec<std::ffi::OsString> {
    names
        .filter(|name| {
            let name = name.to_string_lossy();
            matches!(
                name.as_ref(),
                "CLAUDECODE" | "CLAUDE_PID" | "CLAUDE_EFFORT" | "TMUX" | "TMUX_PANE"
            ) || name.starts_with("CLAUDE_CODE_")
        })
        .collect()
}

fn run(cli: Cli) -> io::Result<()> {
    let paths = Paths::from_env()?;
    let mut config = Config::load(&paths)?;
    match cli.command {
        Command::Serve { listen } => {
            if let Some(listen) = listen {
                config.listen = listen;
            }
            serve_forever(config, &paths)
        }
        Command::Pair { label, hosts } => pair(&config, &paths, label, hosts),
        Command::Clients => {
            let clients = Store::new(paths.ensure_state_dir()?).clients()?;
            if clients.is_empty() {
                println!("No clients are paired. Run `ai-profiles-server pair` to pair one.");
            }
            for client in clients {
                println!(
                    "{}  {}  paired {}",
                    client.id,
                    client.name,
                    client.created_at.format("%Y-%m-%d %H:%M")
                );
            }
            Ok(())
        }
        Command::Revoke { client } => {
            let removed = Store::new(paths.ensure_state_dir()?).revoke(&client)?;
            if removed.is_empty() {
                return Err(io::Error::other(format!(
                    "no client with id or name {client:?}"
                )));
            }
            for client in removed {
                println!(
                    "Revoked {} ({}). It can't connect any more.",
                    client.name, client.id
                );
            }
            Ok(())
        }
        Command::InstallService { no_start } => {
            let unit = service::install(&config.home, !no_start)?;
            println!("Installed {}.", unit.display());
            if !no_start {
                println!(
                    "Started. Logs: journalctl --user -u {} -f",
                    service::UNIT_NAME
                );
            }
            if service::lingering() == Some(false) {
                println!(
                    "\nThe service stops when you log out. To keep it running, run once:\n  sudo loginctl enable-linger {}",
                    std::env::var("USER").unwrap_or_else(|_| "$USER".into())
                );
            }
            Ok(())
        }
        Command::Doctor => doctor(&config, &paths),
        Command::Setup => setup::run(&paths),
        Command::Fingerprint => {
            let identity = Identity::load_or_create(paths.ensure_state_dir()?)?;
            println!("{}", display_fingerprint(&identity.fingerprint));
            Ok(())
        }
    }
}

fn serve_forever(config: Config, paths: &Paths) -> io::Result<()> {
    let state_dir = paths.ensure_state_dir()?.to_path_buf();
    let identity = Identity::load_or_create(&state_dir)?;
    let tls = identity.server_config()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(config.listen).await?;
        eprintln!(
            "ai-profiles-server {} listening on {} (certificate {})",
            env!("CARGO_PKG_VERSION"),
            config.listen,
            display_fingerprint(&identity.fingerprint)
        );
        // Claude says which sessions are live, where it can.
        let processes =
            ai_profiles_server::agents::ClaudeAgents::new(hostinfo::claude_path(&config));
        let state = Arc::new(ServerState::new(config, &state_dir, Arc::new(processes)));
        // After a reboot, bring back what was running; from then on, keep
        // writing down what is. In the background: the server answers
        // meanwhile.
        let background = state.clone();
        tokio::spawn(async move {
            let reviving = background.clone();
            let revived = tokio::task::spawn_blocking(move || revive::revive(&reviving))
                .await
                .unwrap_or_default();
            for (account, id, outcome) in revived {
                match outcome {
                    Ok(()) => {
                        eprintln!("resumed {account}/{id}, which was running before the restart")
                    }
                    Err(message) => eprintln!("could not resume {account}/{id}: {message}"),
                }
            }
            let config = background.config.clone();
            let compressed =
                tokio::task::spawn_blocking(move || moves::compress_old_archives(&config))
                    .await
                    .unwrap_or_default();
            for line in compressed {
                eprintln!("{line}");
            }
            loop {
                let remembering = background.clone();
                let _ = tokio::task::spawn_blocking(move || revive::remember(&remembering)).await;
                tokio::time::sleep(revive::REMEMBER_EVERY).await;
            }
        });
        serve::serve(state, listener, tls, shutdown_signal()).await
    })
}

async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM can be watched");
    tokio::select! {
        _ = terminate.recv() => {},
        _ = tokio::signal::ctrl_c() => {},
    }
    eprintln!("shutting down");
}

fn pair(
    config: &Config,
    paths: &Paths,
    label: Option<String>,
    hosts: Vec<String>,
) -> io::Result<()> {
    let issued = pairing_code::issue(config, paths, label, hosts)?;
    println!("Paste this into ai-profiles → Settings → Remote hosts → Pair a host.");
    println!(
        "It works once, for the next {} minutes.\n",
        PAIRING_TTL.num_minutes()
    );
    println!("{}\n", issued.code);
    println!("Addresses:   {}", issued.hosts.join(", "));
    println!("Certificate: {}", issued.fingerprint);
    println!(
        "(ai-profiles shows the same certificate fingerprint before it pairs: check they match.)"
    );
    Ok(())
}

fn doctor(config: &Config, paths: &Paths) -> io::Result<()> {
    let mut problems = 0;
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "✓" } else { "✗" });
        if !ok {
            problems += 1;
        }
    };
    match hostinfo::tmux_version() {
        Some(version) => check(true, format!("tmux: {version}")),
        None => check(
            false,
            "tmux: not found (sessions can be listed but not started)".into(),
        ),
    }
    match hostinfo::claude_path(config) {
        Some(path) => check(true, format!("claude: {}", path.display())),
        None => check(
            false,
            "claude: not found (set claude_path in config.toml)".into(),
        ),
    }
    let accounts = accounts::discover(config);
    check(
        !accounts.is_empty(),
        format!(
            "accounts in {}: {}",
            config.accounts_base.display(),
            if accounts.is_empty() {
                "none".into()
            } else {
                accounts
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ),
    );
    check(true, format!("listen: {}", config.listen));
    for tunnel in ai_profiles_server::tunnels::wireguard() {
        let lonely = ai_profiles_server::tunnels::admits_no_peer(&tunnel);
        check(
            !lonely,
            if lonely {
                format!(
                    "WireGuard {}: {} is a single address, so no peer is accepted from it; add the peers' network to allow_from",
                    tunnel.interface, tunnel.network
                )
            } else {
                format!(
                    "WireGuard {}: {} (accepted)",
                    tunnel.interface, tunnel.network
                )
            },
        );
    }
    check(
        !config.allow_from.is_empty(),
        format!(
            "allow_from: {}",
            config
                .allow_from
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    let identity = Identity::load_or_create(paths.ensure_state_dir()?)?;
    check(
        true,
        format!(
            "certificate: {}",
            display_fingerprint(&identity.fingerprint)
        ),
    );
    let clients = Store::new(&paths.state_dir).clients()?.len();
    check(true, format!("paired clients: {clients}"));
    if let Some(lingering) = service::lingering() {
        check(
            lingering,
            format!(
                "linger: {}",
                if lingering {
                    "on"
                } else {
                    "off (the service stops when you log out)"
                }
            ),
        );
    }
    println!(
        "\nconfig: {}\nstate:  {}",
        paths.config_file().display(),
        paths.state_dir.display()
    );
    if problems > 0 {
        return Err(io::Error::other(format!("{problems} problem(s) found")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgets_the_claude_session_and_tmux_pane_it_was_started_from() {
        let names = [
            "CLAUDECODE",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_MESSAGING_SOCKET",
            "CLAUDE_PID",
            "TMUX",
            "TMUX_PANE",
            "CLAUDE_CONFIG_DIR",
            "PATH",
            "HOME",
            "TMUX_TMPDIR",
        ];
        let dropped: Vec<String> =
            inherited_session_variables(names.iter().map(std::ffi::OsString::from))
                .into_iter()
                .map(|name| name.to_string_lossy().into_owned())
                .collect();
        assert_eq!(
            dropped,
            [
                "CLAUDECODE",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_CODE_MESSAGING_SOCKET",
                "CLAUDE_PID",
                "TMUX",
                "TMUX_PANE"
            ]
        );
    }
}
