//! Prepaid credit left on the OpenAI organization: the top-ups the operator entered minus every
//! cost the Costs API reports since the first of them. The API needs an admin key.

use chrono::Utc;
use metocast_core::transcription::{CreditStatus, OpenAiConfig};
use serde_json::Value;

const COSTS_URL: &str = "https://api.openai.com/v1/organization/costs";

/// `None` when there is no admin key or no top-up to count from.
///
/// # Errors
/// Returns the HTTP or API failure.
pub async fn fetch(config: &OpenAiConfig) -> Result<Option<CreditStatus>, String> {
    let Some(first) = config.top_ups.iter().map(|t| t.date).min() else {
        return Ok(None);
    };
    if config.admin_key.is_empty() {
        return Ok(None);
    }
    let start_time = first
        .and_hms_opt(0, 0, 0)
        .map_or(0, |t| t.and_utc().timestamp())
        .to_string();
    let client = reqwest::Client::new();
    let mut spent = 0.0;
    let mut page: Option<String> = None;
    loop {
        let mut query = vec![
            ("start_time", start_time.clone()),
            ("bucket_width", "1d".to_string()),
            ("limit", "180".to_string()),
        ];
        if let Some(page) = &page {
            query.push(("page", page.clone()));
        }
        let response = client
            .get(COSTS_URL)
            .bearer_auth(&config.admin_key)
            .query(&query)
            .send()
            .await
            .map_err(|e| format!("costs request failed: {}", e.without_url()))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("costs response unreadable: {}", e.without_url()))?;
        if !status.is_success() {
            return Err(format!(
                "costs API {status}: {}",
                body["error"]["message"].as_str().unwrap_or("")
            ));
        }
        spent += page_cost(&body);
        match body["next_page"].as_str() {
            Some(next) if body["has_more"].as_bool() == Some(true) => page = Some(next.to_string()),
            _ => break,
        }
    }
    let topped_up = config.top_ups.iter().map(|t| t.amount_usd).sum();
    Ok(Some(CreditStatus::new(topped_up, spent, Utc::now())))
}

/// USD summed over one page of `/organization/costs` buckets.
fn page_cost(page: &Value) -> f64 {
    page["data"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|bucket| bucket["results"].as_array().into_iter().flatten())
        .filter(|result| {
            result["amount"]["currency"]
                .as_str()
                .is_none_or(|c| c.eq_ignore_ascii_case("usd"))
        })
        .filter_map(|result| result["amount"]["value"].as_f64())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sums_usd_over_buckets_and_pages() {
        let pages = [
            json!({"object": "page", "has_more": true, "next_page": "p2", "data": [
                {"object": "bucket", "results": [
                    {"amount": {"value": 1.25, "currency": "usd"}},
                    {"amount": {"value": 0.034, "currency": "usd"}}
                ]},
                {"object": "bucket", "results": []}
            ]}),
            json!({"object": "page", "has_more": false, "next_page": null, "data": [
                {"object": "bucket", "results": [{"amount": {"value": 2.0, "currency": "usd"}}]}
            ]}),
        ];
        let spent: f64 = pages.iter().map(page_cost).sum();
        assert!((spent - 3.284).abs() < 1e-9);
        let credit = CreditStatus::new(8.0, spent, Utc::now());
        assert!(credit.low);
        assert!(!CreditStatus::new(10.0, spent, Utc::now()).low);
    }
}
