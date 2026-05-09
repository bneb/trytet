use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn memory_query(client: &Client, alias: &str, vector: &str) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Querying semantic memory for {}...", alias.cyan()));

    // Parse the float string (e.g. "[0.1, 0.2]")
    let vec_query: Result<Vec<f32>, _> = serde_json::from_str(vector);
    let vector_data = match vec_query {
        Ok(v) => v,
        Err(_) => {
            pb.finish_with_message(format!(
                "{} Vector must be floating array like [0.1, 0.2]",
                "✘".red()
            ));
            return Ok(());
        }
    };

    let query_payload = tet_core::memory::SearchQuery {
        collection: "default".to_string(), // Default space
        query_vector: vector_data,
        limit: 5,
        min_score: 0.0,
    };

    let res = client
        .post(&format!("{}/v1/tet/memory/{}", get_api_url(), alias))
        .json(&query_payload)
        .send()
        .await?;

    if res.status().is_success() {
        let results: Vec<tet_core::memory::SearchResult> = res.json().await?;
        pb.finish_with_message(format!(
            "{} Retrieved {} Semantic Matches from {}",
            "✔".green(),
            results.len(),
            alias.cyan()
        ));
        for (i, result) in results.iter().enumerate() {
            println!(
                "  [Match {}] ID: {} (Score: {:.4})",
                i,
                result.id.yellow(),
                result.score
            );
        }
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Memory search failed: {}", "✘".red(), err));
    }

    Ok(())
}
