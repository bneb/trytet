wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use varisat::{ExtendFormula, Solver};
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
                        m.into_iter()
                            .map(|l| {
                                let val = l.index() + 1;
                                if l.is_positive() {
                                    val as isize
                                } else {
                                    -(val as isize)
                                }
                            })
                            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_sat_satisfiable() {
        // Formula: (x1) -- a single clause with one positive literal
        let input = r#"{"dimacs":"c simple\np cnf 1 1\n1 0"}"#;
        let result = SatEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["satisfiable"], true);
        assert!(resp["error"].is_null());
        let model = resp["model"].as_array();
        assert!(model.is_some(), "SAT should return a model");
    }

    #[test]
    fn test_sat_unsatisfiable() {
        // Formula: (x1) AND (not x1) -- contradictory
        let input = r#"{"dimacs":"c unsatisfiable\np cnf 1 2\n1 0\n-1 0"}"#;
        let result = SatEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["satisfiable"], false);
        assert!(resp["model"].is_null());
        assert!(resp["error"].is_null());
    }

    #[test]
    fn test_sat_invalid_json() {
        let result = SatEvaluator::execute("not-json".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_sat_malformed_dimacs() {
        let input = r#"{"dimacs":"garbage not dimacs"}"#;
        let result = SatEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok with error, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["satisfiable"], false);
        assert!(resp["error"]
            .as_str()
            .unwrap()
            .contains("DIMACS parse error"));
    }

    #[test]
    fn test_sat_empty_dimacs() {
        // Empty DIMACS is trivially satisfiable (no constraints)
        let input = r#"{"dimacs":""}"#;
        let result = SatEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["satisfiable"], true);
    }
}
