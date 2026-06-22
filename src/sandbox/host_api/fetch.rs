//! `fetch` — HTTP egress with security policy enforcement.
use super::helpers::{get_memory, read_guest_bytes, read_guest_str, write_response};
use super::TetState;
use crate::engine::TetError;
use wasmtime::Caller;

pub fn register(linker: &mut wasmtime::Linker<TetState>) -> Result<(), TetError> {
    linker
        .func_wrap_async(
            "trytet",
            "fetch",
            move |mut caller: Caller<'_, TetState>,
                  (url_ptr, url_len, method_ptr, method_len, body_ptr, body_len, out_ptr, out_len_ptr): (
                i32, i32, i32, i32, i32, i32, i32, i32,
            )|
                  -> Box<dyn std::future::Future<Output = wasmtime::Result<i32>> + Send + '_> {
                Box::new(async move {
                    let memory = get_memory(&mut caller)?;
                    let target_url = read_guest_str(&memory, &caller, url_ptr, url_len)?;
                    let req_method_str = read_guest_str(&memory, &caller, method_ptr, method_len)?;
                    let req_body = read_guest_bytes(&memory, &caller, body_ptr, body_len)?;

                    enforce_egress_policy(&caller, &target_url)?;
                    charge_preflight_fuel(&mut caller, &target_url, &req_method_str, &req_body)?;

                    let quota_result = check_egress_quota(&caller, &target_url, &req_method_str, &req_body);
                    if quota_result.is_err() {
                        return Ok(8);
                    }

                    let oracle = caller.data().oracle.clone();
                    let cache_dir = caller.data().oracle_cache_dir.clone();
                    let identity_headers = build_identity_headers(&caller, &req_method_str, &target_url, &req_body);
                    let oracle_req = crate::oracle::OracleRequest {
                        url: target_url.clone(),
                        method: req_method_str,
                        body: req_body,
                    };

                    let (status_code, returned_bytes) = oracle
                        .resolve_with_headers(oracle_req, &cache_dir, identity_headers)
                        .await
                        .unwrap_or((500, vec![]));

                    let _ = caller.data().quota_manager.check_and_record(
                        &caller.data().tenant_id,
                        returned_bytes.len() as u64,
                        caller.data().max_egress_bytes,
                    );

                    charge_response_fuel(&mut caller, returned_bytes.len() as u64)?;

                    let success_code = if (200..400).contains(&status_code) { 0_i32 } else { 6_i32 };
                    if success_code == 0 {
                        let memory = get_memory(&mut caller)?;
                        write_response(&memory, &mut caller, out_ptr, out_len_ptr, &returned_bytes)
                    } else {
                        Ok(success_code)
                    }
                })
            },
        )
        .map_err(|e| TetError::EngineError(format!("Failed to register trytet::fetch: {e:#}")))?;
    Ok(())
}

fn enforce_egress_policy(caller: &Caller<'_, TetState>, url: &str) -> wasmtime::Result<()> {
    if !url.starts_with("http") {
        let jailer = crate::sandbox::security::PathJailer::new(std::path::PathBuf::from(
            "/vfs/Agent_Workspace_Root",
        ));
        jailer
            .safe_join(url)
            .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
    }
    let policy = caller
        .data()
        .egress_policy
        .as_ref()
        .ok_or_else(|| wasmtime::Error::msg("Security Violation: No EgressPolicy assigned"))?;
    if policy.require_https && !url.starts_with("https://") {
        return Err(wasmtime::Error::msg("Security Violation: HTTPS required"));
    }
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| wasmtime::Error::msg("Security Violation: Unparseable URI"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| wasmtime::Error::msg("Security Violation: No hostname"))?;
    if !policy.allowed_domains.contains(&host.to_string()) {
        return Err(wasmtime::Error::msg(format!(
            "Security Violation: Domain '{}' not in allow list",
            host
        )));
    }
    Ok(())
}

fn charge_preflight_fuel(
    caller: &mut Caller<'_, TetState>,
    url: &str,
    method: &str,
    body: &[u8],
) -> wasmtime::Result<()> {
    let req_size = url.len() as u64 + method.len() as u64 + body.len() as u64;
    let cost = 50_000 + (req_size / 1024) * 10;
    deduct_fuel(caller, cost)
}

fn charge_response_fuel(
    caller: &mut Caller<'_, TetState>,
    response_bytes: u64,
) -> wasmtime::Result<()> {
    let cost = (response_bytes / 1024) * 10;
    deduct_fuel(caller, cost)
}

fn deduct_fuel(caller: &mut Caller<'_, TetState>, amount: u64) -> wasmtime::Result<()> {
    let current = caller.get_fuel().unwrap_or(0);
    if current < amount {
        let _ = caller.set_fuel(0);
        return Ok(());
    }
    let _ = caller.set_fuel(current - amount);
    Ok(())
}

fn check_egress_quota(
    caller: &Caller<'_, TetState>,
    url: &str,
    method: &str,
    body: &[u8],
) -> Result<(), ()> {
    let req_size = url.len() as u64 + method.len() as u64 + body.len() as u64;
    let header_overhead = crate::fortress::IdentityHeaders::header_overhead(
        &caller.data().tet_id,
        &caller.data().author_pubkey,
    );
    caller
        .data()
        .quota_manager
        .check_and_record(
            &caller.data().tenant_id,
            req_size + header_overhead,
            caller.data().max_egress_bytes,
        )
        .map_err(|_| ())
}

fn build_identity_headers(
    caller: &Caller<'_, TetState>,
    method: &str,
    url: &str,
    body: &[u8],
) -> Vec<(String, String)> {
    crate::fortress::IdentityHeaders::inject(
        &caller.data().tet_id,
        &caller.data().author_pubkey,
        &caller.data().oracle.wallet,
        method,
        url,
        body,
    )
}
