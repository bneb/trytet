//! JSON Schema Validator cartridge.
//!
//! Validates JSON data against a JSON Schema. Returns structured validation
//! results or error details. Useful for agents that need to verify structured
//! outputs before committing them.

wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use serde_json::Value;

struct JsonSchemaValidator;

impl Guest for JsonSchemaValidator {
    fn execute(input: String) -> Result<String, String> {
        let parsed: Value =
            serde_json::from_str(&input).map_err(|e| format!("Invalid input JSON: {}", e))?;

        let schema_value = parsed["schema"].clone();
        let data_value = parsed["data"].clone();

        let schema = jsonschema::JSONSchema::options()
            .compile(&schema_value)
            .map_err(|e| format!("Invalid JSON Schema: {}", e))?;

        let validation = schema.validate(&data_value);
        match validation {
            Ok(()) => Ok(r#"{"valid":true,"errors":[]}"#.to_string()),
            Err(errors) => {
                let msgs: Vec<String> = errors
                    .map(|e| format!("{}: {}", e.instance_path, e))
                    .collect();
                let list = msgs
                    .iter()
                    .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!("{{\"valid\":false,\"errors\":[{}]}}", list))
            }
        }
    }
}

export!(JsonSchemaValidator);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_validate_valid_data() {
        let input = r#"{"schema":{"type":"object","properties":{"name":{"type":"string"}}},"data":{"name":"alice"}}"#;
        let result = JsonSchemaValidator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["valid"], true);
        assert_eq!(resp["errors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_validate_invalid_data() {
        let input = r#"{"schema":{"type":"object","properties":{"age":{"type":"integer"}}},"data":{"age":"not-a-number"}}"#;
        let result = JsonSchemaValidator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["valid"], false);
        assert!(resp["errors"].as_array().unwrap().len() > 0);
    }

    #[test]
    fn test_validate_invalid_schema() {
        let input = r#"{"schema":123,"data":{}}"#;
        let result = JsonSchemaValidator::execute(input.into());
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_json() {
        let result = JsonSchemaValidator::execute("not-json".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_object() {
        let input = r#"{"schema":{},"data":{}}"#;
        let result = JsonSchemaValidator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["valid"], true);
    }

    #[test]
    fn test_validate_mixed_validity() {
        // Multiple properties, one valid and one invalid
        let input = r#"{"schema":{"type":"object","properties":{"name":{"type":"string"},"count":{"type":"number"},"active":{"type":"boolean"}},"required":["name","count","active"]},"data":{"name":"test","count":42,"active":"not-a-bool"}}"#;
        let result = JsonSchemaValidator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["valid"], false);
        // Error should reference the mismatched field
        let errors = resp["errors"].as_array().unwrap();
        assert!(errors.len() > 0);
    }
}
