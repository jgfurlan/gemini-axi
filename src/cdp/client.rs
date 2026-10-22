use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, trace, warn};

use super::discovery::{discover_target_http, find_devtools_active_port, normalize_http_url};
use super::types::{CdpRequest, CdpResponse, EvaluateResult};
use crate::error::{AxiError, Result};
use crate::gemini::selectors::BASE_APP_URL;

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value>>>>>;

/// CDP Client communicating over a WebSocket connection to Chrome.
#[derive(Clone)]
pub struct CdpClient {
    next_id: Arc<AtomicU64>,
    write_tx: mpsc::Sender<Message>,
    pending: PendingMap,
    session_id: Option<String>,
}

impl CdpClient {
    /// Connects to a Chrome WebSocket debugger URL.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        debug!(url = %ws_url, "Connecting to Chrome DevTools WebSocket");

        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("403") {
                    AxiError::WebSocket(format!(
                        "permission blocked: Chrome is reachable, but the 'Allow remote debugging' popup has not been approved in Chrome. Click 'Allow' in Chrome (or run 'browser-harness mac-approve') and retry ({e})"
                    ))
                } else {
                    AxiError::WebSocket(format!("failed to connect to {ws_url}: {e}"))
                }
            })?;

        let (mut write_half, mut read_half) = ws_stream.split();
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (write_tx, mut write_rx) = mpsc::channel::<Message>(64);

        // Writer task
        let pending_for_writer = pending.clone();
        tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                if let Err(e) = write_half.send(msg).await {
                    warn!(error = %e, "WebSocket send error");
                    break;
                }
            }
            let mut map = pending_for_writer.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(AxiError::WebSocket("connection closed".into())));
            }
        });

        // Reader task
        let pending_for_reader = pending.clone();
        tokio::spawn(async move {
            while let Some(msg_res) = read_half.next().await {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        trace!(payload = %text, "Received CDP message");
                        match serde_json::from_str::<CdpResponse>(&text) {
                            Ok(resp) => {
                                if let Some(id) = resp.id {
                                    let mut map = pending_for_reader.lock().await;
                                    if let Some(tx) = map.remove(&id) {
                                        if let Some(err) = resp.error {
                                            let _ = tx.send(Err(AxiError::CdpProtocol {
                                                code: err.code,
                                                message: err.message,
                                            }));
                                        } else {
                                            let res_val =
                                                resp.result.unwrap_or(serde_json::Value::Null);
                                            let _ = tx.send(Ok(res_val));
                                        }
                                    }
                                } else if let Some(method) = resp.method {
                                    trace!(method = %method, "Received CDP event");
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, raw = %text, "Failed to parse CDP response JSON");
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("CDP WebSocket closed by remote");
                        break;
                    }
                    Err(e) => {
                        error!(error = %e, "CDP WebSocket read error");
                        break;
                    }
                    _ => {}
                }
            }
            let mut map = pending_for_reader.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(AxiError::WebSocket(
                    "connection closed by Chrome".into(),
                )));
            }
        });

        Ok(Self {
            next_id: Arc::new(AtomicU64::new(1)),
            write_tx,
            pending,
            session_id: None,
        })
    }

    /// Returns a new CdpClient sharing the same underlying WebSocket connection,
    /// but scoped to a specific session ID (flat CDP mode).
    pub fn with_session(&self, session_id: impl Into<String>) -> Self {
        Self {
            next_id: self.next_id.clone(),
            write_tx: self.write_tx.clone(),
            pending: self.pending.clone(),
            session_id: Some(session_id.into()),
        }
    }

    /// Returns the active session ID, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Connects to a Chrome browser-level WebSocket and attaches to a matching page target
    /// using flat session mode (`Target.attachToTarget`).
    pub async fn connect_browser_target(
        browser_ws: &str,
        preferred_url: Option<&str>,
    ) -> Result<Self> {
        debug!(browser_ws = %browser_ws, "Connecting to browser WebSocket");
        let client = Self::connect(browser_ws).await?;
        let match_url = preferred_url.unwrap_or("gemini.google.com");

        let val = client
            .call("Target.getTargets", serde_json::json!({}))
            .await?;
        let targets = val
            .get("targetInfos")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut target_id = None;
        for t in &targets {
            let t_type = t.get("type").and_then(|v| v.as_str()).unwrap_or_default();
            let t_url = t.get("url").and_then(|v| v.as_str()).unwrap_or_default();
            if t_type == "page"
                && t_url.contains(match_url)
                && let Some(id) = t.get("targetId").and_then(|v| v.as_str())
            {
                debug!(id = %id, url = %t_url, "Found matching target via Target.getTargets");
                target_id = Some(id.to_string());
                break;
            }
        }

        if target_id.is_none() {
            let target_url = preferred_url.unwrap_or(BASE_APP_URL);
            debug!(url = %target_url, "Creating new tab via Target.createTarget");
            let create_res = client
                .call(
                    "Target.createTarget",
                    serde_json::json!({ "url": target_url }),
                )
                .await?;
            if let Some(id) = create_res.get("targetId").and_then(|v| v.as_str()) {
                target_id = Some(id.to_string());
            }
        }

        if target_id.is_none() {
            for t in &targets {
                let t_type = t.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                if t_type == "page"
                    && let Some(id) = t.get("targetId").and_then(|v| v.as_str())
                {
                    debug!(id = %id, "Reusing first available page target");
                    target_id = Some(id.to_string());
                    break;
                }
            }
        }

        let tid = target_id.ok_or_else(|| AxiError::TabNotFound(match_url.to_string()))?;
        let attach_res = client
            .call(
                "Target.attachToTarget",
                serde_json::json!({ "targetId": tid, "flatten": true }),
            )
            .await?;

        let session_id = attach_res
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AxiError::Other("missing sessionId from Target.attachToTarget".into())
            })?;

        debug!(
            target_id = %tid,
            session_id = %session_id,
            "Attached to target session via browser WebSocket"
        );
        Ok(client.with_session(session_id))
    }

    /// Discovers target page and connects, seamlessly handling both HTTP targets
    /// and browser-level `DevToolsActivePort` endpoints.
    pub async fn connect_to_target(
        cdp_endpoint: &str,
        preferred_url: Option<&str>,
    ) -> Result<Self> {
        let endpoint = cdp_endpoint.trim();
        if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            if endpoint.contains("/devtools/browser/") {
                return Self::connect_browser_target(endpoint, preferred_url).await;
            } else {
                return Self::connect(endpoint).await;
            }
        }

        // Try HTTP target discovery first (e.g. mock server or non-restricted profile)
        match discover_target_http(endpoint, preferred_url).await {
            Ok(page_ws) => {
                debug!(ws_url = %page_ws, "Connected to Chrome target via HTTP discovery");
                Self::connect(&page_ws).await
            }
            Err(e) => {
                // If HTTP failed or returned 404, check DevToolsActivePort for daily Chrome
                let http_base = normalize_http_url(endpoint);
                let endpoint_url = url::Url::parse(&http_base).ok();
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
                    let browser_ws = format!("ws://127.0.0.1:{active_port}{ws_path}");
                    debug!(
                        active_port = active_port,
                        "Connecting via DevToolsActivePort browser WebSocket"
                    );
                    return Self::connect_browser_target(&browser_ws, preferred_url).await;
                }

                Err(e)
            }
        }
    }

    /// Invokes a CDP method with parameters and a default 30-second timeout.
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.call_with_timeout(method, params, Duration::from_secs(30))
            .await
    }

    /// Invokes a CDP method with parameters and an explicit timeout.
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = CdpRequest {
            id,
            method: method.to_string(),
            params,
            session_id: self.session_id.clone(),
        };

        let req_json = serde_json::to_string(&req)?;
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        self.write_tx
            .send(Message::Text(req_json.into()))
            .await
            .map_err(|e| AxiError::WebSocket(format!("failed to enqueue request: {e}")))?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(AxiError::WebSocket("channel dropped unexpectedly".into())),
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(AxiError::Timeout(
                    timeout.as_secs(),
                    format!("CDP call '{method}'"),
                ))
            }
        }
    }

    /// Evaluates a JavaScript expression in the page and returns the parsed JSON value.
    pub async fn evaluate(&self, expression: &str) -> Result<Option<serde_json::Value>> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        });

        let raw_result = self.call("Runtime.evaluate", params).await?;
        let eval_result: EvaluateResult = serde_json::from_value(raw_result)?;

        if let Some(exc) = eval_result.exception_details {
            let desc = exc
                .exception
                .and_then(|e| e.description)
                .unwrap_or(exc.text);
            return Err(AxiError::Other(format!("JavaScript exception: {desc}")));
        }

        Ok(eval_result.result.value)
    }

    /// Evaluates a JavaScript expression and checks if the result is boolean `true`.
    pub async fn evaluate_bool(&self, expression: &str) -> Result<bool> {
        match self.evaluate(expression).await? {
            Some(serde_json::Value::Bool(b)) => Ok(b),
            _ => Ok(false),
        }
    }

    /// Evaluates a JavaScript expression and returns the result as a String.
    pub async fn evaluate_string(&self, expression: &str) -> Result<Option<String>> {
        match self.evaluate(expression).await? {
            Some(serde_json::Value::String(s)) => Ok(Some(s)),
            Some(other) => Ok(Some(other.to_string())),
            None => Ok(None),
        }
    }

    /// Navigates the current target page to a URL.
    pub async fn navigate(&self, url: &str) -> Result<()> {
        let params = serde_json::json!({ "url": url });
        self.call("Page.navigate", params).await?;
        Ok(())
    }

    /// Enters text using CDP Input.insertText.
    pub async fn insert_text(&self, text: &str) -> Result<()> {
        let params = serde_json::json!({ "text": text });
        self.call("Input.insertText", params).await?;
        Ok(())
    }

    /// Enables standard domains (Page, Runtime, DOM).
    pub async fn enable_standard_domains(&self) -> Result<()> {
        self.call("Page.enable", serde_json::json!({})).await?;
        self.call("Runtime.enable", serde_json::json!({})).await?;
        self.call("DOM.enable", serde_json::json!({})).await?;
        Ok(())
    }

    /// Waits for a condition expression to evaluate to `true` within the given timeout.
    pub async fn wait_for_condition(
        &self,
        description: &str,
        expression: &str,
        interval: Duration,
        timeout: Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.evaluate_bool(expression).await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => {
                    trace!(error = %e, "Transient evaluation error in wait_for_condition");
                }
            }
            tokio::time::sleep(interval).await;
        }

        Err(AxiError::Timeout(
            timeout.as_secs(),
            description.to_string(),
        ))
    }
}
