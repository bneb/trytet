use wasmtime::*;
use wasmtime_wasi::p1::{WasiP1Ctx, add_to_linker_async};
use wasmtime_wasi::WasiCtxBuilder;

struct State { wasi: WasiP1Ctx }

fn main() {
    let mut config = Config::new();
    config.async_support(true);
    let engine = Engine::new(&config).unwrap();
    let mut linker: Linker<State> = Linker::new(&engine);
    add_to_linker_async(&mut linker, |s: &mut State| &mut s.wasi).unwrap();
}
