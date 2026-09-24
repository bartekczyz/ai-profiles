//! The pairing code `ai-profiles-server pair` prints and ai-profiles reads:
//! `aip1.` + base64url(JSON) holding where the server listens, a one-time
//! secret, and the fingerprint of the server's certificate.
//!
//! The fingerprint is what makes the first connection safe on any network:
//! the client accepts only the certificate it names, so there is no moment
//! where it has to trust whatever answers.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PREFIX: &str = "aip1.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    pub v: u32,
    /// `host:port` candidates, best first (Tailscale address, then name, …).
    pub hosts: Vec<String>,
    /// Single use, exchanged for a token by `POST /v1/pair`.
    pub secret: String,
    /// SHA-256 of the server certificate (DER), lowercase hex.
    pub fp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingError {
    /// Not an `aip1.` code at all.
    NotACode,
    /// The right prefix, but not something this version can read.
    Malformed,
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            PairingError::NotACode => "that isn't an ai-profiles pairing code",
            PairingError::Malformed => "the pairing code is damaged or from a newer server",
        })
    }
}

impl std::error::Error for PairingError {}

pub fn encode(code: &PairingCode) -> String {
    let json = serde_json::to_vec(code).expect("a pairing code always serializes");
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
}

/// Read a pairing code, tolerating the whitespace a copy and paste adds.
pub fn decode(text: &str) -> Result<PairingCode, PairingError> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let body = compact.strip_prefix(PREFIX).ok_or(PairingError::NotACode)?;
    let json = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| PairingError::Malformed)?;
    let code: PairingCode = serde_json::from_slice(&json).map_err(|_| PairingError::Malformed)?;
    let fingerprint_ok = code.fp.len() == 64
        && code
            .fp
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if code.v != 1 || code.hosts.is_empty() || code.secret.is_empty() || !fingerprint_ok {
        return Err(PairingError::Malformed);
    }
    Ok(code)
}

/// SHA-256 of a DER certificate, lowercase hex: what a code carries and what
/// the client pins.
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A fingerprint as people compare it: `AB:CD:…`.
pub fn display_fingerprint(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| String::from_utf8_lossy(pair).to_uppercase())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> PairingCode {
        PairingCode {
            v: 1,
            hosts: vec!["100.110.48.97:7443".into(), "xjopa1:7443".into()],
            secret: "s3cret".into(),
            fp: fingerprint(b"certificate"),
        }
    }

    #[test]
    fn round_trips_and_survives_pasted_whitespace() {
        let encoded = encode(&code());
        assert!(encoded.starts_with("aip1."));
        let pasted = format!("  {}\n{}  ", &encoded[..20], &encoded[20..]);
        assert_eq!(decode(&pasted).unwrap(), code());
    }

    #[test]
    fn rejects_what_is_not_a_code_or_is_damaged() {
        assert_eq!(decode("hello"), Err(PairingError::NotACode));
        assert_eq!(decode("aip1.!!!"), Err(PairingError::Malformed));
        let mut bad = code();
        bad.fp = "zz".into();
        assert_eq!(decode(&encode(&bad)), Err(PairingError::Malformed));
        let mut newer = code();
        newer.v = 2;
        assert_eq!(decode(&encode(&newer)), Err(PairingError::Malformed));
    }

    #[test]
    fn fingerprints_read_as_people_compare_them() {
        let hex = fingerprint(b"x");
        assert_eq!(hex.len(), 64);
        let shown = display_fingerprint(&hex);
        assert_eq!(shown.len(), 64 + 31);
        assert!(shown.starts_with(&hex[..2].to_uppercase()));
    }
}
