//! Authentication: the `no_std` primitives live in [`crate::proto::auth`]; the
//! replay window needs a monotonic clock and lives here.

pub use crate::proto::auth::{
    AUTH_SKEW_SECS, AuthFailure, compute_auth_digest, verify_auth_digest,
};

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a digest stays in the replay window.
pub const REPLAY_TTL_SECS: u64 = (AUTH_SKEW_SECS as u64) * 2;

/// Hard cap on distinct digests held at once.
pub const REPLAY_MAX_ENTRIES: usize = 100_000;

/// Extended failure set for the server-side handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginFailure {
    Auth(AuthFailure),
    Replay,
    Capacity,
}

impl LoginFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auth(failure) => failure.as_str(),
            Self::Replay => "authentication digest reused",
            Self::Capacity => "authentication replay cache full",
        }
    }
}

impl From<AuthFailure> for LoginFailure {
    fn from(value: AuthFailure) -> Self {
        Self::Auth(value)
    }
}

impl std::fmt::Display for LoginFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Bounded, self-purging set of accepted digests.
#[derive(Debug, Default)]
pub struct ReplayCache {
    inner: Mutex<ReplayWindow>,
}

#[derive(Debug, Default)]
struct ReplayWindow {
    keys: HashSet<[u8; 32]>,
    expiry: VecDeque<(Instant, [u8; 32])>,
}

impl ReplayCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accept(&self, digest: &str) -> Result<(), LoginFailure> {
        let mut key = [0; 32];
        crate::proto::hex::decode_into(digest.as_bytes(), &mut key)
            .ok_or(AuthFailure::InvalidDigest)?;
        let mut window = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // Take the timestamp under the lock so FIFO expiry stays ordered.
        window.insert(key, Instant::now())
    }
}

impl ReplayWindow {
    fn insert(&mut self, key: [u8; 32], now: Instant) -> Result<(), LoginFailure> {
        while let Some((_, expired)) = self.expiry.pop_front_if(|(end, _)| *end <= now) {
            self.keys.remove(&expired);
        }
        if self.keys.contains(&key) {
            return Err(LoginFailure::Replay);
        }
        if self.keys.len() >= REPLAY_MAX_ENTRIES {
            return Err(LoginFailure::Capacity);
        }
        self.keys.insert(key);
        self.expiry
            .push_back((now + Duration::from_secs(REPLAY_TTL_SECS), key));
        Ok(())
    }
}

/// Verifies a login digest and records it against replay.
pub fn verify_login(
    token: &str,
    auth_digest: &str,
    timestamp: i64,
    now_secs: i64,
    replay: &ReplayCache,
) -> Result<(), LoginFailure> {
    verify_auth_digest(token, auth_digest, timestamp, now_secs)?;
    if token.is_empty() {
        return Ok(());
    }
    replay.accept(auth_digest)
}

pub fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keys_match_the_authenticated_digest_bytes() {
        let replay = ReplayCache::new();
        let digest = compute_auth_digest("token", 101);
        assert!(verify_login("token", &digest, 101, 101, &replay).is_ok());
        assert_eq!(
            verify_login("token", &digest.to_uppercase(), 101, 101, &replay),
            Err(LoginFailure::Replay)
        );
    }

    #[test]
    fn expires_only_elapsed_entries() {
        let now = Instant::now();
        let mut window = ReplayWindow::default();
        window.insert([1; 32], now).unwrap();
        window
            .insert([2; 32], now + Duration::from_secs(1))
            .unwrap();
        let later = now + Duration::from_secs(REPLAY_TTL_SECS);
        assert!(window.insert([1; 32], later).is_ok());
        assert_eq!(window.insert([2; 32], later), Err(LoginFailure::Replay));
    }
}
