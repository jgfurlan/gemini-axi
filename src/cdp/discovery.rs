use reqwest::Client;
use std::time::Duration;
use tracing::debug;

use super::types::TargetInfo;
use crate::error::{AxiError, Result};
use crate::gemini::selectors::BASE_APP_URL;

/// Discovers a target WebSocket URL for interacting with Gemini.
///
/// If `cdp_endpoint` is already a WebSocket URL (`ws://` or `wss://`), it is returned directly.
/// If `cdp_endpoint` is an HTTP endpoint (`http://127.0.0.1:9222`), Chrome is queried for open tabs:
/// - First, looks for an existing tab matching `preferred_url` or `gemini.google.com`.
/// - If none is found, requests Chrome to create a new tab pointing to `BASE_APP_URL`.
/// - If creation via `/json/new` is not supported, reuses the first available `page` target.
pub async fn discover_target(cdp_endpoint: &str, preferred_url: Option<&str>) -> Result<String> {
    let endpoint = cdp_endpoint.trim();
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_string());
    }

    let http_base = normalize_http_url(endpoint);
    debug!(base = %http_base, "Discovering Chrome tabs via HTTP endpoint");

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| AxiError::Http(format!("failed to initialize HTTP client: {e}")))?;

    // Fetch target list
    let list_url = format!("{http_base}/json/list");
    let resp = match client.get(&list_url).send().await {
        Ok(r) => r,
        Err(_) => {
            // Fallback to /json
            let alt_url = format!("{http_base}/json");
            client.get(&alt_url).send().await.map_err(|e| {
                if e.is_connect() {
                    AxiError::ConnectionRefused(http_base.clone())
                } else {
                    AxiError::Http(format!("failed to query Chrome at {http_base}: {e}"))
                }
            })?
        }
    };

    let targets: Vec<TargetInfo> = resp
        .json()
        .await
        .map_err(|e| AxiError::Http(format!("invalid JSON from Chrome target list: {e}")))?;

    // 1. Search for existing Gemini tab or tab matching preferred_url
    let match_url = preferred_url.unwrap_or("gemini.google.com");
    for target in &targets {
        if target.target_type == "page"
            && target.url.contains(match_url)
            && let Some(ref ws) = target.web_socket_debugger_url
        {
            debug!(id = %target.id, url = %target.url, "Found existing matching tab");
            return Ok(ws.clone());
        }
    }

    // 2. Try creating a new tab with Gemini App URL
    let target_url = preferred_url.unwrap_or(BASE_APP_URL);
    let new_url = format!("{http_base}/json/new?{target_url}");
    if let Ok(new_resp) = client.put(&new_url).send().await
        && new_resp.status().is_success()
        && let Ok(new_target) = new_resp.json::<TargetInfo>().await
        && let Some(ws) = new_target.web_socket_debugger_url
    {
        debug!(id = %new_target.id, "Created new Gemini tab via PUT /json/new");
        return Ok(ws);
    }

    // Try GET /json/new as fallback (supported on some Chromium builds)
    if let Ok(new_resp) = client.get(&new_url).send().await
        && new_resp.status().is_success()
        && let Ok(new_target) = new_resp.json::<TargetInfo>().await
        && let Some(ws) = new_target.web_socket_debugger_url
    {
        debug!(id = %new_target.id, "Created new Gemini tab via GET /json/new");
        return Ok(ws);
    }

    // 3. Fallback: reuse first available page target (will be navigated to Gemini)
    for target in &targets {
        if target.target_type == "page"
            && let Some(ref ws) = target.web_socket_debugger_url
        {
            debug!(id = %target.id, url = %target.url, "Reusing existing Chrome tab");
            return Ok(ws.clone());
        }
    }

    Err(AxiError::TabNotFound(match_url.to_string()))
}

/// Normalizes endpoint URL to an `http://` string without trailing slashes.
pub fn normalize_http_url(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("http://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_http_url() {
        assert_eq!(
            normalize_http_url("127.0.0.1:9222"),
            "http://127.0.0.1:9222"
        );
        assert_eq!(
            normalize_http_url("http://localhost:9222/"),
            "http://localhost:9222"
        );
        assert_eq!(
            normalize_http_url("https://remote.chrome.dev:9222"),
            "https://remote.chrome.dev:9222"
        );
    }
}
