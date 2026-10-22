use std::path::PathBuf;

/// Application errors for gemini-axi.
#[derive(thiserror::Error, Debug)]
pub enum AxiError {
    #[error("connection refused connecting to CDP at {0}")]
    ConnectionRefused(String),

    #[error("no Chrome tab found matching '{0}'")]
    TabNotFound(String),

    #[error("CDP error ({code}): {message}")]
    CdpProtocol { code: i64, message: String },

    #[error("timeout after {0}s waiting for {1}")]
    Timeout(u64, String),

    #[error("generation did not start within {0}s")]
    GenerationDidNotStart(u64),

    #[error("Deep Research only produced a plan and did not generate a final report")]
    ResearchPlanNotCompleted,

    #[error("prompt was not submitted to conversation DOM: {0}")]
    PromptNotSubmitted(String),

    #[error("element not found: {0}")]
    ElementNotFound(String),

    #[error("failed to read file '{0}': {1}")]
    FileRead(PathBuf, std::io::Error),

    #[error("failed to write output file '{0}': {1}")]
    FileWrite(PathBuf, std::io::Error),

    #[error("prompt is empty")]
    EmptyPrompt,

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AxiError>;
