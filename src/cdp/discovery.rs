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

    Err(AxiError::TabNotFound(match_url.to_string()))
}

/// Discovers a target WebSocket URL for interacting with Gemini.
pub async fn discover_target(cdp_endpoint: &str, preferred_url: Option<&str>) -> Result<String> {
    let endpoint = cdp_endpoint.trim();
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_string());
    }

    discover_target_http(endpoint, preferred_url).await
}

/// Ensures the CDP endpoint is reachable. If the default endpoint (127.0.0.1:9333) is not
/// responding and was not explicitly overridden by --cdp or GEMINI_AXI_CDP, auto-launches
/// Chrome with the dedicated agents profile and flags, waiting up to 10 seconds for it to start.
pub async fn ensure_cdp_reachable(endpoint: &str, is_explicit: bool) -> Result<()> {
    let http_base = normalize_http_url(endpoint);
    let check_url = format!("{http_base}/json/version");
    let client = Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .map_err(|e| AxiError::Http(e.to_string()))?;

    // Check if endpoint is already alive
    if let Ok(resp) = client.get(&check_url).send().await
        && resp.status().is_success()
    {
        return Ok(());
    }

    // Never auto-launch if an explicit CDP endpoint was configured
    if is_explicit {
        return Err(AxiError::ConnectionRefused(http_base));
    }

    // Only auto-launch for loopback port 9333
    let url_parsed = Url::parse(&http_base).ok();
    let port = url_parsed.as_ref().and_then(|u| u.port());
    let host = url_parsed.as_ref().and_then(|u| u.host_str());
    let is_localhost_9333 =
        (host == Some("127.0.0.1") || host == Some("localhost")) && port == Some(9333);

    if !is_localhost_9333 {
        return Err(AxiError::ConnectionRefused(http_base));
    }

    debug!("Agents Chrome on port 9333 not responding; attempting auto-launch");

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|_| AxiError::Other("HOME env not set".into()))?;
        let user_data_dir = format!("{home}/.agents-chrome");
        tokio::process::Command::new("open")
            .args([
                "-na",
                "Google Chrome",
                "--args",
                &format!("--user-data-dir={user_data_dir}"),
                "--remote-debugging-port=9333",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-renderer-backgrounding",
                "--disable-backgrounding-occluded-windows",
                "--disable-background-timer-throttling",
            ])
            .spawn()
            .map_err(|e| AxiError::Other(format!("failed to spawn Chrome: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|_| AxiError::Other("HOME env not set".into()))?;
        let user_data_dir = format!("{home}/.agents-chrome");
        tokio::process::Command::new("google-chrome")
            .args([
                &format!("--user-data-dir={user_data_dir}"),
                "--remote-debugging-port=9333",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-renderer-backgrounding",
                "--disable-backgrounding-occluded-windows",
                "--disable-background-timer-throttling",
            ])
            .spawn()
            .map_err(|e| AxiError::Other(format!("failed to spawn Chrome: {e}")))?;
    }

    // Wait up to 10 seconds for Chrome to be ready
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Ok(resp) = client.get(&check_url).send().await
            && resp.status().is_success()
        {
            debug!("Agents Chrome started and responsive on port 9333");
            return Ok(());
        }
    }

    Err(AxiError::Timeout(
        10,
        "waiting for Chrome on port 9333".into(),
    ))
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
