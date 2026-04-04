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

    // Construct a malicious payload
    // Command is ResolveAlias(String)
    // Tag: 2u32
    // String length: 2GB (an absurd length that will trigger OOM allocator panic safely with bincode options)
    let mut payload = Vec::new();
    payload.extend_from_slice(&2u32.to_le_bytes()); // ResolveAlias tag
    payload.extend_from_slice(&(2_u64 * 1024 * 1024 * 1024).to_le_bytes()); // 2GB string
    payload.extend_from_slice(b"fake data because it will never read this far");

    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes()).await.unwrap();
    stream.write_all(&payload).await.unwrap();

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
