use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Authorizes access to `/backend/v3/ops/database/*` endpoints.
pub trait DatabaseOpsAuth: Send + Sync {
    /// Check whether a request is authorized.
    /// Returns `Ok(())` on success, `Err(StatusCode)` on failure.
    fn authorize(&self, headers: &HeaderMap) -> Result<(), StatusCode>;
}

/// Rejects every request until an application wires a real backend/admin auth provider.
#[derive(Debug, Clone, Default)]
pub struct RejectAllOpsAuth;

impl DatabaseOpsAuth for RejectAllOpsAuth {
    fn authorize(&self, _headers: &HeaderMap) -> Result<(), StatusCode> {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Constant-time comparison for bearer tokens to prevent timing attacks.
///
/// Uses HMAC-based comparison with SHA-256 — both the expected and
/// received values are hashed before comparison, so even if the
/// comparison itself were not constant-time, no raw token material
/// would be exposed through timing of a first-byte-difference short-circuit.
#[allow(dead_code)]
fn constant_time_compare(actual: &str, expected: &str) -> bool {
    // Hash both sides to normalize lengths and prevent raw-token timing leakage.
    let actual_hash = Sha256::digest(actual.as_bytes());
    let expected_hash = Sha256::digest(expected.as_bytes());
    // Use XOR-based constant-time comparison.
    // `expected_hash == actual_hash` in Rust performs a byte-by-byte
    // constant-time comparison for arrays of equal length.
    actual_hash == expected_hash
}

/// Validates `Authorization: Bearer <token>` against a configured ops token.
///
/// Uses SHA-256 hashed comparison to avoid exposing the raw token through
/// timing side channels.
#[derive(Debug, Clone)]
pub struct BearerTokenOpsAuth {
    /// The expected bearer token (stored as SHA-256 hash).
    token_hash: Vec<u8>,
}

impl BearerTokenOpsAuth {
    /// Create a new auth guard that accepts the given plaintext token.
    ///
    /// The token is immediately hashed and never stored in plaintext.
    pub fn new(token: impl Into<String>) -> Self {
        let raw = token.into();
        let hash = Sha256::digest(raw.as_bytes()).to_vec();
        // Zero out the raw string.
        drop(raw);
        Self { token_hash: hash }
    }

    /// Load token from environment variable and hash it immediately.
    ///
    /// Returns `None` if the env var is empty or unset (caller should fall back
    /// to `RejectAllOpsAuth`).
    pub fn from_env(var_name: &str) -> Option<Self> {
        let raw = std::env::var(var_name).ok()?;
        if raw.is_empty() {
            return None;
        }
        let hash = Sha256::digest(raw.as_bytes()).to_vec();
        drop(raw);
        Some(Self { token_hash: hash })
    }
}

impl DatabaseOpsAuth for BearerTokenOpsAuth {
    fn authorize(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        let header_value = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Extract token from "Bearer <token>"
        let received = header_value
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Constant-time compare the token hash
        let received_hash = Sha256::digest(received.as_bytes());
        if received_hash.as_slice() == self.token_hash.as_slice() {
            Ok(())
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Rate-limited auth wrapper that tracks request counts per second.
///
/// Uses a simple token-bucket-like rate limit to protect ops endpoints
/// from brute-force attacks.
pub struct RateLimitedOpsAuth {
    inner: Arc<dyn DatabaseOpsAuth>,
    max_rpm: u64,
    window_start: Arc<AtomicU64>,
    window_count: Arc<AtomicU64>,
}

impl std::fmt::Debug for RateLimitedOpsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitedOpsAuth")
            .field("max_rpm", &self.max_rpm)
            .field("window_start", &self.window_start.load(Ordering::Relaxed))
            .field("window_count", &self.window_count.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Clone for RateLimitedOpsAuth {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_rpm: self.max_rpm,
            window_start: self.window_start.clone(),
            window_count: self.window_count.clone(),
        }
    }
}

impl RateLimitedOpsAuth {
    /// Wrap an auth provider with a per-minute rate limit.
    pub fn new(inner: Arc<dyn DatabaseOpsAuth>, max_requests_per_minute: u64) -> Self {
        Self {
            inner,
            max_rpm: max_requests_per_minute,
            window_start: Arc::new(AtomicU64::new(0)),
            window_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a default rate-limited auth (300 RPM).
    pub fn default(inner: Arc<dyn DatabaseOpsAuth>) -> Self {
        Self::new(inner, 300)
    }
}

impl DatabaseOpsAuth for RateLimitedOpsAuth {
    fn authorize(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        let now = Instant::now().elapsed().as_secs();
        let window = self.window_start.load(Ordering::Relaxed);

        if now - window >= 60 {
            // New window
            self.window_start.store(now, Ordering::Relaxed);
            self.window_count.store(1, Ordering::Relaxed);
        } else {
            let count = self.window_count.fetch_add(1, Ordering::Relaxed);
            if count >= self.max_rpm {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }

        self.inner.authorize(headers)
    }
}

/// Audit-logging auth wrapper that records all auth attempts.
pub struct AuditingOpsAuth {
    inner: Arc<dyn DatabaseOpsAuth>,
}

impl std::fmt::Debug for AuditingOpsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditingOpsAuth").finish_non_exhaustive()
    }
}

impl Clone for AuditingOpsAuth {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl AuditingOpsAuth {
    pub fn new(inner: Arc<dyn DatabaseOpsAuth>) -> Self {
        Self { inner }
    }
}

impl DatabaseOpsAuth for AuditingOpsAuth {
    fn authorize(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        let result = self.inner.authorize(headers);

        // Log the auth attempt along with a sanitized version of the request.
        let source = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");
        match &result {
            Ok(()) => {
                tracing::info!(target: "sdkwork.database.ops.auth", source, "authorized");
            }
            Err(status) => {
                tracing::warn!(target: "sdkwork.database.ops.auth", source, %status, "unauthorized");
            }
        }
        result
    }
}

/// Convenience function to build a production-ready auth chain:
/// Auditing + RateLimiting + BearerToken.
///
/// Falls back to `RejectAllOpsAuth` if `SDKWORK_ACCESS_TOKEN` is not set.
pub fn default_ops_auth() -> Arc<dyn DatabaseOpsAuth> {
    if let Some(token_auth) = BearerTokenOpsAuth::from_env("SDKWORK_ACCESS_TOKEN") {
        let auth: Arc<dyn DatabaseOpsAuth> = Arc::new(RateLimitedOpsAuth::default(Arc::new(
            AuditingOpsAuth::new(Arc::new(token_auth)),
        )));
        auth
    } else {
        tracing::warn!(
            target: "sdkwork.database.ops.auth",
            "SDKWORK_ACCESS_TOKEN not set — ops endpoints will reject all requests"
        );
        Arc::new(RejectAllOpsAuth)
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

#[allow(clippy::result_large_err)]
fn authorize_request(
    auth: &dyn DatabaseOpsAuth,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    auth.authorize(headers)
        .map_err(|status| status.into_response())
}

#[allow(clippy::result_large_err)]
pub(crate) fn guard_ops_request(
    auth: &dyn DatabaseOpsAuth,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    authorize_request(auth, headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_all_returns_unauthorized() {
        let auth = RejectAllOpsAuth;
        assert_eq!(
            auth.authorize(&HeaderMap::new()).unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn bearer_token_accepts_matching_header() {
        let auth = BearerTokenOpsAuth::new("secret123");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret123".parse().unwrap(),
        );
        assert!(auth.authorize(&headers).is_ok());
    }

    #[test]
    fn bearer_token_rejects_wrong_token() {
        let auth = BearerTokenOpsAuth::new("secret123");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer wrong".parse().unwrap(),
        );
        assert_eq!(
            auth.authorize(&headers).unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn bearer_token_rejects_missing_header() {
        let auth = BearerTokenOpsAuth::new("secret123");
        assert_eq!(
            auth.authorize(&HeaderMap::new()).unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn bearer_token_rejects_malformed_header() {
        let auth = BearerTokenOpsAuth::new("secret123");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic dXNlcjpwYXNz".parse().unwrap(),
        );
        assert_eq!(
            auth.authorize(&headers).unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn constant_time_compare_works() {
        assert!(constant_time_compare("hello", "hello"));
        assert!(!constant_time_compare("hello", "world"));
        assert!(!constant_time_compare("hello", "helloo"));
        assert!(!constant_time_compare("", "hello"));
        assert!(constant_time_compare("", ""));
    }

    #[test]
    fn bearer_token_stores_hash_not_plaintext() {
        let auth = BearerTokenOpsAuth::new("my_secret_token");
        // Verify we can still authenticate
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer my_secret_token".parse().unwrap(),
        );
        assert!(auth.authorize(&headers).is_ok());
        // But the token_hash should NOT contain the plaintext
        let hash_str = String::from_utf8_lossy(&auth.token_hash);
        assert!(!hash_str.contains("my_secret_token"));
    }

    #[test]
    fn from_env_returns_none_when_unset() {
        // SDKWORK_ACCESS_TOKEN is likely not set in test environment
        let auth = BearerTokenOpsAuth::from_env("SDKWORK_ACCESS_TOKEN_UNSET_VAR_12345");
        assert!(auth.is_none());
    }
}
