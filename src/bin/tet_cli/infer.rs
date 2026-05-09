use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn infer_cmd(
    client: &Client,
    alias: &str,
    prompt: &str,
    model: &str,
    temperature: f32,
    max_tokens: u32,
) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Invoking Sovereign Inference on {}...",
        alias.cyan()
    ));

    let request = tet_core::inference::InferenceRequest {
        model_alias: model.to_string(),
        prompt: prompt.to_string(),
        temperature,
        max_tokens,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let res = client
        .post(&format!("{}/v1/tet/infer/{}", get_api_url(), alias))
        .json(&request)
        .send()
        .await?;

    if res.status().is_success() {
        let response: tet_core::inference::InferenceResponse = res.json().await?;
        pb.finish_with_message(format!(
            "{} Inference Complete ({})",
            "✔".green(),
            response.model_alias.cyan()
        ));
        println!("\n{}", response.text);
        println!(
            "\n  {} Prompt Tokens: {}, Generated: {}, Fuel Burned: {}",
            "⚡".yellow(),
            response.prompt_tokens,
            response.tokens_generated,
            response.fuel_burned
        );
        println!("  {} Session: {}", "🧠".to_string(), response.session_id);
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Inference failed: {}", "✘".red(), err));
    }

    Ok(())
}
