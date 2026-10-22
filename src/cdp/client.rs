use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, trace, warn};

use super::types::{CdpRequest, CdpResponse, EvaluateResult};
use crate::error::{AxiError, Result};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value>>>>>;

/// CDP Client communicating over a WebSocket connection to Chrome.
#[derive(Clone)]
pub struct CdpClient {
    next_id: Arc<AtomicU64>,
    write_tx: mpsc::Sender<Message>,
    pending: PendingMap,
}

impl CdpClient {
    /// Connects to a Chrome WebSocket debugger URL.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        debug!(url = %ws_url, "Connecting to Chrome DevTools WebSocket");

        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| AxiError::WebSocket(format!("failed to connect to {ws_url}: {e}")))?;

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
        })
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
