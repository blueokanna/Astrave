extern crate alloc;

#[cfg(feature = "runtime")]
pub mod client;
#[cfg(feature = "runtime")]
pub mod core;
#[cfg(feature = "http")]
pub mod http;
pub mod proto;

#[cfg(all(target_os = "android", feature = "app"))]
mod android;
#[cfg(feature = "app")]
pub mod api;
#[cfg(feature = "app")]
mod cert;
#[cfg(feature = "app")]
mod client_log_layer;
#[cfg(feature = "app")]
mod frb_generated;
#[cfg(feature = "app")]
mod process_stats;
