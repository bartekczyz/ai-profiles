//! Issuing a pairing code: a single-use secret, the addresses the Mac can
//! reach the server at, and the certificate's fingerprint, in one string to
//! paste into ai-profiles. `pair` prints one, and `setup` does too.

use std::io;

use ai_profiles_core::pairing::{self, display_fingerprint, PairingCode};

use crate::certs::Identity;
use crate::config::{Config, Paths};
use crate::hostinfo;
use crate::store::{random_token, Store};

/// A pairing code, and what it holds, to show next to it.
pub struct Issued {
    pub code: String,
    /// `host:port`, in the order the Mac tries them.
    pub hosts: Vec<String>,
    /// `AB:CD:…`
    pub fingerprint: String,
}

/// Issue a code for a Mac to pair with, named `label` when given. `hosts`
/// are the addresses to offer, host or host:port; by default the Tailscale
/// and WireGuard addresses and the host name.
pub fn issue(
    config: &Config,
    paths: &Paths,
    label: Option<String>,
    hosts: Vec<String>,
) -> io::Result<Issued> {
    let state_dir = paths.ensure_state_dir()?;
    let identity = Identity::load_or_create(state_dir)?;
    let port = config.listen.port();
    let hosts: Vec<String> = if hosts.is_empty() {
        hostinfo::reachable_hosts()
    } else {
        hosts
    }
    .into_iter()
    .map(|host| hostinfo::with_port(&host, port))
    .collect();
    let secret = random_token(18);
    Store::new(state_dir).add_pending(&secret, label)?;
    let code = pairing::encode(&PairingCode {
        v: 1,
        hosts: hosts.clone(),
        secret,
        fp: identity.fingerprint.clone(),
    });
    Ok(Issued {
        code,
        hosts,
        fingerprint: display_fingerprint(&identity.fingerprint),
    })
}
