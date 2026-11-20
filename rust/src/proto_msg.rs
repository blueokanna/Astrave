//! Control-plane message contracts.
//!
//! JSON field names are part of the published wire contract.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::net::{IpAddr, SocketAddr};
use core::str::FromStr;

use super::frame::{Codec, FrameError, FrameHeader, HEADER_LEN, MAX_FRAME_BODY};
use nextjson::{NsonDeserialize, NsonSerialize};

pub const TYPE_LOGIN: u8 = b'A';
pub const TYPE_LOGIN_RESP: u8 = b'a';
pub const TYPE_NEW_TUNNEL: u8 = b'T';
pub const TYPE_NEW_TUNNEL_RESP: u8 = b't';
pub const TYPE_CLOSE_TUNNEL: u8 = b'X';
pub const TYPE_NEW_DATA_CONN: u8 = b'W';
pub const TYPE_REQ_DATA_CONN: u8 = b'Q';
pub const TYPE_START_DATA_CONN: u8 = b'S';
pub const TYPE_PING: u8 = b'G';
pub const TYPE_PONG: u8 = b'g';
pub const TYPE_UDP_PACKET: u8 = b'D';
pub const TYPE_KICK_OUT: u8 = b'E';

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct Login {
    #[njson(default)]
    pub version: String,
    #[njson(default)]
    pub hostname: String,
    #[njson(default)]
    pub os: String,
    #[njson(default)]
    pub arch: String,
    #[njson(default)]
    pub user: String,
    #[njson(default)]
    pub agent_id: String,
    #[njson(default)]
    pub auth_digest: String,
    #[njson(default)]
    pub timestamp: i64,
    #[njson(default)]
    pub session_id: String,
    #[njson(default)]
    pub pool_count: i32,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct LoginResp {
    #[njson(default)]
    pub version: String,
    #[njson(default)]
    pub session_id: String,
    #[njson(default)]
    pub error: String,
    /// `"binary"` when the server accepts the `rustbinary` profile on this
    /// connection. Empty for servers that only speak JSON.
    #[njson(default)]
    pub codec: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct NewTunnel {
    #[njson(default)]
    pub tunnel_name: String,
    #[njson(default)]
    pub protocol: String,
    #[njson(default)]
    pub remote_port: i32,
    #[njson(default)]
    pub local_ip: String,
    #[njson(default)]
    pub local_port: i32,
    #[njson(default)]
    pub domains: Vec<String>,
    #[njson(default)]
    pub locations: Vec<String>,
    #[njson(default)]
    pub basic_auth_user: String,
    #[njson(default)]
    pub basic_auth_password: String,
    #[njson(default)]
    pub host_header_rewrite: String,
    #[njson(default)]
    pub headers: BTreeMap<String, String>,
    #[njson(default)]
    pub response_headers: BTreeMap<String, String>,
    #[njson(default)]
    pub bandwidth: f64,
    #[njson(default)]
    pub bandwidth_limit_side: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct NewTunnelResp {
    #[njson(default)]
    pub tunnel_name: String,
    #[njson(default)]
    pub remote_addr: String,
    #[njson(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct CloseTunnel {
    #[njson(default)]
    pub tunnel_name: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct ReqDataConn {}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct NewDataConn {
    #[njson(default)]
    pub session_id: String,
    #[njson(default)]
    pub auth_digest: String,
    #[njson(default)]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct StartDataConn {
    #[njson(default)]
    pub tunnel_name: String,
    #[njson(default)]
    pub src_addr: String,
    #[njson(default)]
    pub src_port: u16,
    #[njson(default)]
    pub dst_addr: String,
    #[njson(default)]
    pub dst_port: u16,
    #[njson(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct Ping {
    #[njson(default)]
    pub auth_digest: String,
    #[njson(default)]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct Pong {
    #[njson(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct KickOut {
    #[njson(default)]
    pub reason: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary,
)]
pub struct UdpSocketAddr {
    #[njson(default)]
    pub ip: String,
    #[njson(default)]
    pub port: u16,
}

impl UdpSocketAddr {
    pub fn from_std(addr: SocketAddr) -> Self {
        Self {
            ip: addr.ip().to_string(),
            port: addr.port(),
        }
    }

    pub fn to_std(&self) -> Option<SocketAddr> {
        let ip = IpAddr::from_str(&self.ip).ok()?;
        Some(SocketAddr::new(ip, self.port))
    }
}

#[derive(Debug, Clone, Default, NsonSerialize, NsonDeserialize, rustbinary::CompactBinary)]
pub struct UdpPacket {
    /// Base64 on the JSON path, a length-prefixed byte run on the binary path.
    #[njson(default, with = "b64_bytes", skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<u8>,
    #[njson(default, rename = "local", skip_serializing_if = "Option::is_none")]
    pub local_addr: Option<UdpSocketAddr>,
    #[njson(default, rename = "remote", skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<UdpSocketAddr>,
}

impl UdpPacket {
    pub fn new(content: Vec<u8>, remote: Option<SocketAddr>) -> Self {
        Self {
            content,
            local_addr: None,
            remote_addr: remote.map(UdpSocketAddr::from_std),
        }
    }

    /// Appends this packet as one `TYPE_UDP_PACKET` frame.
    ///
    /// Encoding straight from `&UdpPacket` lets the server coalesce queued
    /// datagrams without cloning their payloads into a `Message` first.
    pub fn encode_frame_into(&self, codec: Codec, out: &mut Vec<u8>) -> Result<(), FrameError> {
        append_frame(out, TYPE_UDP_PACKET, codec, self)
    }
}

mod b64_bytes {
    use alloc::vec::Vec;

    use nextjson::{FormatDecoder, FormatEncoder, FormatError};

    pub fn serialize<E: FormatEncoder>(data: &[u8], encoder: &mut E) -> Result<(), E::Error> {
        if data.len() > super::MAX_FRAME_BODY as usize / 4 * 3 {
            return Err(E::Error::custom("UDP payload exceeds frame size limit"));
        }
        encoder.write_str(&crate::proto::base64::encode(data))
    }

    pub fn deserialize<'de, D: FormatDecoder<'de>>(decoder: &mut D) -> Result<Vec<u8>, D::Error> {
        let text = decoder.string()?;
        if text.is_empty() {
            return Ok(Vec::new());
        }
        crate::proto::base64::decode(text.as_bytes())
            .ok_or_else(|| D::Error::custom("udp payload is not valid base64"))
    }
}

macro_rules! message_table {
    ($($tag:ident => $variant:ident($payload:ty)),* $(,)?) => {
        #[derive(Debug, Clone)]
        pub enum Message { $($variant($payload)),* }

        impl Message {
            pub(super) const fn supports(tag: u8) -> bool { matches!(tag, $($tag)|*) }
            pub fn type_byte(&self) -> u8 {
                match self {
                    $(Self::$variant(_) => $tag,)*
                }
            }

            /// Encodes the payload without the frame header.
            pub fn encode_body(&self, codec: Codec) -> Result<Vec<u8>, FrameError> {
                let mut out = Vec::with_capacity(128);
                self.encode_body_into(codec, &mut out)?;
                Ok(out)
            }

            /// Appends the payload to `out`, reusing its capacity.
            pub fn encode_body_into(
                &self,
                codec: Codec,
                out: &mut Vec<u8>,
            ) -> Result<(), FrameError> {
                match self {
                    $(Self::$variant(payload) => encode_value_into(payload, codec, out),)*
                }
            }

            /// Appends one complete frame to `out`, reusing its capacity.
            pub fn encode_frame_into(
                &self,
                codec: Codec,
                out: &mut Vec<u8>,
            ) -> Result<(), FrameError> {
                match self {
                    $(Self::$variant(payload) => append_frame(out, $tag, codec, payload),)*
                }
            }

            /// Decodes a payload previously produced by [`Message::encode_body`].
            pub fn decode_body(
                type_byte: u8,
                body: &[u8],
                codec: Codec,
            ) -> Result<Self, FrameError> {
                if body.len() > MAX_FRAME_BODY as usize { return Err(too_large(body.len())); }
                match type_byte {
                    $($tag => Ok(Self::$variant(decode_value::<$payload>(body, codec)?)),)*
                    other => Err(FrameError::UnknownType(other)),
                }
            }
        }
    };
}

message_table! {
    TYPE_LOGIN => Login(Login),
    TYPE_LOGIN_RESP => LoginResp(LoginResp),
    TYPE_NEW_TUNNEL => NewTunnel(NewTunnel),
    TYPE_NEW_TUNNEL_RESP => NewTunnelResp(NewTunnelResp),
    TYPE_CLOSE_TUNNEL => CloseTunnel(CloseTunnel),
    TYPE_REQ_DATA_CONN => ReqDataConn(ReqDataConn),
    TYPE_NEW_DATA_CONN => NewDataConn(NewDataConn),
    TYPE_START_DATA_CONN => StartDataConn(StartDataConn),
    TYPE_PING => Ping(Ping),
    TYPE_PONG => Pong(Pong),
    TYPE_UDP_PACKET => UdpPacket(UdpPacket),
    TYPE_KICK_OUT => KickOut(KickOut),
}

/// Appends one framed value: header, then body encoded straight into `out`.
pub(crate) fn append_frame<T>(
    out: &mut Vec<u8>,
    type_byte: u8,
    codec: Codec,
    value: &T,
) -> Result<(), FrameError>
where
    T: NsonSerialize + rustbinary::CompactEncode + ?Sized,
{
    let start = out.len();
    out.extend_from_slice(&[0u8; HEADER_LEN]);
    if let Err(err) = encode_value_into(value, codec, out) {
        out.truncate(start);
        return Err(err);
    }

    let len = (out.len() - start - HEADER_LEN) as u32;
    let header = FrameHeader {
        type_byte,
        codec,
        len,
    };
    let mut raw = [0u8; HEADER_LEN];
    header.encode(&mut raw);
    out[start..start + HEADER_LEN].copy_from_slice(&raw);
    Ok(())
}

fn too_large(len: usize) -> FrameError {
    FrameError::TooLarge(u32::try_from(len).unwrap_or(u32::MAX))
}

struct JsonBody<'a> {
    buffer: &'a mut Vec<u8>,
    start: usize,
    rejected: Option<usize>,
}
impl nextjson::Write for JsonBody<'_> {
    fn write_all(&mut self, bytes: &[u8]) -> nextjson::Result<()> {
        let len = (self.buffer.len() - self.start).saturating_add(bytes.len());
        if len > MAX_FRAME_BODY as usize {
            self.rejected = Some(len);
            return Err(nextjson::Error::custom("frame exceeds size limit"));
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }
}

fn encode_value_into<T>(value: &T, codec: Codec, out: &mut Vec<u8>) -> Result<(), FrameError>
where
    T: NsonSerialize + rustbinary::CompactEncode + ?Sized,
{
    match codec {
        Codec::Json => {
            let start = out.len();
            let mut writer = JsonBody {
                buffer: out,
                start,
                rejected: None,
            };
            nextjson::to_writer(&mut writer, value).map_err(|e| {
                writer.buffer.truncate(start);
                writer.rejected.map_or(FrameError::Json(e), too_large)
            })
        }
        Codec::Binary => {
            let compact = compact();
            let start = out.len();
            // A previous large batch can leave a lot of spare capacity. Limit
            // speculative zero-filling and retry at the exact size when needed.
            let mut capacity = out.capacity().saturating_sub(start).clamp(256, 4096);
            loop {
                out.resize(start + capacity, 0);
                match compact.serialize_into_slice(&mut out[start..], value) {
                    Ok(len) => {
                        out.truncate(start + len);
                        return Ok(());
                    }
                    Err(rustbinary::Error::BufferTooSmall { required, .. }) => {
                        out.truncate(start);
                        if required > MAX_FRAME_BODY as usize {
                            return Err(too_large(required));
                        }
                        capacity = required;
                    }
                    Err(err) => {
                        out.truncate(start);
                        return Err(FrameError::Binary(alloc::format!("{err}")));
                    }
                }
            }
        }
    }
}

fn decode_value<'de, T>(body: &'de [u8], codec: Codec) -> Result<T, FrameError>
where
    T: NsonDeserialize<'de> + rustbinary::CompactDecode<'de>,
{
    match codec {
        Codec::Json => nextjson::nextdecode(body).map_err(FrameError::Json),
        Codec::Binary => compact()
            .deserialize(body)
            .map_err(|e| FrameError::Binary(alloc::format!("{e}"))),
    }
}

/// The compact profile: no tags, no field names, length-prefixed containers,
/// with the default byte and collection ceilings kept at the trust boundary.
fn compact() -> rustbinary::CompactConfig {
    rustbinary::Config::standard().with_compact_format()
}
