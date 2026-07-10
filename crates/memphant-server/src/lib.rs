use std::sync::Arc;

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memphant_core::service::{MemoryService, ServiceError, clamp_trust};
use memphant_core::{CoreError, InMemoryStore, MemoryStore, NoopEmbedding, SystemClock};
use memphant_types::{
    CorrectRequest, ENGINE_VERSION, ErrorBody, ErrorEnvelope, HealthResponse, MarkRequest,
    RecallHttpRequest, ReflectRequest, ReflectResult, RetainEpisodeHttpRequest,
    RetainEpisodeHttpResponse, RetrievalTrace, SCHEMA_COMPAT_REVISION, ScopeMemoryResponse,
    TRACE_SCHEMA_VERSION, TenantId, TrustLevel,
};
use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const HEALTH_PATH: &str = "/v1/health";
const OPENAPI_PATH: &str = "/v1/openapi.json";
const EPISODES_PATH: &str = "/v1/episodes";
const RECALL_PATH: &str = "/v1/recall";
const REFLECT_PATH: &str = "/v1/reflect";
const CORRECT_PATH: &str = "/v1/correct";
const FORGET_PATH: &str = "/v1/forget";
const MARK_PATH: &str = "/v1/mark";
const TRACE_PATH: &str = "/v1/traces/{id}";
const SCOPE_MEMORY_PATH: &str = "/v1/scopes/{id}/memory";

const DOCUMENTED_OPENAPI_PATHS: &[&str] = &[
    EPISODES_PATH,
    RECALL_PATH,
    REFLECT_PATH,
    CORRECT_PATH,
    FORGET_PATH,
    MARK_PATH,
    TRACE_PATH,
    SCOPE_MEMORY_PATH,
    HEALTH_PATH,
];

pub fn documented_openapi_paths() -> &'static [&'static str] {
    DOCUMENTED_OPENAPI_PATHS
}

/// Hashes a presented bearer token into the stored `api_key.key_hash` form.
pub fn api_key_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub struct AppState<S: MemoryStore> {
    service: MemoryService<S>,
    store_name: &'static str,
    dev_tenant: Option<TenantId>,
}

impl<S: MemoryStore> Clone for AppState<S> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            store_name: self.store_name,
            dev_tenant: self.dev_tenant,
        }
    }
}

impl AppState<InMemoryStore> {
    pub fn new_in_memory() -> Self {
        Self::from_service(
            MemoryService::new(
                Arc::new(InMemoryStore::default()),
                Arc::new(SystemClock),
                Arc::new(NoopEmbedding),
            ),
            "memory",
        )
    }
}

impl<S: MemoryStore> AppState<S> {
    pub fn from_service(service: MemoryService<S>, store_name: &'static str) -> Self {
        Self {
            service,
            store_name,
            dev_tenant: None,
        }
    }

    /// Dev mode: binds ALL requests to this tenant, ignoring body tenant ids.
    /// Wired from `MEMPHANT_DEV_TENANT` in `main`; loud by design.
    pub fn with_dev_tenant(mut self, tenant: TenantId) -> Self {
        eprintln!(
            "memphant-server: AUTH DISABLED (dev) — all requests bound to tenant {}",
            tenant.as_uuid()
        );
        self.dev_tenant = Some(tenant);
        self
    }

    pub fn service(&self) -> &MemoryService<S> {
        &self.service
    }

    pub fn store(&self) -> &S {
        self.service.store()
    }
}

/// The tenant binding + trust ceiling resolved from `Authorization: Bearer
/// mk_…` (or the dev-mode tenant). All tenant-scoped reads/writes are bound
/// server-side to this value, never to client-declared body fields.
#[derive(Debug, Clone, Copy)]
pub struct AuthedTenant {
    pub tenant: TenantId,
    pub max_trust: TrustLevel,
    dev_mode: bool,
}

impl AuthedTenant {
    /// The tenant every request body must carry (unless dev mode ignores it).
    fn check_body_tenant(&self, body_tenant: TenantId) -> Result<(), ApiError> {
        if self.dev_mode || body_tenant == self.tenant {
            Ok(())
        } else {
            Err(ApiError::tenant_mismatch())
        }
    }
}

impl<S: MemoryStore + 'static> FromRequestParts<AppState<S>> for AuthedTenant {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState<S>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(tenant) = state.dev_tenant {
            return Ok(Self {
                tenant,
                max_trust: TrustLevel::TrustedSystem,
                dev_mode: true,
            });
        }
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(ApiError::auth_required)?;
        let token = header
            .strip_prefix("Bearer ")
            .filter(|token| token.starts_with("mk_"))
            .ok_or_else(ApiError::auth_required)?;
        let row = state
            .service
            .store()
            .lookup_api_key(&api_key_hash(token))
            .await
            .map_err(|_| ApiError::backend_unavailable())?
            .ok_or_else(ApiError::auth_required)?;
        if row.revoked {
            return Err(ApiError::auth_required());
        }
        Ok(Self {
            tenant: row.tenant_id,
            max_trust: row.max_trust,
            dev_mode: false,
        })
    }
}

