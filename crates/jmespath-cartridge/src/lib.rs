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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_jmespath_simple_key() {
        let input = r#"{"json":"{\"name\":\"alice\"}","expression":"name"}"#;
        let result = JmesPathEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["result"], "\"alice\"");
        assert!(resp["error"].is_null());
    }

    #[test]
    fn test_jmespath_invalid_json_input() {
        let result = JmesPathEvaluator::execute("not-json".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_jmespath_invalid_expression() {
        let input = r#"{"json":"{}","expression":"[[]] invalid"}"#;
        let result = JmesPathEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok with error, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"]
            .as_str()
            .unwrap()
            .contains("JMESPath compilation error"));
    }

    #[test]
    fn test_jmespath_null_result() {
        let input = r#"{"json":"{\"a\":1}","expression":"b"}"#;
        let result = JmesPathEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["result"], "null");
    }

    #[test]
    fn test_jmespath_nested_field() {
        let input = r#"{"json":"{\"a\":{\"b\":42}}","expression":"a.b"}"#;
        let result = JmesPathEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["result"], "42");
    }

    #[test]
    fn test_jmespath_empty_expression() {
        // Empty expression — should return an error, not panic
        let input = r#"{"json":"{\"x\":1}","expression":""}"#;
        let result = JmesPathEvaluator::execute(input.into());
        assert!(
            result.is_ok(),
            "Execute should not fail on empty expression, got {:?}",
            result
        );
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        // Either result or error field must be present and non-null
        let has_result = resp.get("result").map_or(false, |v| !v.is_null());
        let has_error = resp.get("error").map_or(false, |v| !v.is_null());
        assert!(
            has_result || has_error,
            "Expected result or error to be non-null, got {:?}",
            resp
        );
    }
}
