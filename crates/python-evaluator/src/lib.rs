wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use rustpython_vm::{compiler::Mode, AsObject, Interpreter, Settings};

struct PythonEvaluator;

impl Guest for PythonEvaluator {
    fn execute(input: String) -> Result<String, String> {
        let settings = Settings::default();

        Interpreter::with_init(settings, |_| {}).enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            // Try to compile as an expression first (so we get a return value)
            let code_obj = vm
                .compile(&input, Mode::Eval, "<trytet>".to_owned())
                .or_else(|_| vm.compile(&input, Mode::Exec, "<trytet>".to_owned()));

            let result = match code_obj {
                Ok(code) => vm.run_code_obj(code, scope),
                Err(err) => {
                    let syntax_error = vm.new_syntax_error(&err, Some(&input));
                    Err(syntax_error)
                }
            };

            match result {
                Ok(val) => {
                    if val.is(&vm.ctx.none()) {
                        Ok("None".to_string())
                    } else {
                        let repr = val
                            .str(vm)
                            .map(|s| s.as_str().to_string())
                            .unwrap_or_else(|_| "<unprintable>".to_string());
                        Ok(repr)
                    }
                }
                Err(exc) => {
                    // Capture the exception string
                    let mut err_msg = String::new();
                    vm.write_exception(&mut err_msg, &exc).unwrap_or_else(|_| {
                        err_msg = format!("Unknown Python Error: {:?}", exc);
                    });
                    Err(err_msg)
                }
            }
        })
    }
}

export!(PythonEvaluator);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_python_add() {
        let result = PythonEvaluator::execute("1 + 2".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "3");
    }

    #[test]
    fn test_python_string_literal() {
        let result = PythonEvaluator::execute("'hello'".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_python_syntax_error() {
        let result = PythonEvaluator::execute("if True".into());
        assert!(result.is_err(), "Expected Err for syntax error");
    }

    #[test]
    fn test_python_none() {
        let result = PythonEvaluator::execute("None".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "None");
    }

    #[test]
    fn test_python_list_expression() {
        let result = PythonEvaluator::execute("[1, 2, 3]".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "[1, 2, 3]");
    }

    #[test]
    fn test_python_bool() {
        let result = PythonEvaluator::execute("1 < 2".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "True");
    }

    #[test]
    fn test_python_empty_input() {
        let result = PythonEvaluator::execute("".into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(result.unwrap(), "None");
    }
}
