use std::sync::Arc;
use tet_core::economy::VoucherManager;
use tet_core::hive::{HivePeers, HiveServer};
use tet_core::mesh::TetMesh;
use tet_core::sandbox::WasmtimeSandbox;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn test_hive_deserialization_bomb() {
    let peers = HivePeers::new();
    let server = HiveServer::new(peers.clone(), None, None);

    let (mesh, _rx) = TetMesh::new(100, peers);
    let vm = Arc::new(VoucherManager::new("test_provider".to_string()));
    let sandbox = Arc::new(
        WasmtimeSandbox::new(mesh.clone(), vm, false, "test_node_id".to_string()).unwrap(),
    );

    // Bind to arbitrary port
    let s_mesh = mesh.clone();
    let s_sandbox = sandbox.clone();
    tokio::spawn(async move {
        let _ = server.start(34599, s_mesh, s_sandbox).await;
    });

    // Give it a split second to start
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Connect manually
    let mut stream = TcpStream::connect("127.0.0.1:34599")
        .await
        .expect("Failed to connect to test server");

    use tet_core::network::tunnel::SovereignTunnel;
    let mut tunnel = SovereignTunnel::init_initiator_nn().unwrap();

    let mut ix_buf = vec![0u8; 65535];
    let len = tunnel.noise_state.as_mut().unwrap().write_message(&[], &mut ix_buf).unwrap();
    stream.write_all(&(len as u32).to_be_bytes()).await.unwrap();
    stream.write_all(&ix_buf[..len]).await.unwrap();

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut resp_payload = vec![0u8; resp_len];
    stream.read_exact(&mut resp_payload).await.unwrap();

    let mut rx_buf = vec![0u8; 65535];
    tunnel.noise_state.as_mut().unwrap().read_message(&resp_payload, &mut rx_buf).unwrap();
    tunnel.to_transport().unwrap();

    // Construct a malicious payload
    // Command is ResolveAlias(String)
    // Tag: 2u32 (Wait, the tag in bincode might be different, but let's just make it huge)
    let mut payload = Vec::new();
    payload.extend_from_slice(&3u32.to_le_bytes()); // ResolveAlias tag (index 3)
    payload.extend_from_slice(&(2_u64 * 1024 * 1024 * 1024).to_le_bytes()); // 2GB string
    payload.extend_from_slice(b"fake data because it will never read this far");

    // We must encrypt this manually since it's not a HiveCommand
    // encrypt_command takes a HiveCommand, so we copy its logic:
    let mut final_out = Vec::new();
    let chunk_size = 65000;
    for chunk in payload.chunks(chunk_size) {
        let mut out = vec![0u8; chunk.len() + 1024];
        let len = tunnel.transport.as_mut().unwrap().write_message(chunk, &mut out).unwrap();
        final_out.extend_from_slice(&(len as u32).to_be_bytes());
        final_out.extend_from_slice(&out[..len]);
    }

    let final_len = final_out.len() as u32;
    stream.write_all(&final_len.to_be_bytes()).await.unwrap();
    stream.write_all(&final_out).await.unwrap();

    // With limits, the server should reject connection before OOM panic
    let mut buf = [0u8; 4];
    let res = stream.read_exact(&mut buf).await;
    let mut resp = vec![0; 5];
    let _ = stream.read_exact(&mut resp).await;
    println!("result: {:?}, buf: {:?}, resp: {:?}", res, buf, resp);
    assert!(
        res.is_err(),
        "Server should have dropped the connection due to payload limits"
    );
}
