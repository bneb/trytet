// lib.rs

wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use boa_engine::{Context, Source};
use crate::exports::trytet::component::cartridge_v1::Guest;

struct JsEvaluator;

impl Guest for JsEvaluator {
    fn execute(input: String) -> Result<String, String> {
        // Initialize a new Boa JavaScript context
        let mut context = Context::default();
        
        // Parse and evaluate the input string
        let source = Source::from_bytes(input.as_bytes());
        match context.eval(source) {
            Ok(value) => {
                // Return the display string of the JS output
                Ok(value.display().to_string())
            }
            Err(e) => {
                // If evaluation fails (syntax error, type error, etc.), return the error
                Err(e.to_string())
            }
        }
    }
}

export!(JsEvaluator);
