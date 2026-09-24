//! The server's TLS identity: a self-signed certificate made on first run and
//! kept in `<state>/tls/`. Clients don't check it against a CA; they pin its
//! fingerprint, which a pairing code carries. So it never needs renewing, and
//! making a new one (delete the folder) means pairing every client again.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use ai_profiles_core::pairing::fingerprint;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

pub struct Identity {
    pub cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    /// SHA-256 of the certificate, lowercase hex.
    pub fingerprint: String,
}

impl Identity {
    /// The identity in `<state_dir>/tls/`, made there first if there is none.
    pub fn load_or_create(state_dir: &Path) -> io::Result<Identity> {
        let dir = state_dir.join("tls");
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        if !cert_path.exists() || !key_path.exists() {
            fs::create_dir_all(&dir)?;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
            let hostname = crate::hostinfo::hostname();
            let made =
                rcgen::generate_simple_self_signed(vec![hostname, "ai-profiles-server".into()])
                    .map_err(|err| io::Error::other(format!("making a certificate: {err}")))?;
            let staged_key = dir.join(".key.pem.tmp");
            fs::write(&staged_key, made.key_pair.serialize_pem())?;
            fs::set_permissions(&staged_key, fs::Permissions::from_mode(0o600))?;
            fs::rename(&staged_key, &key_path)?;
            fs::write(&cert_path, made.cert.pem())?;
        }
        let cert = CertificateDer::from_pem_file(&cert_path)
            .map_err(|err| io::Error::other(format!("{}: {err}", cert_path.display())))?;
        let key = PrivateKeyDer::from_pem_file(&key_path)
            .map_err(|err| io::Error::other(format!("{}: {err}", key_path.display())))?;
        Ok(Identity {
            fingerprint: fingerprint(cert.as_ref()),
            cert,
            key,
        })
    }

    /// TLS 1.3 only, HTTP/1.1.
    pub fn server_config(&self) -> io::Result<Arc<rustls::ServerConfig>> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?
            .with_no_client_auth()
            .with_single_cert(vec![self.cert.clone()], self.key.clone_key())
            .map_err(io::Error::other)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Arc::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_an_identity_once_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let first = Identity::load_or_create(dir.path()).unwrap();
        let second = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.fingerprint.len(), 64);
        let key_mode = fs::metadata(dir.path().join("tls/key.pem"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(key_mode & 0o777, 0o600);
        assert!(first.server_config().is_ok());
    }
}
