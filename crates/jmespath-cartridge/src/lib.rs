wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct JmesPathRequest {
    json: String,
    expression: String,
}

#[derive(Serialize)]
struct JmesPathResponse {
    result: Option<String>,
    error: Option<String>,
}

struct JmesPathEvaluator;

impl Guest for JmesPathEvaluator {
    fn execute(input: String) -> Result<String, String> {
        // Parse the input JSON
        let req: JmesPathRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Err(format!("Invalid input JSON: {}", e)),
        };

        // Compile the JMESPath expression
        let expr = match jmespath::compile(&req.expression) {
            Ok(e) => e,
            Err(e) => {
                let resp = JmesPathResponse {
                    result: None,
                    error: Some(format!("JMESPath compilation error: {}", e)),
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        };

        // Parse the payload string into jmespath variable
        let data = match jmespath::Variable::from_json(&req.json) {
            Ok(d) => d,
            Err(e) => {
                let resp = JmesPathResponse {
                    result: None,
                    error: Some(format!("Invalid JSON payload data: {}", e)),
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        };

        // Execute expression
        match expr.search(data) {
            Ok(res) => {
                // Return result as JSON string
                let resp = JmesPathResponse {
                    result: Some(res.to_string()),
                    error: None,
                };
                Ok(serde_json::to_string(&resp).unwrap_or_default())
            }
            Err(e) => {
                let resp = JmesPathResponse {
                    result: None,
                    error: Some(format!("JMESPath execution error: {}", e)),
                };
                Ok(serde_json::to_string(&resp).unwrap_or_default())
            }
        }
    }
}

export!(JmesPathEvaluator);
