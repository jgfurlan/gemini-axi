use serde::{Deserialize, Serialize};

/// Outgoing CDP JSON-RPC request.
#[derive(Serialize, Debug)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Incoming CDP JSON-RPC response or event.
#[derive(Deserialize, Debug, Clone)]
pub struct CdpResponse {
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<CdpErrorPayload>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
}

/// CDP protocol error payload.
#[derive(Deserialize, Debug, Clone)]
pub struct CdpErrorPayload {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Result returned from `Runtime.evaluate`.
#[derive(Deserialize, Debug, Clone)]
pub struct EvaluateResult {
    pub result: RemoteObject,
    #[serde(rename = "exceptionDetails")]
    pub exception_details: Option<ExceptionDetails>,
}

/// Remote object returned from CDP runtime evaluations.
#[derive(Deserialize, Debug, Clone)]
pub struct RemoteObject {
    #[serde(rename = "type")]
    pub object_type: String,
    pub value: Option<serde_json::Value>,
    pub description: Option<String>,
}

/// Exception details if evaluation threw an unhandled error.
#[derive(Deserialize, Debug, Clone)]
pub struct ExceptionDetails {
    pub text: String,
    pub exception: Option<RemoteObject>,
}

/// Browser or Page Target information returned by Chrome's HTTP endpoint.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TargetInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: Option<String>,
}

/// Chrome version info from `/json/version`.
#[derive(Deserialize, Debug, Clone)]
pub struct VersionInfo {
    #[serde(rename = "Browser")]
    pub browser: Option<String>,
    #[serde(rename = "Protocol-Version")]
    pub protocol_version: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: Option<String>,
}
