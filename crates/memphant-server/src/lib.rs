use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use memphant_core::{
    CoreError, InMemoryStore, SystemClock, correct_memory, forget_memory, recall, record_mark,
    reflect_recorded, retain_episode,
};
use memphant_types::{
    COMPILER_VERSION, CorrectRequest, ENGINE_VERSION, ErrorBody, ErrorEnvelope, HealthResponse,
    MarkRequest, RecallHttpRequest, RecallMode, RecallRequest, ReflectCandidate, ReflectInput,
    ReflectRequest, ReflectResult, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse,
    RetainRequest, RetrievalTrace, SCHEMA_COMPAT_REVISION, ScopeMemoryResponse,
    TRACE_SCHEMA_VERSION,
};
use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde::Deserialize;
use serde_json::{Value, json};
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
        .route(HEALTH_PATH, get(health))
        .route(OPENAPI_PATH, get(openapi))
        .route(EPISODES_PATH, post(retain_episode_handler))
        .route(RECALL_PATH, post(recall_handler))
        .route(REFLECT_PATH, post(reflect_handler))
        .route(CORRECT_PATH, post(correct_handler))
        .route(FORGET_PATH, post(forget_handler))
        .route(MARK_PATH, post(mark_handler))
        .route(TRACE_PATH, get(trace_handler))
        .route(SCOPE_MEMORY_PATH, get(scope_memory_handler))
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
            subject: request.subject,
            predicate: request.predicate,
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
                    subject: job.subject.clone(),
                    predicate: job.predicate.clone(),
                    body: episode.body.clone(),
                    churn_class: None,
                    admission_hint: None,
                    contextual_chunks: Vec::new(),
                    valid_from: None,
                    valid_to: None,
                }],
            },
            &SystemClock,
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
            edge_expansion_enabled: request.edge_expansion_enabled.unwrap_or(true),
            context_packing_abstention_enabled: request
                .context_packing_abstention_enabled
                .unwrap_or(true),
            rerank_enabled: request.rerank_enabled.unwrap_or(true),
            learned_rerank_profile: None,
            query_decomposition_enabled: request.query_decomposition_enabled.unwrap_or(true),
            procedure_recall_enabled: request.procedure_recall_enabled.unwrap_or(true),
            decay_enabled: request.decay_enabled.unwrap_or(true),
            engine_version: ENGINE_VERSION.to_string(),
        },
        &SystemClock,
    )
    .await?;
    Ok(Json(response))
}

async fn correct_handler(
    State(state): State<AppState>,
    Json(request): Json<CorrectRequest>,
) -> Result<Json<memphant_types::CorrectResult>, ApiError> {
    Ok(Json(
        correct_memory(&state.store, request, &SystemClock).await?,
    ))
}

async fn forget_handler(
    State(state): State<AppState>,
    Json(request): Json<memphant_types::ForgetRequest>,
) -> Result<Json<memphant_types::ForgetResult>, ApiError> {
    Ok(Json(
        forget_memory(&state.store, request, &SystemClock).await?,
    ))
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
        .trace_by_id_any_tenant(trace_id)
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
        "paths": openapi_paths(),
        "components": {
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
            vec![path_param("id"), query_param("tenant_id")],
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

fn query_param(name: &str) -> Value {
    json!({
        "name": name,
        "in": "query",
        "required": true,
        "schema": { "type": "string", "format": "uuid" }
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
