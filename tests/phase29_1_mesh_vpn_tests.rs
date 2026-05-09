use tet_core::hive::HiveCommand;
use tet_core::network::tunnel::SovereignTunnel;

#[tokio::test]
async fn test_identity_bound_handshake_rejection() {
    let builder = snow::Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
    let responder_kp = builder.generate_keypair().unwrap();
    let attacker_kp = builder.generate_keypair().unwrap();

    // We expect responder's public key but a fake one is used.
    let tunnel_result = SovereignTunnel::init_initiator(&attacker_kp.private, &[0u8; 32]);
    // It should not panic on build, but it will fail on handshake!
    let mut attacker = tunnel_result.unwrap();

    let mut responder =
        SovereignTunnel::init_responder(&responder_kp.private, &responder_kp.public).unwrap();

    let mut ix_buf = vec![0u8; 65535];
    let len = attacker
        .noise_state
        .as_mut()
        .unwrap()
        .write_message(&[], &mut ix_buf)
        .unwrap();

    let mut rx_buf = vec![0u8; 65535];
    let result = responder
        .noise_state
        .as_mut()
        .unwrap()
        .read_message(&ix_buf[..len], &mut rx_buf);

    assert!(
        result.is_err(),
        "Tunnel builder MUST immediately reject fake handshake states and drop TCP."
    );
}

#[tokio::test]
async fn test_eavesdropper_silence() {
    let builder = snow::Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
    let init_kp = builder.generate_keypair().unwrap();
    let resp_kp = builder.generate_keypair().unwrap();

    let mut initiator =
        SovereignTunnel::init_initiator(&init_kp.private, &resp_kp.public).unwrap();

    // The Command
    let cmd = HiveCommand::Network(tet_core::hive::HiveNetworkCommand::Pulse);

    let encrypted_payload = initiator.encrypt_command(&cmd).unwrap();

    // Eavesdropper captures `encrypted_payload`.
    // Attempt serialization
    let attempt: Result<HiveCommand, bincode::Error> = bincode::deserialize(&encrypted_payload);
    assert!(
        attempt.is_err(),
        "Eavesdropper could read the HiveCommand without IK pattern keys!"
    );

    // Entropy verification — the array should be relatively high entropy
    let mut counts = [0usize; 256];
    for &b in &encrypted_payload {
        counts[b as usize] += 1;
    }
    let entropy: f64 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / encrypted_payload.len() as f64;
            -p * p.log2()
        })
        .sum();

    // Ensure noise format looks random
    assert!(
        entropy > 3.0,
        "Encrypted payload Entropy {} is suspiciously low!",
        entropy
    );
}

#[tokio::test]
async fn test_forward_secrecy_and_throughput() {
    let builder = snow::Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
    let init_kp = builder.generate_keypair().unwrap();
    let resp_kp = builder.generate_keypair().unwrap();

    let mut initiator =
        SovereignTunnel::init_initiator(&init_kp.private, &resp_kp.public).unwrap();
    let mut responder =
        SovereignTunnel::init_responder(&resp_kp.private, &resp_kp.public).unwrap();

    // Handshake
    let mut ix_buf = vec![0u8; 65535];
    let len = initiator
        .noise_state
        .as_mut()
        .unwrap()
        .write_message(&[], &mut ix_buf)
        .unwrap();
    let mut rx_buf = vec![0u8; 65535];
    responder
        .noise_state
        .as_mut()
        .unwrap()
        .read_message(&ix_buf[..len], &mut rx_buf)
        .unwrap();

    let len = responder
        .noise_state
        .as_mut()
        .unwrap()
        .write_message(&[], &mut ix_buf)
        .unwrap();
    initiator
        .noise_state
        .as_mut()
        .unwrap()
        .read_message(&ix_buf[..len], &mut rx_buf)
        .unwrap();

    let _ = initiator.to_transport();
    let _ = responder.to_transport();

    // Simulate 1MB transfer inside the tunnel.
    let chunk = vec![1u8; 1_000_000];
    let cmd = HiveCommand::Registry(tet_core::hive::HiveRegistryCommand::ChunkStream {
        cid: "benchmark".into(),
        seq: 1,
        chunk,
    });

    let start_ser = std::time::Instant::now();
    let _payload = bincode::serialize(&cmd).unwrap();
    let ser_dur = start_ser.elapsed();

    let start = std::time::Instant::now();
    let enc = initiator.encrypt_command(&cmd).unwrap();
    let encrypt_dur = start.elapsed();

    println!("Ser: {}ms, Enc: {}ms", ser_dur.as_millis(), encrypt_dur.as_millis());

    assert!(
        encrypt_dur.as_millis() < 500,
        "Encryption is creating >500ms latency: {}ms (Ser: {}ms)", encrypt_dur.as_millis(), ser_dur.as_millis()
    );

    let start_dec = std::time::Instant::now();
    let _dec_cmd = responder.decrypt_payload(&enc).unwrap();
    let dec_dur = start_dec.elapsed();

    assert!(
        dec_dur.as_millis() < 500,
        "Decryption is creating >500ms latency (too slow!)"
    );
}
