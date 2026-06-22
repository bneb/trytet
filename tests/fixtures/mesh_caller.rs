#[link(wasm_import_module = "trytet")]
extern "C" {
    fn invoke(
        target_ptr: *const u8, target_len: i32,
        payload_ptr: *const u8, payload_len: i32,
        out_ptr: *mut u8, out_len_ptr: *mut i32,
        fuel: i64
    ) -> i32;
}

fn main() {
    let target = "service-alpha";
    let payload = "SecretData";
    
    let mut out_buf = [0u8; 1024];
    let mut out_len = 1024_i32;

    unsafe {
        let status = invoke(
            target.as_ptr(), target.len() as i32,
            payload.as_ptr(), payload.len() as i32,
            out_buf.as_mut_ptr(), &mut out_len as *mut i32,
            5_000_000 // Fuel transferred
        );

        if status == 0 {
            let response = std::str::from_utf8(&out_buf[..out_len as usize]).unwrap();
            println!("CALLER_RECEIVED: {response}");
        } else {
            println!("CALLER_FAILED: {status}");
        }
    }
}
