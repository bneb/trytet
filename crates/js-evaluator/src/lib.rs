// lib.rs

wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use boa_engine::{Context, Source};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_execute_simple_expression() {
        let result = JsEvaluator::execute("2 + 2".into());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "4");
    }

    #[test]
    fn test_execute_string_literal() {
        let result = JsEvaluator::execute("'hello'".into());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[test]
    fn test_execute_object_literal() {
        let result = JsEvaluator::execute("({a: 1})".into());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("a"));
    }

    #[test]
    fn test_execute_invalid_syntax() {
        let result = JsEvaluator::execute("{{{".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_empty_input() {
        let result = JsEvaluator::execute("".into());
        assert!(result.is_ok());
    }
}
