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
