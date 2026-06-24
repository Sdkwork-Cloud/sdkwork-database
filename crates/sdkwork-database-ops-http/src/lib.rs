mod auth;

use std::sync::Arc;

use axum::{extract::Query, http::HeaderMap, response::IntoResponse, routing::get, Json, Router};
use sdkwork_database_ops::DatabaseOpsService;
use sdkwork_database_spi::{DefaultDatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::DatabasePool;
use serde::Deserialize;

pub use auth::{BearerTokenOpsAuth, DatabaseOpsAuth, RejectAllOpsAuth};

#[derive(Clone)]
pub struct DatabaseOpsHttpState {
    pub service: Arc<DatabaseOpsService>,
    pub default_locale: LocaleTag,
    pub default_profile: SeedProfile,
    pub auth: Arc<dyn DatabaseOpsAuth>,
}

impl DatabaseOpsHttpState {
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
}

#[derive(Debug, Deserialize)]
struct DriftQuery {
    #[serde(default)]
    refresh: bool,
}

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
                                error.to_string(),
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
                                error.to_string(),
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
                                error.to_string(),
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
                                error.to_string(),
                            )
                                .into_response(),
                        }
                    }
                }
            }),
        )
}
