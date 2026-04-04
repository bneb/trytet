use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use tet_core::economy::VoucherManager;
use tet_core::hive::security::{create_secure_hive_client, create_secure_hive_server};
use tet_core::hive::{HiveClient, HiveCommand, HivePeers, HiveServer};
use tet_core::mesh::TetMesh;
use tet_core::sandbox::WasmtimeSandbox;
use tokio::net::TcpListener;

fn generate_cert(
    subject_alt_names: Vec<String>,
) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let key = PrivateKeyDer::Pkcs8(cert.serialize_private_key_der().into());
    let cert_der = CertificateDer::from(cert.serialize_der().unwrap());
    (vec![cert_der], key)
}

#[tokio::test]
async fn test_untrusted_peer_rejected() {
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Generate Root & Certs for Target Node
    let target_names = vec!["localhost".to_string()];
    let (target_cert_chain, target_key) = generate_cert(target_names.clone());

    // We create a root store with just the target's self-signed cert as trusted root
    let mut trusted_roots = rustls::RootCertStore::empty();
    trusted_roots.add(target_cert_chain[0].clone()).unwrap();

    let tls_acceptor = create_secure_hive_server(
        target_cert_chain.clone(),
        target_key,
        Some(trusted_roots.clone()),
    )
    .unwrap();

    // 2. Start Target Hive Server with mTLS
    let hive_peers = HivePeers::new();
    let (mesh, _call_rx) = TetMesh::new(10, hive_peers.clone());
    let voucher_manager = Arc::new(VoucherManager::new("target".to_string()));
    let target_sandbox = Arc::new(
        WasmtimeSandbox::new(mesh.clone(), voucher_manager, false, "target".to_string()).unwrap(),
    );

    // Listen on free port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = listener.local_addr().unwrap().port();
    drop(listener);

    let target_server = HiveServer::new(hive_peers.clone(), None, Some(tls_acceptor));
    let target_sandbox_clone = target_sandbox.clone();

    let target_mesh = mesh.clone();
    tokio::spawn(async move {
        target_server
            .start(target_port, target_mesh, target_sandbox_clone)
            .await
            .unwrap();
    });
    // tet_core::mesh_worker::spawn_mesh_worker(target_sandbox.clone(), call_rx);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 3. Generate a ROGUE Client Cert (Not trusted by the target root store)
    let (rogue_cert_chain, rogue_key) = generate_cert(vec!["rogue.local".to_string()]);

    // Create malicious connector using rogue cert but trusting the server's root
    let tls_connector =
        create_secure_hive_client(trusted_roots, Some((rogue_cert_chain, rogue_key))).unwrap();

    // 4. Client attempts to connect
    let cmd = HiveCommand::Pulse;
    let target_addr = format!("127.0.0.1:{}", target_port);

    let result =
        HiveClient::send_command_tls(&target_addr, cmd, Some(tls_connector), Some("localhost"))
            .await;

    // 5. Verify the connection was rejected
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    println!("Expected TLS error: {}", err_str);
}