pub fn app<S: MemoryStore + 'static>(state: AppState<S>) -> Router {
    Router::new()
        .route(HEALTH_PATH, get(health::<S>))
        .route(OPENAPI_PATH, get(openapi))
        .route(EPISODES_PATH, post(retain_handler::<S>))
        .route(RECALL_PATH, post(recall_handler::<S>))
        .route(REFLECT_PATH, post(reflect_handler::<S>))
        .route(CORRECT_PATH, post(correct_handler::<S>))
        .route(FORGET_PATH, post(forget_handler::<S>))
        .route(MARK_PATH, post(mark_handler::<S>))
        .route(TRACE_PATH, get(trace_handler::<S>))
        .route(SCOPE_MEMORY_PATH, get(scope_memory_handler::<S>))
        .with_state(state)
}

async fn health<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
) -> Json<HealthResponse> {
    let db_ok = state.store().ping().await.is_ok();
    let dead_letter_jobs = state.store().dead_letter_count().await.ok();
    Json(HealthResponse {
        status: if db_ok { "ok" } else { "degraded" }.to_string(),
        store: state.store_name.to_string(),
        dead_letter_jobs,
        engine_version: ENGINE_VERSION.to_string(),
        trace_schema_version: TRACE_SCHEMA_VERSION.to_string(),
        schema_compat_revision: SCHEMA_COMPAT_REVISION.to_string(),
    })
}

async fn openapi() -> Json<Value> {
    Json(openapi_document())
}

async fn retain_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(mut request): Json<RetainEpisodeHttpRequest>,
) -> Result<Json<RetainEpisodeHttpResponse>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    // Caller-declared trust is a hint, capped at the key's ceiling.
    request.source_trust = clamp_trust(request.source_trust, authed.max_trust);
    Ok(Json(state.service.retain(authed.tenant, request).await?))
}

async fn reflect_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(request): Json<ReflectRequest>,
) -> Result<Json<ReflectResult>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    Ok(Json(
        state
            .service
            .reflect(authed.tenant, request.scope_id, request.compiler_version)
            .await?,
    ))
}

async fn recall_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(request): Json<RecallHttpRequest>,
) -> Result<Json<memphant_types::RecallResponse>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    Ok(Json(state.service.recall(authed.tenant, request).await?))
}

async fn correct_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(request): Json<CorrectRequest>,
) -> Result<Json<memphant_types::CorrectResult>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    Ok(Json(state.service.correct(authed.tenant, request).await?))
}

async fn forget_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(request): Json<memphant_types::ForgetRequest>,
) -> Result<Json<memphant_types::ForgetResult>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    Ok(Json(state.service.forget(authed.tenant, request).await?))
}

async fn mark_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Json(request): Json<MarkRequest>,
) -> Result<Json<memphant_types::MarkResult>, ApiError> {
    authed.check_body_tenant(request.tenant_id)?;
    Ok(Json(state.service.mark(authed.tenant, request).await?))
}

async fn trace_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Path(id): Path<String>,
) -> Result<Json<RetrievalTrace>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::invalid("invalid trace id"))?;
    let trace_id = memphant_types::TraceId::from_u128(uuid.as_u128());
    state
        .service
        .trace(authed.tenant, trace_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("trace"))
}

#[derive(Debug, Deserialize)]
struct ScopeMemoryQuery {
    #[serde(default)]
    cursor: Option<Uuid>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn scope_memory_handler<S: MemoryStore + 'static>(
    State(state): State<AppState<S>>,
    authed: AuthedTenant,
    Path(id): Path<String>,
    Query(query): Query<ScopeMemoryQuery>,
) -> Result<Json<ScopeMemoryResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::invalid("invalid scope id"))?;
    let scope_id = memphant_types::ScopeId::from_u128(uuid.as_u128());
    let cursor = query
        .cursor
        .map(|cursor| memphant_types::UnitId::from_u128(cursor.as_u128()));
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let page = state
        .service
        .scope_memory_page(authed.tenant, scope_id, cursor, limit)
        .await?;
    Ok(Json(ScopeMemoryResponse {
        tenant_id: authed.tenant,
        scope_id,
        items: page.items,
        next_cursor: page.next_cursor.map(|cursor| cursor.as_uuid().to_string()),
        has_more: page.has_more,
    }))
}

pub fn openapi_document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "MemPhant API",
            "version": ENGINE_VERSION
        },
        "paths": openapi_paths(),
        "security": [ { "bearerApiKey": [] } ],
        "components": {
            "securitySchemes": {
                "bearerApiKey": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "mk_<key>",
                    "description": "MemPhant API key; the key binds the tenant and caps source_trust at its max_trust tier."
                }
            },
            "schemas": component_schemas()
        }
    })
}

