//! Common helpers for host function implementations.
//!
//! Every host function reads guest linear memory via pointer+length pairs
//! and writes results back the same way. These helpers eliminate the
//! repetitive OOB-check + copy patterns.

use crate::sandbox::sandbox_wasmtime::{validate_range, validate_range_mut, TetState};
use wasmtime::Caller;

/// Read a `(ptr, len)` string from guest memory.
pub fn read_guest_str(
    memory: &wasmtime::Memory,
    caller: &Caller<'_, TetState>,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<String> {
    let bytes = validate_range(memory, caller, ptr, len)?;
    Ok(String::from_utf8_lossy(bytes).to_string())
}

/// Read raw bytes from guest memory.
pub fn read_guest_bytes(
    memory: &wasmtime::Memory,
    caller: &Caller<'_, TetState>,
    ptr: i32,
    len: i32,
) -> wasmtime::Result<Vec<u8>> {
    validate_range(memory, caller, ptr, len).map(|s| s.to_vec())
}

/// Resolve the exported memory from a caller, or return an error.
/// Requires `&mut` because `get_export` may need mutable access in some wasmtime versions.
pub fn get_memory(caller: &mut Caller<'_, TetState>) -> wasmtime::Result<wasmtime::Memory> {
    caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("No memory exported"))
}

/// Write response bytes into guest memory via `(out_ptr, out_len_ptr)`.
///
/// Return codes:
/// - 0 on success
/// - 2 if the guest buffer is too small (required size written to out_len_ptr)
pub fn write_response(
    memory: &wasmtime::Memory,
    caller: &mut Caller<'_, TetState>,
    out_ptr: i32,
    out_len_ptr: i32,
    data: &[u8],
) -> wasmtime::Result<i32> {
    let response_len = data.len() as i32;
    let guest_buffer_size = read_i32_le(memory, caller, out_len_ptr)?;

    if response_len > guest_buffer_size {
        write_i32_le(memory, caller, out_len_ptr, response_len)?;
        return Ok(2);
    }

    let dest = validate_range_mut(memory, caller, out_ptr, response_len)?;
    dest.copy_from_slice(data);
    write_i32_le(memory, caller, out_len_ptr, response_len)?;
    Ok(0)
}

fn read_i32_le(
    memory: &wasmtime::Memory,
    caller: &Caller<'_, TetState>,
    ptr: i32,
) -> wasmtime::Result<i32> {
    let slice = validate_range(memory, caller, ptr, 4)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Ok(i32::from_le_bytes(buf))
}

fn write_i32_le(
    memory: &wasmtime::Memory,
    caller: &mut Caller<'_, TetState>,
    ptr: i32,
    value: i32,
) -> wasmtime::Result<()> {
    let dest = validate_range_mut(memory, caller, ptr, 4)?;
    dest.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
