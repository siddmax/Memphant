use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memphant_core::{
    CoreError, InMemoryStore, correct_memory, forget_memory, recall, record_mark, reflect_recorded,
    retain_episode,
};
use memphant_types::{
    COMPILER_VERSION, CorrectRequest, ENGINE_VERSION, ErrorBody, ErrorEnvelope, HealthResponse,
    MarkRequest, RecallHttpRequest, RecallMode, RecallRequest, ReflectCandidate, ReflectInput,
    ReflectRequest, ReflectResult, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse,
    RetainRequest, RetrievalTrace, SCHEMA_COMPAT_REVISION, ScopeMemoryResponse,
    TRACE_SCHEMA_VERSION,
};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    store: InMemoryStore,
}

impl AppState {
    pub fn new_in_memory() -> Self {
        Self {
            store: InMemoryStore::default(),
        }
    }

    pub fn store(&self) -> InMemoryStore {
        self.store.clone()
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/openapi.json", get(openapi))
        .route("/v1/episodes", post(retain_episode_handler))
        .route("/v1/recall", post(recall_handler))
        .route("/v1/reflect", post(reflect_handler))
        .route("/v1/correct", post(correct_handler))
        .route("/v1/forget", post(forget_handler))
        .route("/v1/mark", post(mark_handler))
        .route("/v1/traces/{id}", get(trace_handler))
        .route("/v1/scopes/{id}/memory", get(scope_memory_handler))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        engine_version: ENGINE_VERSION.to_string(),
        trace_schema_version: TRACE_SCHEMA_VERSION.to_string(),
        schema_compat_revision: SCHEMA_COMPAT_REVISION.to_string(),
    })
}

async fn openapi() -> Json<Value> {
    Json(openapi_document())
}

async fn retain_episode_handler(
    State(state): State<AppState>,
    Json(request): Json<RetainEpisodeHttpRequest>,
) -> Result<Json<RetainEpisodeHttpResponse>, ApiError> {
    let retained = retain_episode(
        &state.store,
        RetainRequest {
            tenant_id: request.tenant_id,
            scope_id: request.scope_id,
            actor_id: request.actor_id,
            source_kind: request.source_kind,
            source_trust: request.source_trust,
            subject_hint: request.subject_hint,
            body: request.body,
            compiler_version: request
                .compiler_version
                .unwrap_or_else(|| COMPILER_VERSION.to_string()),
        },
    )
    .await?;

    Ok(Json(RetainEpisodeHttpResponse {
        episode_id: retained.episode_id,
        dedup: retained.dedup,
        enqueued: vec!["reflect_episode".to_string()],
        trace_ref: None,
    }))
}

async fn reflect_handler(
    State(state): State<AppState>,
    Json(request): Json<ReflectRequest>,
) -> Result<Json<ReflectResult>, ApiError> {
    let jobs = state
        .store
        .reflect_jobs(request.tenant_id)
        .into_iter()
        .filter(|job| job.scope_id == request.scope_id)
        .collect::<Vec<_>>();
    let episodes = state.store.episodes(request.tenant_id);
    let compiler_version = request
        .compiler_version
        .unwrap_or_else(|| COMPILER_VERSION.to_string());
    let mut consumed = 0;
    let mut created = 0;
    let mut trace_ref = None;

    for job in jobs {
        let Some(episode_id) = job.episode_id else {
            continue;
        };
        let Some(episode) = episodes.iter().find(|episode| episode.id == episode_id) else {
            continue;
        };
        consumed += 1;
        let trace = reflect_recorded(
            &state.store,
            ReflectInput {
                tenant_id: request.tenant_id,
                scope_id: request.scope_id,
                actor_id: request.actor_id,
                episode_id,
                job_id: job.id,
                compiler_version: compiler_version.clone(),
                candidates: vec![ReflectCandidate {
                    source_kind: episode.source_kind.clone(),
                    trust_level: episode.source_trust,
                    actor_id: episode.actor_id,
                    subject: Some("retained episode".to_string()),
                    predicate: Some("body".to_string()),
                    body: episode.body.clone(),
                    churn_class: None,
                    admission_hint: None,
                }],
            },
        )
        .await?;
        created += trace
            .actions
            .iter()
            .filter(|action| action.creates_unit())
            .count();
        trace_ref = Some(format!("memphant://trace/{}", trace.job_id.as_uuid()));
    }

    Ok(Json(ReflectResult {
        reflect_id: format!("rfl_{}", request.scope_id.as_uuid()),
        episodes_consumed: consumed,
        candidates_created: created,
        trace_ref,
    }))
}