fn openapi_paths() -> serde_json::Map<String, Value> {
    let mut paths = serde_json::Map::new();
    paths.insert(
        EPISODES_PATH.to_string(),
        path_item(
            "post",
            "RetainEpisodeHttpRequest",
            "RetainEpisodeHttpResponse",
        ),
    );
    paths.insert(
        RECALL_PATH.to_string(),
        path_item("post", "RecallHttpRequest", "RecallResponse"),
    );
    paths.insert(
        REFLECT_PATH.to_string(),
        path_item("post", "ReflectRequest", "ReflectResult"),
    );
    paths.insert(
        CORRECT_PATH.to_string(),
        path_item("post", "CorrectRequest", "CorrectResult"),
    );
    paths.insert(
        FORGET_PATH.to_string(),
        path_item("post", "ForgetRequest", "ForgetResult"),
    );
    paths.insert(
        MARK_PATH.to_string(),
        path_item("post", "MarkRequest", "MarkResult"),
    );
    paths.insert(
        TRACE_PATH.to_string(),
        get_path_item("RetrievalTrace", vec![path_param("id")]),
    );
    paths.insert(
        SCOPE_MEMORY_PATH.to_string(),
        get_path_item(
            "ScopeMemoryResponse",
            vec![
                path_param("id"),
                optional_query_param("cursor", "uuid"),
                optional_query_param("limit", "integer"),
            ],
        ),
    );
    paths.insert(
        HEALTH_PATH.to_string(),
        get_path_item("HealthResponse", Vec::new()),
    );
    paths
}

fn component_schemas() -> serde_json::Map<String, Value> {
    let mut generator = openapi_schema_generator();
    seed_component::<RetainEpisodeHttpRequest>(&mut generator);
    seed_component::<RetainEpisodeHttpResponse>(&mut generator);
    seed_component::<RecallHttpRequest>(&mut generator);
    seed_component::<memphant_types::RecallResponse>(&mut generator);
    seed_component::<ReflectRequest>(&mut generator);
    seed_component::<ReflectResult>(&mut generator);
    seed_component::<CorrectRequest>(&mut generator);
    seed_component::<memphant_types::CorrectResult>(&mut generator);
    seed_component::<memphant_types::ForgetRequest>(&mut generator);
    seed_component::<memphant_types::ForgetResult>(&mut generator);
    seed_component::<MarkRequest>(&mut generator);
    seed_component::<memphant_types::MarkResult>(&mut generator);
    seed_component::<RetrievalTrace>(&mut generator);
    seed_component::<ScopeMemoryResponse>(&mut generator);
    seed_component::<HealthResponse>(&mut generator);
    seed_component::<ErrorEnvelope>(&mut generator);
    generator.take_definitions(true)
}

fn openapi_schema_generator() -> SchemaGenerator {
    SchemaSettings::draft2020_12()
        .with(|settings| {
            settings.definitions_path = "/components/schemas".into();
            settings.meta_schema = None;
        })
        .into_generator()
}

fn seed_component<T: JsonSchema>(generator: &mut SchemaGenerator) {
    let _ = generator.subschema_for::<T>();
}

fn path_item(method: &str, input_schema: &str, output_schema: &str) -> Value {
    json!({
        method: {
            "requestBody": {
                "content": {
                    "application/json": {
                        "schema": { "$ref": format!("#/components/schemas/{input_schema}") }
                    }
                }
            },
            "responses": {
                "200": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{output_schema}") }
                        }
                    }
                },
                "default": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorEnvelope" }
                        }
                    }
                }
            }
        }
    })
}

fn get_path_item(output_schema: &str, parameters: Vec<Value>) -> Value {
    json!({
        "get": {
            "parameters": parameters,
            "responses": {
                "200": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{output_schema}") }
                        }
                    }
                },
                "default": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorEnvelope" }
                        }
                    }
                }
            }
        }
    })
}

fn path_param(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": { "type": "string", "format": "uuid" }
    })
}

fn optional_query_param(name: &str, format: &str) -> Value {
    let schema = if format == "integer" {
        json!({ "type": "integer" })
    } else {
        json!({ "type": "string", "format": format })
    };
    json!({
        "name": name,
        "in": "query",
        "required": false,
        "schema": schema
    })
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn not_found(entity: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: format!("{entity} not found"),
        }
    }

    fn auth_required() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "auth_required",
            message: "a valid Authorization: Bearer mk_<key> header is required".to_string(),
        }
    }

    fn tenant_mismatch() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "tenant_mismatch",
            message: "body tenant_id does not match the API key's tenant".to_string(),
        }
    }

    fn backend_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "backend_unavailable",
            message: "backend unavailable".to_string(),
        }
    }
}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::EmptyBody | CoreError::Invalid(_) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "invalid_request",
                message: error.to_string(),
            },
            CoreError::NotFound(_) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: error.to_string(),
            },
            CoreError::PolicyDenied(_) => Self {
                status: StatusCode::FORBIDDEN,
                code: "scope_denied",
                message: error.to_string(),
            },
            CoreError::Store(_) => Self::backend_unavailable(),
        }
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::Core(core) => core.into(),
            ServiceError::Invalid(message) => Self::invalid(message),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message,
                request_id: "req_local".to_string(),
                details: json!({}),
            },
        };
        (self.status, Json(body)).into_response()
    }
}
