//! Pre-shared-key authentication.
//!
//! The digest is `HMAC-SHA256(token, decimal(timestamp))`. Verification is
//! constant time (`ring` compares the tags) and the timestamp window is checked
//! before the MAC so a clock-skewed peer does not consume hashing work.

use alloc::string::String;
use core::fmt;

use ring::hmac;

/// Accepted clock skew, in seconds, between peers.
pub const AUTH_SKEW_SECS: i64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    EmptyDigest,
    InvalidDigest,
    TimestampSkew,
}

impl AuthFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyDigest => "empty authentication digest",
            Self::InvalidDigest => "invalid authentication digest",
            Self::TimestampSkew => "timestamp outside allowed window",
        }
    }
}

impl fmt::Display for AuthFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl core::error::Error for AuthFailure {}

pub fn compute_auth_digest(token: &str, timestamp: i64) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, token.as_bytes());
    crate::proto::hex::encode(hmac::sign(&key, decimal(timestamp, &mut [0; 20])).as_ref())
}

/// Verifies a digest produced by [`compute_auth_digest`].
///
/// An empty `token` disables authentication, matching the `auth.token` config
/// semantics: the operator opted into an open endpoint.
pub fn verify_auth_digest(
    token: &str,
    auth_digest: &str,
    timestamp: i64,
    now_secs: i64,
) -> Result<(), AuthFailure> {
    if token.is_empty() {
        return Ok(());
    }
    if auth_digest.is_empty() {
        return Err(AuthFailure::EmptyDigest);
    }
    if now_secs.abs_diff(timestamp) > AUTH_SKEW_SECS as u64 {
        return Err(AuthFailure::TimestampSkew);
    }

    let mut expected = [0u8; 32];
    crate::proto::hex::decode_into(auth_digest.as_bytes(), &mut expected)
        .ok_or(AuthFailure::InvalidDigest)?;

    let key = hmac::Key::new(hmac::HMAC_SHA256, token.as_bytes());
    hmac::verify(&key, decimal(timestamp, &mut [0; 20]), &expected)
        .map_err(|_| AuthFailure::InvalidDigest)
}

fn decimal(value: i64, buf: &mut [u8; 20]) -> &[u8] {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut cursor = buf.len();
    loop {
        cursor -= 1;
        buf[cursor] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        cursor -= 1;
        buf[cursor] = b'-';
    }
    &buf[cursor..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_roundtrip_and_skew() {
        let now = 1_700_000_000;
        let digest = compute_auth_digest("s3cret", now);
        assert_eq!(
            digest,
            "b82d76a98690627a418267faa0be5e8830d04d82cc88b5450d2c0f32173127c5"
        );
        assert!(verify_auth_digest("s3cret", &digest, now, now).is_ok());
        assert_eq!(
            verify_auth_digest("wrong", &digest, now, now),
            Err(AuthFailure::InvalidDigest)
        );
        assert_eq!(
            verify_auth_digest("s3cret", &digest, now, now + AUTH_SKEW_SECS + 1),
            Err(AuthFailure::TimestampSkew)
        );
        assert_eq!(
            verify_auth_digest("s3cret", "", now, now),
            Err(AuthFailure::EmptyDigest)
        );
        assert!(verify_auth_digest("", "", now, now).is_ok());
    }

    #[test]
    fn negative_timestamps_format() {
        assert_eq!(decimal(0, &mut [0; 20]), b"0");
        assert_eq!(decimal(-1, &mut [0; 20]), b"-1");
        assert_eq!(decimal(i64::MIN, &mut [0; 20]), b"-9223372036854775808");
        assert_eq!(decimal(i64::MAX, &mut [0; 20]), b"9223372036854775807");
    }
}
