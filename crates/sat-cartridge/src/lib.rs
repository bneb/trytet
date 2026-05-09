wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use varisat::{Solver, ExtendFormula};
use varisat_dimacs::DimacsParser;

#[derive(Deserialize)]
struct SatRequest {
    dimacs: String,
}

#[derive(Serialize)]
struct SatResponse {
    satisfiable: bool,
    model: Option<Vec<isize>>,
    error: Option<String>,
}

struct SatEvaluator;

impl Guest for SatEvaluator {
    fn execute(input: String) -> Result<String, String> {
        let req: SatRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Err(format!("Invalid input JSON: {}", e)),
        };

        let mut solver = Solver::new();
        
        let cursor = Cursor::new(req.dimacs.as_bytes());
        let parser = DimacsParser::parse(cursor);

        match parser {
            Ok(formula) => {
                solver.add_formula(&formula);
            }
            Err(e) => {
                let resp = SatResponse {
                    satisfiable: false,
                    model: None,
                    error: Some(format!("DIMACS parse error: {}", e)),
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        }

        // Trytet's fuel limits will trap this if it triggers exponential combinatorial explosion
        match solver.solve() {
            Ok(is_sat) => {
                let model = if is_sat {
                    solver.model().map(|m| {
                        m.into_iter().map(|l| {
                            let val = l.index() + 1;
                            if l.is_positive() { val as isize } else { -(val as isize) }
                        }).collect()
                    })
                } else {
                    None
                };

                let resp = SatResponse {
                    satisfiable: is_sat,
                    model,
                    error: None,
                };
                Ok(serde_json::to_string(&resp).unwrap_or_default())
            }
            Err(e) => {
                let resp = SatResponse {
                    satisfiable: false,
                    model: None,
                    error: Some(format!("Solve error: {}", e)),
                };
                Ok(serde_json::to_string(&resp).unwrap_or_default())
            }
        }
    }
}

export!(SatEvaluator);
