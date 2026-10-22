use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, trace, warn};

use super::discovery::{discover_target_http, normalize_http_url};
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
    target_id: Option<String>,
    is_created_target: bool,
    browser_client: Option<Arc<CdpClient>>,
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
            target_id: None,
            is_created_target: false,
            browser_client: None,
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
            target_id: self.target_id.clone(),
            is_created_target: self.is_created_target,
            browser_client: self.browser_client.clone(),
        }
    }

    /// Configures the target ID, whether it was created dynamically, and the root browser client.
    pub fn with_target(
        mut self,
        target_id: impl Into<String>,
        is_created: bool,
        browser: Option<Arc<CdpClient>>,
    ) -> Self {
        self.target_id = Some(target_id.into());
        self.is_created_target = is_created;
        self.browser_client = browser;
        self
    }

    /// Returns the active session ID, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Returns the target ID, if known.
    pub fn target_id(&self) -> Option<&str> {
        self.target_id.as_deref()
    }

    /// Connects to a Chrome browser-level WebSocket and attaches to a matching page target
    /// using flat session mode (`Target.attachToTarget`).
    pub async fn connect_browser_target(
        browser_ws: &str,
        preferred_url: Option<&str>,
    ) -> Result<Self> {
        debug!(browser_ws = %browser_ws, "Connecting to browser WebSocket");
        let client = Self::connect(browser_ws).await?;
        let browser_arc = Arc::new(client.clone());

        // By default, create a fresh tab for clean conversations rather than reusing
        // an existing tab that might contain prior conversations and stale messages.
        let (tid, is_created) = if let Some(purl) = preferred_url {
            let val = client
                .call("Target.getTargets", serde_json::json!({}))
                .await?;
            let targets = val
                .get("targetInfos")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut matched_id = None;
            for t in &targets {
                let t_type = t.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                let t_url = t.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                if t_type == "page"
                    && t_url.contains(purl)
                    && let Some(id) = t.get("targetId").and_then(|v| v.as_str())
                {
                    debug!(id = %id, url = %t_url, "Found matching target for preferred URL");
                    matched_id = Some(id.to_string());
                    break;
                }
            }

            if let Some(id) = matched_id {
                (id, false)
            } else {
                debug!(url = %purl, "Creating new tab for preferred URL via Target.createTarget");
                let create_res = client
                    .call("Target.createTarget", serde_json::json!({ "url": purl }))
                    .await?;
                let id = create_res
                    .get("targetId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AxiError::Other("missing targetId from Target.createTarget".into())
                    })?;
                (id.to_string(), true)
            }
        } else {
            let target_url = BASE_APP_URL;
            debug!(url = %target_url, "Creating fresh tab via Target.createTarget for new conversation");
            let create_res = client
                .call(
                    "Target.createTarget",
                    serde_json::json!({ "url": target_url }),
                )
                .await?;
            let id = create_res
                .get("targetId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AxiError::Other("missing targetId from Target.createTarget".into())
                })?;
            (id.to_string(), true)
        };

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
            is_created = is_created,
            "Attached to target session via browser WebSocket"
        );
        Ok(client
            .with_session(session_id)
            .with_target(tid, is_created, Some(browser_arc)))
    }

    /// Discovers target page and connects over WebSocket CDP.
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

        let http_base = normalize_http_url(endpoint);
        let version_url = format!("{http_base}/json/version");
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| AxiError::Http(e.to_string()))?;

        if let Ok(resp) = http_client.get(&version_url).send().await
            && resp.status().is_success()
            && let Ok(val) = resp.json::<serde_json::Value>().await
            && let Some(browser_ws) = val.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
        {
            debug!(browser_ws = %browser_ws, "Connecting via browser-level WebSocket");
            return Self::connect_browser_target(browser_ws, preferred_url).await;
        }

        let page_ws = discover_target_http(endpoint, preferred_url).await?;
        debug!(ws_url = %page_ws, "Connected to Chrome target via HTTP discovery");
        Self::connect(&page_ws).await
    }

    /// Brings the target page to the front and enables focus emulation to prevent
    /// Chrome background throttling (e.g. frozen requestAnimationFrame / timers).
    pub async fn ensure_active_and_focused(&self) -> Result<()> {
        let _ = self.call("Page.bringToFront", serde_json::json!({})).await;
        let _ = self
            .call(
                "Emulation.setFocusEmulationEnabled",
                serde_json::json!({ "enabled": true }),
            )
            .await;
        Ok(())
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

    /// Evaluates a JavaScript expression and returns the result as a u64.
    pub async fn evaluate_u64(&self, expression: &str) -> Result<u64> {
        match self.evaluate(expression).await? {
            Some(serde_json::Value::Number(n)) => Ok(n.as_u64().unwrap_or(0)),
            _ => Ok(0),
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

    /// If this client was attached to a dynamically created target tab, closes that target tab.
    pub async fn close_target(&self) -> Result<()> {
        if self.is_created_target
            && let (Some(tid), Some(browser)) = (&self.target_id, &self.browser_client)
        {
            debug!(target_id = %tid, "Closing dynamically created target tab");
            let _ = browser
                .call("Target.closeTarget", serde_json::json!({ "targetId": tid }))
                .await;
        }
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
