pub mod client;
pub mod discovery;
pub mod types;

pub use client::CdpClient;
pub use discovery::{discover_target, ensure_cdp_reachable};
pub use types::{CdpRequest, CdpResponse, TargetInfo};
