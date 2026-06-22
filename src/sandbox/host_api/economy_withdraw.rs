//! `withdraw` — bridge fuel to an external asset chain.
use super::helpers::{get_memory, read_guest_str};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "withdraw",
            |mut caller: Caller<'_, TetState>, (amount, addr_ptr, addr_len): (i64, i32, i32)| -> Box<
                dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_,
            > {
                Box::new(async move {
                    let amount = amount as u64;
                    let memory = get_memory(&mut caller)?;
                    let target_address = read_guest_str(&memory, &caller, addr_ptr, addr_len)?;

                    let max_fuel = caller.get_fuel().unwrap_or(0);
                    if amount > max_fuel {
                        return Ok(5);
                    }
                    let _ = caller.set_fuel(max_fuel - amount);

                    let source_alias = caller.data().manifest.metadata.name.clone();
                    let mesh = caller.data().mesh.clone();

                    let sig = sign_bridge_intent(&source_alias, amount, &target_address);
                    let intent = crate::economy::bridge::BridgeIntent {
                        internal_fuel: amount,
                        external_asset: "ETH".to_string(),
                        target_address,
                        agent_signature: sig,
                    };

                    let pkt = crate::hive::HiveCommand::Economy(
                        crate::hive::HiveEconomyCommand::WithdrawalPending(intent),
                    );
                    if mesh.send_economy_packet(pkt).await.is_err() {
                        let _ = caller.set_fuel(max_fuel);
                        return Ok(6);
                    }
                    Ok(0)
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::withdraw: {e:#}")))?;
    Ok(())
}

fn sign_bridge_intent(source_alias: &str, amount: u64, target_address: &str) -> Vec<u8> {
    use ed25519_dalek::Signer;
    use sha2::Digest;
    let seed = sha2::Sha256::digest(source_alias.as_bytes());
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed.into());
    let mut data = Vec::new();
    data.extend_from_slice(&amount.to_be_bytes());
    data.extend_from_slice(b"ETH");
    data.extend_from_slice(target_address.as_bytes());
    signing_key.sign(&data).to_bytes().to_vec()
}
