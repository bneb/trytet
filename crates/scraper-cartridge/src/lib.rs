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
