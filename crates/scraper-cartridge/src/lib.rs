wit_bindgen::generate!({
    world: "tool-guest",
    path: "../../wit/cartridge.wit",
});

use crate::exports::trytet::component::cartridge_v1::Guest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ScraperRequest {
    html: String,
    selector: String,
    extract_attribute: Option<String>,
}

#[derive(Serialize)]
struct ScraperResponse {
    elements: Vec<String>,
    error: Option<String>,
}

struct ScraperEvaluator;

impl Guest for ScraperEvaluator {
    fn execute(input: String) -> Result<String, String> {
        // Parse the input JSON
        let req: ScraperRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Err(format!("Invalid input JSON: {}", e)),
        };

        // Parse the CSS selector
        let selector = match Selector::parse(&req.selector) {
            Ok(s) => s,
            Err(e) => {
                let resp = ScraperResponse {
                    elements: vec![],
                    error: Some(format!("Invalid CSS selector: {:?}", e)),
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        };

        // Parse the HTML document
        let document = Html::parse_document(&req.html);

        let mut elements = Vec::new();

        // Limit the number of elements to extract to avoid massive memory allocations
        // The host's fuel and memory limits will also catch runaway extraction
        for element in document.select(&selector).take(10_000) {
            if let Some(attr) = &req.extract_attribute {
                if let Some(val) = element.value().attr(attr) {
                    elements.push(val.to_string());
                }
            } else {
                // If no attribute specified, get the inner text
                let text = element.text().collect::<Vec<_>>().join(" ");
                elements.push(text.trim().to_string());
            }
        }

        let resp = ScraperResponse {
            elements,
            error: None,
        };

        match serde_json::to_string(&resp) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Serialization error: {}", e)),
        }
    }
}

export!(ScraperEvaluator);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::trytet::component::cartridge_v1::Guest;

    #[test]
    fn test_scraper_select_by_tag() {
        let input =
            r#"{"html":"<html><body><p>hello</p><p>world</p></body></html>","selector":"p"}"#;
        let result = ScraperEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"].is_null());
        let elements = resp["elements"].as_array().unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0], "hello");
        assert_eq!(elements[1], "world");
    }

    #[test]
    fn test_scraper_extract_attribute() {
        let input =
            r#"{"html":"<a href=\"/link\">click</a>","selector":"a","extract_attribute":"href"}"#;
        let result = ScraperEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"].is_null());
        let elements = resp["elements"].as_array().unwrap();
        assert_eq!(elements[0], "/link");
    }

    #[test]
    fn test_scraper_no_match() {
        let input = r#"{"html":"<div>content</div>","selector":"span"}"#;
        let result = ScraperEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"].is_null());
        assert!(resp["elements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_scraper_invalid_json() {
        let result = ScraperEvaluator::execute("not-json".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_scraper_invalid_selector() {
        let input = r#"{"html":"<p>text</p>","selector":":"}"#;
        let result = ScraperEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok with error, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"].is_string());
        assert!(resp["elements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_scraper_empty_html() {
        let input = r#"{"html":"","selector":"p"}"#;
        let result = ScraperEvaluator::execute(input.into());
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let resp: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(resp["error"].is_null());
        assert!(resp["elements"].as_array().unwrap().is_empty());
    }
}
