//! Trust-on-first-use certificate pinning.
//!
//! Cameras present self-signed certificates. We never add them to a global trust
//! store: we record the SHA-256 of the leaf cert the camera presented, the operator
//! accepts it once, and every later connection must present that exact certificate.

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::{DigitallySignedStruct, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of a DER certificate — what the operator sees and accepts.
pub fn fingerprint_of(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone, Debug)]
pub enum Trust {
    /// Accept whatever is presented and record it. For the initial probe only.
    OnFirstUse,
    /// Accept only this exact fingerprint.
    Pinned(String),
}

#[derive(Debug)]
struct Verifier {
    trust: Trust,
    /// The fingerprint actually presented, for the caller to show the operator.
    seen: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let seen = fingerprint_of(end_entity);
        *self.seen.lock().expect("fingerprint lock") = Some(seen.clone());

        match &self.trust {
            Trust::OnFirstUse => Ok(ServerCertVerified::assertion()),
            // Constant-time comparison is pointless here: the expected value is
            // stored locally and the attacker already controls what they present.
            Trust::Pinned(expected) if expected.eq_ignore_ascii_case(&seen) => {
                Ok(ServerCertVerified::assertion())
            }
            Trust::Pinned(_) => Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch (presented {seen})"
            ))),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
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
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
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

/// A TLS config pinned per the given trust mode, plus a handle to read back the
/// fingerprint the camera presented once a connection has been attempted.
pub fn client_config(trust: Trust) -> (rustls::ClientConfig, Arc<Mutex<Option<String>>>) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let seen = Arc::new(Mutex::new(None));

    let config = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Verifier {
            trust,
            seen: Arc::clone(&seen),
            provider,
        }))
        .with_no_client_auth();

    (config, seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_lowercase_hex_sha256() {
        assert_eq!(
            fingerprint_of(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