async fn recall_handler(
    State(state): State<AppState>,
    Json(request): Json<RecallHttpRequest>,
) -> Result<Json<memphant_types::RecallResponse>, ApiError> {
    let response = recall(
        &state.store,
        RecallRequest {
            tenant_id: request.tenant_id,
            scope_id: request.scope_id,
            actor_id: request.actor_id,
            allowed_scope_ids: request
                .allowed_scope_ids
                .unwrap_or_else(|| vec![request.scope_id]),
            query: request.query,
            k: request.limit.unwrap_or(8),
            budget_tokens: request.budget_tokens.unwrap_or(512),
            mode: request.mode.unwrap_or(RecallMode::Fast),
            include_beliefs: request.include_beliefs.unwrap_or(false),
            engine_version: ENGINE_VERSION.to_string(),
        },
    )
    .await?;
    Ok(Json(response))
}

async fn correct_handler(
    State(state): State<AppState>,
    Json(request): Json<CorrectRequest>,
) -> Result<Json<memphant_types::CorrectResult>, ApiError> {
    Ok(Json(correct_memory(&state.store, request).await?))
}

async fn forget_handler(
    State(state): State<AppState>,
    Json(request): Json<memphant_types::ForgetRequest>,
) -> Result<Json<memphant_types::ForgetResult>, ApiError> {
    Ok(Json(forget_memory(&state.store, request).await?))
}

async fn mark_handler(
    State(state): State<AppState>,
    Json(request): Json<MarkRequest>,
) -> Result<Json<memphant_types::MarkResult>, ApiError> {
    Ok(Json(record_mark(&state.store, request).await?))
}

async fn trace_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RetrievalTrace>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::invalid("invalid trace id"))?;
    let trace_id = memphant_types::TraceId::from_u128(uuid.as_u128());
    state
        .store
        .trace_by_id(trace_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("trace"))
}

#[derive(Debug, Deserialize)]
struct ScopeMemoryQuery {
    tenant_id: memphant_types::TenantId,
}

async fn scope_memory_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ScopeMemoryQuery>,
) -> Result<Json<ScopeMemoryResponse>, ApiError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| ApiError::invalid("invalid scope id"))?;
    let scope_id = memphant_types::ScopeId::from_u128(uuid.as_u128());
    Ok(Json(ScopeMemoryResponse {
        tenant_id: query.tenant_id,
        scope_id,
        items: state.store.scope_memory(query.tenant_id, scope_id),
        next_cursor: None,
        has_more: false,
    }))
}

pub fn openapi_document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "MemPhant API",
            "version": ENGINE_VERSION
        },
        "paths": {
            "/v1/episodes": path_item("post", "RetainEpisodeHttpRequest", "RetainEpisodeHttpResponse"),
            "/v1/memory": path_item("post", "RetainEpisodeHttpRequest", "RetainEpisodeHttpResponse"),
            "/v1/recall": path_item("post", "RecallHttpRequest", "RecallResponse"),
            "/v1/reflect": path_item("post", "ReflectRequest", "ReflectResult"),
            "/v1/correct": path_item("post", "CorrectRequest", "CorrectResult"),
            "/v1/forget": path_item("post", "ForgetRequest", "ForgetResult"),
            "/v1/mark": path_item("post", "MarkRequest", "MarkResult"),
            "/v1/traces/{id}": path_item("get", "TraceId", "RetrievalTrace"),
            "/v1/scopes/{id}/memory": path_item("get", "ScopeId", "ScopeMemoryResponse"),
            "/v1/scopes/{id}/stats": path_item("get", "ScopeId", "HealthResponse"),
            "/v1/scopes/{id}/block": path_item("get", "ScopeId", "HealthResponse"),
            "/v1/health": path_item("get", "HealthResponse", "HealthResponse")
        },
        "components": {
            "schemas": {
                "RetainEpisodeHttpRequest": schema::<RetainEpisodeHttpRequest>(),
                "RetainEpisodeHttpResponse": schema::<RetainEpisodeHttpResponse>(),
                "RecallHttpRequest": schema::<RecallHttpRequest>(),
                "RecallResponse": schema::<memphant_types::RecallResponse>(),
                "ReflectRequest": schema::<ReflectRequest>(),
                "ReflectResult": schema::<ReflectResult>(),
                "CorrectRequest": schema::<CorrectRequest>(),
                "CorrectResult": schema::<memphant_types::CorrectResult>(),
                "ForgetRequest": schema::<memphant_types::ForgetRequest>(),
                "ForgetResult": schema::<memphant_types::ForgetResult>(),
                "MarkRequest": schema::<MarkRequest>(),
                "MarkResult": schema::<memphant_types::MarkResult>(),
                "RetrievalTrace": schema::<RetrievalTrace>(),
                "ScopeMemoryResponse": schema::<ScopeMemoryResponse>(),
                "ErrorEnvelope": schema::<ErrorEnvelope>()
            }
        }
    })
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("schema serializes")
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

struct ApiError {
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
            CoreError::Store(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "backend_unavailable",
                message: "backend unavailable".to_string(),
            },
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

trait AdmissionActionExt {
    fn creates_unit(&self) -> bool;
}

impl AdmissionActionExt for memphant_types::AdmissionAction {
    fn creates_unit(&self) -> bool {
        matches!(
            self,
            Self::Append | Self::Supersede | Self::Quarantine | Self::Invalidate
        )
    }
}
