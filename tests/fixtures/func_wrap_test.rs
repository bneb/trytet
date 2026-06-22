use wasmtime::*;
fn main() {
    let engine = Engine::default();
    let mut linker: Linker<()> = Linker::new(&engine);
    linker.func_wrap_async("m", "f", |caller: Caller<'_, ()>, a: i32, b: i32| Box::new(async move { 1i32 })).unwrap();
}
