wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct RegexRequest {
    pattern: String,
    text: String,
}

#[derive(Serialize)]
struct RegexResponse {
    matched: bool,
    matches: Vec<String>,
    captures: Vec<Vec<Option<String>>>,
    error: Option<String>,
}

struct RegexEvaluator;

impl Guest for RegexEvaluator {
    fn execute(input: String) -> Result<String, String> {
        // Parse the input JSON
        let req: RegexRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Err(format!("Invalid input JSON: {}", e)),
        };

        // Compile the regex
        let re = match Regex::new(&req.pattern) {
            Ok(r) => r,
            Err(e) => {
                let resp = RegexResponse {
                    matched: false,
                    matches: vec![],
                    captures: vec![],
                    error: Some(format!("Regex compilation error: {}", e)),
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        };

        // Execute the regex and collect matches/captures
        let matched = re.is_match(&req.text);

        let mut matches = Vec::new();
        let mut captures_list = Vec::new();

        // Limit the number of matches to avoid excessive fuel/memory usage on completely unconstrained repeating patterns
        // though fuel metering covers us mostly.
        for cap in re.captures_iter(&req.text).take(1000) {
            let mut current_caps = Vec::new();
            for (i, m) in cap.iter().enumerate() {
                if i == 0 {
                    if let Some(mat) = m {
                        matches.push(mat.as_str().to_string());
                    }
                }
                current_caps.push(m.map(|mat| mat.as_str().to_string()));
            }
            captures_list.push(current_caps);
        }

        let resp = RegexResponse {
            matched,
            matches,
            captures: captures_list,
            error: None,
        };

        match serde_json::to_string(&resp) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Serialization error: {}", e)),
        }
    }
}

export!(RegexEvaluator);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_regex_basic_match() {
        let input = r#"{"pattern":"hello","text":"hello world"}"#;
        let result = RegexEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["matched"], true);
        assert_eq!(resp["matches"], serde_json::json!(["hello"]));
        assert!(resp["error"].is_null());
    }

    #[test]
    fn test_regex_no_match() {
        let input = r#"{"pattern":"xyz","text":"hello world"}"#;
        let result = RegexEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["matched"], false);
        assert!(resp["matches"].as_array().unwrap().is_empty());
        assert!(resp["error"].is_null());
    }

    #[test]
    fn test_regex_captures() {
        let input = r#"{"pattern":"(\\w+) (\\w+)","text":"hello world"}"#;
        let result = RegexEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["matched"], true);
        assert_eq!(resp["matches"], serde_json::json!(["hello world"]));
        let caps = resp["captures"].as_array().unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0][1], serde_json::json!("hello"));
        assert_eq!(caps[0][2], serde_json::json!("world"));
    }

    #[test]
    fn test_regex_invalid_json() {
        let result = RegexEvaluator::execute("not-json".into());
        assert!(result.is_err(), "Expected Err for invalid JSON");
    }

    #[test]
    fn test_regex_invalid_pattern() {
        let input = r#"{"pattern":"[invalid","text":"hello"}"#;
        let result = RegexEvaluator::execute(input.into());
        assert!(
            result.is_ok(),
            "Expected Ok with error field, got {:?}",
            result
        );
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["matched"], false);
        assert!(resp["error"]
            .as_str()
            .unwrap()
            .contains("Regex compilation error"));
    }

    #[test]
    fn test_regex_empty_text() {
        let input = r#"{"pattern":".*","text":""}"#;
        let result = RegexEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(resp["matched"], true);
        assert_eq!(resp["matches"], serde_json::json!([""]));
    }
}
