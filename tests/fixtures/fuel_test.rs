use wasmtime::{Store, Engine};
fn main() {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());
    store.set_fuel(100).unwrap();
    let fuel = store.get_fuel().unwrap();
}
