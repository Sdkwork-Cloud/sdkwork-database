//! Axum router and auth for SDKWork database ops endpoints.
//!
//! Provides:
//! - `/backend/v3/ops/database/status`
//! - `/backend/v3/ops/database/drift`
//! - `/backend/v3/ops/database/migrations`
//! - `/backend/v3/ops/database/seeds`
//!
//! Authentication is pluggable via [`DatabaseOpsAuth`]. Production deployments
//! SHOULD use the composable auth chain:
//! `AuditingOpsAuth(RateLimitedOpsAuth(BearerTokenOpsAuth))`.

mod auth;

use std::sync::Arc;

use axum::{extract::Query, http::HeaderMap, response::IntoResponse, routing::get, Json, Router};
use sdkwork_database_ops::DatabaseOpsService;
use sdkwork_database_spi::{DefaultDatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::DatabasePool;
use serde::Deserialize;

pub use auth::{
    default_ops_auth, AuditingOpsAuth, BearerTokenOpsAuth, DatabaseOpsAuth, RateLimitedOpsAuth,
    RejectAllOpsAuth,
};

/// Shared state for ops HTTP routes.
#[derive(Clone)]
pub struct DatabaseOpsHttpState {
    pub service: Arc<DatabaseOpsService>,
    pub default_locale: LocaleTag,
    pub default_profile: SeedProfile,
    pub auth: Arc<dyn DatabaseOpsAuth>,
}

impl DatabaseOpsHttpState {
    /// Create a new state from a pool, module, and auth provider.
    pub fn new(
        pool: DatabasePool,
        module: Arc<DefaultDatabaseModule>,
        default_locale: LocaleTag,
        default_profile: SeedProfile,
        auth: Arc<dyn DatabaseOpsAuth>,
    ) -> Self {
        Self {
            service: Arc::new(DatabaseOpsService::new(pool, module)),
            default_locale,
            default_profile,
            auth,
        }
    }

    /// Create a state with the production-ready default auth chain.
    ///
    /// Falls back to `RejectAllOpsAuth` if `SDKWORK_ACCESS_TOKEN` is not set.
    pub fn new_with_default_auth(
        pool: DatabasePool,
        module: Arc<DefaultDatabaseModule>,
        default_locale: LocaleTag,
        default_profile: SeedProfile,
    ) -> Self {
        Self::new(
            pool,
            module,
            default_locale,
            default_profile,
            default_ops_auth(),
        )
    }
}

#[derive(Debug, Deserialize)]
struct DriftQuery {
    #[serde(default)]
    refresh: bool,
}

/// Attach ops routes to an existing Axum router.
///
/// All routes are protected by the provided [`DatabaseOpsAuth`] implementation.
pub fn attach_ops_routes<S>(router: Router<S>, state: DatabaseOpsHttpState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .route(
            "/backend/v3/ops/database/status",
            get({
                let state = state.clone();
                move |headers: HeaderMap| {
                    let state = state.clone();
                    async move {
                        if let Err(response) =
                            auth::guard_ops_request(state.auth.as_ref(), &headers)
                        {
                            return response;
                        }
                        match state.service.status().await {
                            Ok(report) => Json(report).into_response(),
                            Err(error) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                serde_json::json!({
                                    "error": "status_check_failed",
                                    "message": error.to_string()
                                })
                                .to_string(),
                            )
                                .into_response(),
                        }
                    }
                }
            }),
        )
        .route(
            "/backend/v3/ops/database/drift",
            get({
                let state = state.clone();
                move |query: Query<DriftQuery>, headers: HeaderMap| {
                    let state = state.clone();
                    let refresh = query.refresh;
                    async move {
                        if let Err(response) =
                            auth::guard_ops_request(state.auth.as_ref(), &headers)
                        {
                            return response;
                        }
                        match state.service.drift(refresh).await {
                            Ok(report) => Json(report).into_response(),
                            Err(error) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                serde_json::json!({
                                    "error": "drift_check_failed",
                                    "message": error.to_string()
                                })
                                .to_string(),
                            )
                                .into_response(),
                        }
                    }
                }
            }),
        )
        .route(
            "/backend/v3/ops/database/migrations",
            get({
                let state = state.clone();
                move |headers: HeaderMap| {
                    let state = state.clone();
                    async move {
                        if let Err(response) =
                            auth::guard_ops_request(state.auth.as_ref(), &headers)
                        {
                            return response;
                        }
                        match state.service.migrations().await {
                            Ok(report) => Json(report).into_response(),
                            Err(error) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                serde_json::json!({
                                    "error": "migrations_query_failed",
                                    "message": error.to_string()
                                })
                                .to_string(),
                            )
                                .into_response(),
                        }
                    }
                }
            }),
        )
        .route(
            "/backend/v3/ops/database/seeds",
            get({
                let state = state.clone();
                move |headers: HeaderMap| {
                    let state = state.clone();
                    async move {
                        if let Err(response) =
                            auth::guard_ops_request(state.auth.as_ref(), &headers)
                        {
                            return response;
                        }
                        match state
                            .service
                            .seeds(&state.default_locale, &state.default_profile)
                            .await
                        {
                            Ok(report) => Json(report).into_response(),
                            Err(error) => (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                serde_json::json!({
                                    "error": "seeds_query_failed",
                                    "message": error.to_string()
                                })
                                .to_string(),
                            )
                                .into_response(),
                        }
                    }
                }
            }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_default_ops_auth_fallback_to_reject() {
        // When SDKWORK_ACCESS_TOKEN is not set, should fall back to RejectAllOpsAuth
        let auth = default_ops_auth();
        let headers = HeaderMap::new();
        assert_eq!(
            auth.authorize(&headers).unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }
}
