wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use rustpython_vm::{Interpreter, Settings, AsObject, compiler::Mode};

struct PythonEvaluator;

impl Guest for PythonEvaluator {
    fn execute(input: String) -> Result<String, String> {
        let settings = Settings::default();
        
        Interpreter::with_init(settings, |_| {}).enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            // Try to compile as an expression first (so we get a return value)
            let code_obj = vm.compile(&input, Mode::Eval, "<trytet>".to_owned())
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
                        let repr = val.str(vm)
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
