use rustls::{
    pki_types::CertificateDer, pki_types::PrivateKeyDer, server::WebPkiClientVerifier,
    ClientConfig, RootCertStore, ServerConfig,
};
use std::sync::Arc;
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Failed to parse certificate: {0}")]
    CertError(String),
    #[error("Failed to configure rustls: {0}")]
    TlsError(#[from] rustls::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn create_secure_hive_server(
    cert_chain: Vec<CertificateDer<'static>>,
    key_der: PrivateKeyDer<'static>,
    trusted_roots: Option<RootCertStore>,
) -> Result<TlsAcceptor, SecurityError> {
    let builder = ServerConfig::builder();
    let mut config = if let Some(roots) = trusted_roots {
        let client_auth = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| SecurityError::CertError(e.to_string()))?;
        builder
            .with_client_cert_verifier(client_auth)
            .with_single_cert(cert_chain, key_der)?
    } else {
        builder
            .with_no_client_auth()
            .with_single_cert(cert_chain, key_der)?
    };

    config.alpn_protocols = vec![b"trytet-hive-v1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub fn create_secure_hive_client(
    trusted_roots: RootCertStore,
    client_cert: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> Result<TlsConnector, SecurityError> {
    let builder = ClientConfig::builder().with_root_certificates(trusted_roots);

    let mut config = if let Some((cert_chain, key_der)) = client_cert {
        builder.with_client_auth_cert(cert_chain, key_der)?
    } else {
        builder.with_no_client_auth()
    };

    config.alpn_protocols = vec![b"trytet-hive-v1".to_vec()];

    Ok(TlsConnector::from(Arc::new(config)))
}
