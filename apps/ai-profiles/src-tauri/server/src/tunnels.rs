//! WireGuard tunnels on this machine, for reaching the server over a VPN
//! other than Tailscale.
//!
//! Linux labels a WireGuard interface in `/sys/class/net/<name>/uevent`
//! with `DEVTYPE=wireguard`, whatever it is called (`wg0`, `vpn`, …). Its
//! addresses say where a paired Mac can reach the server, and its networks
//! who is on the other end: WireGuard peers have proved who they are with
//! their keys, as Tailscale's have, so their networks are accepted by
//! default, next to Tailscale's.

use std::fs;
use std::net::IpAddr;

use if_addrs::IfAddr;
use ipnet::IpNet;

/// One address of a WireGuard interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tunnel {
    pub interface: String,
    /// This machine's address on it.
    pub address: IpAddr,
    /// The network the address is in: the peers' addresses, when the
    /// interface is given one wider than a single address.
    pub network: IpNet,
}

/// Every address of every WireGuard interface, link-local ones left out.
pub fn wireguard() -> Vec<Tunnel> {
    let names = wireguard_interfaces();
    if names.is_empty() {
        return Vec::new();
    }
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    interfaces
        .into_iter()
        .filter(|interface| names.contains(&interface.name) && !interface.is_link_local())
        .filter_map(|interface| {
            let (address, prefix) = match &interface.addr {
                IfAddr::V4(v4) => (IpAddr::V4(v4.ip), v4.prefixlen),
                IfAddr::V6(v6) => (IpAddr::V6(v6.ip), v6.prefixlen),
            };
            Some(Tunnel {
                network: IpNet::new(address, prefix).ok()?.trunc(),
                interface: interface.name,
                address,
            })
        })
        .collect()
}

/// The names of the WireGuard interfaces, from `/sys/class/net`. None where
/// there's no such folder (not Linux).
fn wireguard_interfaces() -> Vec<String> {
    let Ok(entries) = fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            fs::read_to_string(entry.path().join("uevent")).is_ok_and(|text| is_wireguard(&text))
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

/// Pure: whether an interface's `uevent` says it's WireGuard.
fn is_wireguard(uevent: &str) -> bool {
    uevent
        .lines()
        .any(|line| line.trim() == "DEVTYPE=wireguard")
}

/// Pure: whether a tunnel's network is only its own address (`/32`, `/128`),
/// so it admits no peer: WireGuard is often set up that way, with the
/// peers' addresses only in its routes.
pub fn admits_no_peer(tunnel: &Tunnel) -> bool {
    tunnel.network.prefix_len() == tunnel.network.max_prefix_len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_a_wireguard_interface_by_its_devtype() {
        assert!(is_wireguard(
            "DEVTYPE=wireguard\nINTERFACE=wg0\nIFINDEX=7\n"
        ));
        assert!(!is_wireguard("INTERFACE=eth0\nIFINDEX=2\n"));
        assert!(!is_wireguard("DEVTYPE=bridge\nINTERFACE=docker0\n"));
    }

    #[test]
    fn says_when_a_tunnel_admits_no_peer() {
        let tunnel = |address: &str, network: &str| Tunnel {
            interface: "wg0".into(),
            address: address.parse().unwrap(),
            network: network.parse().unwrap(),
        };
        assert!(!admits_no_peer(&tunnel("10.8.0.1", "10.8.0.0/24")));
        assert!(admits_no_peer(&tunnel("10.8.0.2", "10.8.0.2/32")));
        assert!(admits_no_peer(&tunnel("fd00::2", "fd00::2/128")));
    }
}
