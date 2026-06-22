use crate::tet_cli::utils::{get_api_url, pb_style};
use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;

pub async fn pay_cmd(
    client: &Client,
    from: &str,
    to: &str,
    amount: u64,
    json_out: bool,
) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "💰 Transferring {} fuel: {} → {}",
        amount.to_string().yellow(),
        from.cyan(),
        to.cyan()
    ));

    let payload = serde_json::json!({
        "source_alias": from,
        "target_alias": to,
        "amount": amount,
    });

    let res = client
        .post(&format!("{}/v1/tet/topup", get_api_url()))
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&body)?);
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Complete: {} fuel ({} → {})",
                    "✔".green(),
                    amount.to_string().yellow(),
                    from.cyan(),
                    to.cyan()
                ));
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if json_out {
                pb.finish_and_clear();
                println!(
                    "{}",
                    serde_json::json!({"error": body, "status": status.as_u16()})
                );
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Failed (HTTP {}): {}",
                    "✘".red(),
                    status,
                    body
                ));
            }
        }
        Err(e) => {
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                pb.finish_with_message(format!("{} Network Error: {}", "✘".red(), e));
            }
        }
    }

    Ok(())
}
