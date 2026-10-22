use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tracing::debug;
use url::Url;

use super::types::TargetInfo;
use crate::error::{AxiError, Result};
use crate::gemini::selectors::BASE_APP_URL;

/// Returns known user-data profile directories for Chromium-family browsers on the current OS.
pub fn browser_profile_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var("HOME").ok().map(PathBuf::from);

    #[cfg(target_os = "macos")]
    if let Some(ref h) = home {
        let mac_profiles = [
            "Library/Application Support/Google/Chrome",
            "Library/Application Support/Google/Chrome Canary",
            "Library/Application Support/Comet",
            "Library/Application Support/Arc/User Data",
            "Library/Application Support/Dia/User Data",
            "Library/Application Support/Microsoft Edge",
            "Library/Application Support/Microsoft Edge Beta",
            "Library/Application Support/Microsoft Edge Dev",
            "Library/Application Support/Microsoft Edge Canary",
            "Library/Application Support/BraveSoftware/Brave-Browser",
            "Library/Application Support/BraveSoftware/Brave-Origin",
        ];
        for p in &mac_profiles {
            dirs.push(h.join(p));
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(ref h) = home {
        let linux_profiles = [
            ".config/google-chrome",
            ".config/chromium",
            ".config/chromium-browser",
            ".config/microsoft-edge",
            ".config/microsoft-edge-beta",
            ".config/microsoft-edge-dev",
            ".var/app/org.chromium.Chromium/config/chromium",
            ".var/app/com.google.Chrome/config/google-chrome",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
            ".var/app/com.microsoft.Edge/config/microsoft-edge",
        ];
        for p in &linux_profiles {
            dirs.push(h.join(p));
        }
    }

    #[cfg(target_os = "windows")]
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);
        let win_profiles = [
            "Google/Chrome/User Data",
            "Google/Chrome SxS/User Data",
            "Google/Chrome Beta/User Data",
            "Google/Chrome Dev/User Data",
            "Chromium/User Data",
            "Microsoft/Edge/User Data",
            "Microsoft/Edge Beta/User Data",
            "Microsoft/Edge Dev/User Data",
            "Microsoft/Edge SxS/User Data",
            "BraveSoftware/Brave-Browser/User Data",
        ];
        for p in &win_profiles {
            dirs.push(base.join(p));
        }
    }

    dirs
}

/// Discovers active DevTools port and browser WebSocket path from `DevToolsActivePort` files.
pub fn find_devtools_active_port() -> Option<(u16, String)> {
    for profile in browser_profile_dirs() {
        let path = profile.join("DevToolsActivePort");
        if let Ok(content) = fs::read_to_string(&path) {
            let mut lines = content.lines();
            if let Some(port_line) = lines.next()
                && let Ok(port) = port_line.trim().parse::<u16>()
                && let Some(ws_path) = lines.next()
            {
                let trimmed_path = ws_path.trim().to_string();
                if !trimmed_path.is_empty() {
                    debug!(
                        port = port,
                        path = %trimmed_path,
                        profile = ?profile,
                        "Found active Chrome DevToolsActivePort"
                    );
                    return Some((port, trimmed_path));
                }
            }
        }
    }
    None
}

/// Discovers target page using standard HTTP `/json` endpoints (for mock servers and unconfined instances).
pub async fn discover_target_http(
    cdp_endpoint: &str,
    preferred_url: Option<&str>,
) -> Result<String> {
    let endpoint = cdp_endpoint.trim();
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_string());
    }

    let http_base = normalize_http_url(endpoint);
    debug!(base = %http_base, "Discovering Chrome tabs via HTTP endpoint");

    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| AxiError::Http(format!("failed to initialize HTTP client: {e}")))?;

    // Attempt HTTP target list query
    let list_url = format!("{http_base}/json/list");
    let resp = match client.get(&list_url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
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

    if !resp.status().is_success() {
        return Err(AxiError::Http(format!(
            "Chrome HTTP discovery returned status {}",
            resp.status()
        )));
    }

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

    if let Ok(new_resp) = client.get(&new_url).send().await
        && new_resp.status().is_success()
        && let Ok(new_target) = new_resp.json::<TargetInfo>().await
        && let Some(ws) = new_target.web_socket_debugger_url
    {
        debug!(id = %new_target.id, "Created new Gemini tab via GET /json/new");
        return Ok(ws);
    }

    // 3. Fallback: reuse first available page target
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

/// Discovers a target WebSocket URL for interacting with Gemini.
///
/// If `cdp_endpoint` is already a WebSocket URL (`ws://` or `wss://`), it is returned directly.
/// If `cdp_endpoint` is an HTTP endpoint (`http://127.0.0.1:9222`), Chrome is queried for open tabs:
/// - First, attempts HTTP `/json/list` discovery.
/// - If HTTP returns 404 (Chrome 147+ default profile lockdown) or fails on localhost,
///   automatically falls back to discovering Chrome via `DevToolsActivePort`.
pub async fn discover_target(cdp_endpoint: &str, preferred_url: Option<&str>) -> Result<String> {
    let endpoint = cdp_endpoint.trim();
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_string());
    }

    match discover_target_http(endpoint, preferred_url).await {
        Ok(ws) => Ok(ws),
        Err(e) => {
            let http_base = normalize_http_url(endpoint);
            let endpoint_url = Url::parse(&http_base).ok();
            let requested_port = endpoint_url.as_ref().and_then(|u| u.port());
            let is_localhost = endpoint_url
                .as_ref()
                .map(|u| u.host_str() == Some("127.0.0.1") || u.host_str() == Some("localhost"))
                .unwrap_or(false);

            if is_localhost
                && let Some((active_port, ws_path)) = find_devtools_active_port()
                && (requested_port == Some(active_port)
                    || requested_port == Some(9222)
                    || requested_port.is_none())
            {
                return Ok(format!("ws://127.0.0.1:{active_port}{ws_path}"));
            }

            Err(e)
        }
    }
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

    #[test]
    fn test_browser_profile_dirs_not_empty() {
        let dirs = browser_profile_dirs();
        assert!(!dirs.is_empty());
    }
}
