//! Trusting an ai-profiles-server by its certificate's fingerprint.
//!
//! The server signs its own certificate, so there is no CA to check against.
//! What stands in for one is the fingerprint the pairing code carried: a
//! connection is accepted only if the server presents exactly that
//! certificate, and the handshake is still verified against it, so holding a
//! copy of the certificate without its key gets an impostor nowhere.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Accepts one certificate, named by its SHA-256.
#[derive(Debug)]
pub struct PinnedServerCertVerifier {
    fingerprint: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl PinnedServerCertVerifier {
    /// `None` when `fingerprint_hex` isn't 64 hex digits.
    pub fn new(fingerprint_hex: &str, provider: Arc<CryptoProvider>) -> Option<Self> {
        let mut fingerprint = [0u8; 32];
        if fingerprint_hex.len() != 64 {
            return None;
        }
        for (index, byte) in fingerprint.iter_mut().enumerate() {
            *byte = u8::from_str_radix(fingerprint_hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(PinnedServerCertVerifier {
            fingerprint,
            provider,
        })
    }
}

impl ServerCertVerifier for PinnedServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let presented = Sha256::digest(end_entity.as_ref());
        if bool::from(presented.as_slice().ct_eq(&self.fingerprint)) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A TLS 1.3 client config that trusts only the certificate with this
/// fingerprint. `None` when the fingerprint isn't one.
pub fn pinned_client_config(fingerprint_hex: &str) -> Option<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedServerCertVerifier::new(fingerprint_hex, provider.clone())?;
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_well_formed_fingerprints() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let good = crate::pairing::fingerprint(b"x");
        assert!(PinnedServerCertVerifier::new(&good, provider.clone()).is_some());
        assert!(PinnedServerCertVerifier::new("abc", provider.clone()).is_none());
        assert!(PinnedServerCertVerifier::new(&"zz".repeat(32), provider).is_none());
        assert!(pinned_client_config(&good).is_some());
    }

    #[test]
    fn accepts_the_pinned_certificate_and_nothing_else() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let pinned = CertificateDer::from(b"the server's certificate".to_vec());
        let other = CertificateDer::from(b"someone else's certificate".to_vec());
        let verifier =
            PinnedServerCertVerifier::new(&crate::pairing::fingerprint(&pinned), provider).unwrap();
        let name = ServerName::try_from("anything").unwrap();
        let now = UnixTime::now();
        assert!(verifier
            .verify_server_cert(&pinned, &[], &name, &[], now)
            .is_ok());
        assert!(verifier
            .verify_server_cert(&other, &[], &name, &[], now)
            .is_err());
    }
}
