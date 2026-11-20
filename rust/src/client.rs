#[path = "client_connector.rs"]
mod connector;
#[path = "client_control.rs"]
mod control;
#[path = "client_handle.rs"]
mod handle;
#[path = "client_local_control.rs"]
pub mod local_control;
#[path = "client_plugin.rs"]
mod plugin;
#[path = "client_reload.rs"]
mod reload;
#[path = "client_service.rs"]
mod service;
#[path = "client_tunnel.rs"]
mod tunnel;

pub use crate::core::config::{ClientConfig, resolve_client_config_path};
pub use handle::{ClientHandle, ClientStatus, StartOptions};
pub use reload::{ReloadLevel, ReloadOutcome};
pub use service::Service;

pub use crate::core::VERSION;
