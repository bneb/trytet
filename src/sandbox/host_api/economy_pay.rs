//! `pay` — transfer fuel from this agent to another.
use super::helpers::{get_memory, read_guest_str};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "pay",
            |mut caller: Caller<'_, TetState>, (target_ptr, target_len, amount): (i32, i32, i64)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = get_memory(&mut caller)?;
                    let target_alias = read_guest_str(&memory, &caller, target_ptr, target_len)?;

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if amount > max_fuel {
                        return Ok(5);
                    }
                    let _ = caller.set_fuel(max_fuel - amount);

                    let source_alias = caller.data().manifest.metadata.name.clone();
                    let mesh = caller.data().mesh.clone();
                    let keys = derive_keys(&source_alias, &target_alias);

                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("system clock before unix epoch")
                        .as_nanos() as u64;

                    let sig = sign_transfer(&keys.signing_key, &keys.pub_a, &keys.pub_b, amount, nonce);

                    let tx = crate::economy::registry::FuelTransaction {
                        from: keys.pub_a,
                        to: keys.pub_b,
                        amount,
                        nonce,
                        signature: sig,
                    };

                    let pkt = crate::hive::HiveCommand::Economy(
                        crate::hive::HiveEconomyCommand::TransferCredit(tx),
                    );
                    let _ = mesh.send_economy_packet(pkt).await;
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::pay: {e:#}")))?;
    Ok(())
}

struct DerivedKeys {
    signing_key: ed25519_dalek::SigningKey,
    pub_a: Vec<u8>,
    pub_b: Vec<u8>,
}

fn derive_keys(source_alias: &str, target_alias: &str) -> DerivedKeys {
    use sha2::Digest;
    let seed_a = sha2::Sha256::digest(source_alias.as_bytes());
    let seed_b = sha2::Sha256::digest(target_alias.as_bytes());
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed_a.into());
    let pub_a = signing_key.verifying_key().to_bytes().to_vec();
    let pub_b = ed25519_dalek::SigningKey::from_bytes(&seed_b.into())
        .verifying_key()
        .to_bytes()
        .to_vec();
    DerivedKeys {
        signing_key,
        pub_a,
        pub_b,
    }
}

fn sign_transfer(
    key: &ed25519_dalek::SigningKey,
    pub_a: &[u8],
    pub_b: &[u8],
    amount: u64,
    nonce: u64,
) -> Vec<u8> {
    use ed25519_dalek::Signer;
    let mut data = Vec::new();
    data.extend_from_slice(pub_a);
    data.extend_from_slice(pub_b);
    data.extend_from_slice(&amount.to_be_bytes());
    data.extend_from_slice(&nonce.to_be_bytes());
    key.sign(&data).to_bytes().to_vec()
}
