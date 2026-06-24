use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

/// Authorizes access to `/backend/v3/ops/database/*` endpoints.
pub trait DatabaseOpsAuth: Send + Sync {
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

/// Validates `Authorization: Bearer <token>` against a configured ops token.
#[derive(Debug, Clone)]
pub struct BearerTokenOpsAuth {
    token: String,
}

impl BearerTokenOpsAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    pub fn from_env(var_name: &str) -> Self {
        Self::new(std::env::var(var_name).unwrap_or_default())
    }
}

impl DatabaseOpsAuth for BearerTokenOpsAuth {
    fn authorize(&self, headers: &HeaderMap) -> Result<(), StatusCode> {
        if self.token.is_empty() {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let expected = format!("Bearer {}", self.token);
        let authorized = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == expected);

        if authorized {
            Ok(())
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

fn authorize_request(
    auth: &dyn DatabaseOpsAuth,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    auth.authorize(headers)
        .map_err(|status| status.into_response())
}

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
        let auth = BearerTokenOpsAuth::new("secret");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret".parse().unwrap(),
        );
        assert!(auth.authorize(&headers).is_ok());
    }
}
