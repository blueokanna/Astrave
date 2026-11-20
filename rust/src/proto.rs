//! Runtime-independent control messages, wire framing and authentication.

#![forbid(unsafe_code)]

#[path = "proto_auth.rs"]
pub mod auth;
#[path = "proto_base64.rs"]
pub mod base64;
#[path = "proto_frame.rs"]
pub mod frame;
#[path = "proto_hex.rs"]
pub mod hex;
#[path = "proto_msg.rs"]
pub mod msg;

pub use frame::{Codec, FrameError, FrameHeader, MAX_FRAME_BODY};
pub use msg::Message;
