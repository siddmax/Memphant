use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use memphant_core::{
    CoreError, InMemoryStore, correct_memory, forget_memory, recall, record_mark, reflect_recorded,
    retain_episode,
};
use memphant_types::{
    AdmissionAction, COMPILER_VERSION, CorrectRequest, CorrectResult, ENGINE_VERSION,
    ForgetRequest, ForgetResult, MarkRequest, MarkResult, McpToolAnnotations, McpToolSpec,
    RecallHttpRequest, RecallMode, RecallRequest, RecallResponse, ReflectCandidate, ReflectInput,
    ReflectRequest, ReflectResult, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse,
    RetainRequest, RetrievalTrace, TraceRequest,
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn tool_specs() -> Vec<McpToolSpec> {
    vec![
        tool::<RetainEpisodeHttpRequest, RetainEpisodeHttpResponse>(
            "retain",
            "Store memory or a raw episode.",
            McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: false,
            },
        ),
        tool::<RecallHttpRequest, RecallResponse>(
            "recall",
            "Retrieve cited memory evidence.",
            McpToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                idempotent_hint: false,
                open_world_hint: false,
            },
        ),
        tool::<ReflectRequest, ReflectResult>(
            "reflect",
            "Request consolidation for a scope.",
            McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: false,
            },
        ),
        tool::<CorrectRequest, CorrectResult>(
            "correct",
            "Supersede or invalidate selected memory through an auditable correction.",
            McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: false,
            },
        ),
        tool::<ForgetRequest, ForgetResult>(
            "forget",
            "Forget by ID, scope, kind, or policy selector.",
            McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: true,
                idempotent_hint: true,
                open_world_hint: false,
            },
        ),
        tool::<TraceRequest, RetrievalTrace>(
            "trace",
            "Inspect a retrieval trace.",
            McpToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                idempotent_hint: false,
                open_world_hint: false,
            },
        ),
        tool::<MarkRequest, MarkResult>(
            "mark",
            "Report what the caller did with a recall pack.",
            McpToolAnnotations {
                read_only_hint: false,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: false,
            },
        ),
    ]
}

pub fn tool_specs_json() -> Value {
    serde_json::to_value(tool_specs()).expect("MCP tool specs serialize")
}

#[derive(Clone, Default)]
pub struct McpRuntime {
    store: InMemoryStore,
}

impl McpRuntime {
    pub fn new_in_memory() -> Self {
        Self {
            store: InMemoryStore::default(),
        }
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<McpToolCallResult, McpRuntimeError> {
        match name {
            "retain" => {
                let request: RetainEpisodeHttpRequest = serde_json::from_value(arguments)?;
                let retained = retain_episode(
                    &self.store,
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
                ok(
                    "Retained episode and enqueued reflection.",
                    RetainEpisodeHttpResponse {
                        episode_id: retained.episode_id,
                        dedup: retained.dedup,
                        enqueued: vec!["reflect_episode".to_string()],
                        trace_ref: None,
                    },
                )
            }
            "reflect" => {
                let request: ReflectRequest = serde_json::from_value(arguments)?;
                let reflected = self.reflect(request).await?;
                ok("Reflected pending episodes for scope.", reflected)
            }
            "recall" => {
                let request: RecallHttpRequest = serde_json::from_value(arguments)?;
                let recalled = recall(
                    &self.store,
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
                ok("Returned cited memory evidence.", recalled)
            }
            "correct" => {
                let request: CorrectRequest = serde_json::from_value(arguments)?;
                ok(
                    "Corrected selected memory.",
                    correct_memory(&self.store, request).await?,
                )
            }
            "forget" => {
                let request: ForgetRequest = serde_json::from_value(arguments)?;
                ok(
                    "Forgot selected memory.",
                    forget_memory(&self.store, request).await?,
                )
            }
            "trace" => {
                let request: TraceRequest = serde_json::from_value(arguments)?;
                let trace = self
                    .store
                    .trace_by_id(request.trace_id)
                    .ok_or_else(|| McpRuntimeError::Tool("trace not found".to_string()))?;
                ok("Returned retrieval trace.", trace)
            }
            "mark" => {
                let request: MarkRequest = serde_json::from_value(arguments)?;
                ok(
                    "Recorded recall outcome feedback.",
                    record_mark(&self.store, request).await?,
                )
            }
            other => Err(McpRuntimeError::Tool(format!("unknown tool: {other}"))),
        }
    }

    async fn reflect(&self, request: ReflectRequest) -> Result<ReflectResult, CoreError> {
        let jobs = self
            .store
            .reflect_jobs(request.tenant_id)
            .into_iter()
            .filter(|job| job.scope_id == request.scope_id)
            .collect::<Vec<_>>();
        let episodes = self.store.episodes(request.tenant_id);
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
                &self.store,
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
                        contextual_chunks: Vec::new(),
                        valid_from: None,
                        valid_to: None,
                    }],
                },
            )
            .await?;
            created += trace
                .actions
                .iter()
                .filter(|action| {
                    matches!(
                        action,
                        AdmissionAction::Append
                            | AdmissionAction::Supersede
                            | AdmissionAction::Quarantine
                            | AdmissionAction::Invalidate
                    )
                })
                .count();
            trace_ref = Some(format!("memphant://trace/{}", trace.job_id.as_uuid()));
        }

        Ok(ReflectResult {
            reflect_id: format!("rfl_{}", request.scope_id.as_uuid()),
            episodes_consumed: consumed,
            candidates_created: created,
            trace_ref,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallResult {
    pub content: Vec<McpContent>,
    pub structured_content: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

pub async fn handle_json_rpc_value(runtime: &McpRuntime, value: Value) -> Value {
    match serde_json::from_value::<JsonRpcRequest>(value) {
        Ok(request) => handle_json_rpc(runtime, request).await,
        Err(error) => json_rpc_error(Value::Null, -32600, format!("invalid request: {error}")),
    }
}

pub async fn handle_json_rpc(runtime: &McpRuntime, request: JsonRpcRequest) -> Value {
    let id = request.id.clone();
    match request.method.as_str() {
        "initialize" => json_rpc_result(
            id,
            json!({
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "memphant",
                    "version": ENGINE_VERSION
                },
                "capabilities": {
                    "tools": {}
                }
            }),
        ),
        "tools/list" => json_rpc_result(id, json!({ "tools": tool_specs() })),
        "tools/call" => {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match runtime.call_tool(name, arguments).await {
                Ok(result) => json_rpc_result(id, serde_json::to_value(result).expect("result")),
                Err(error) => json_rpc_error(id, -32000, error.to_string()),
            }
        }
        other => json_rpc_error(id, -32601, format!("unknown method: {other}")),
    }
}

pub fn streamable_http_app(runtime: McpRuntime) -> Router {
    Router::new()
        .route("/mcp", post(mcp_post))
        .with_state(runtime)
}

async fn mcp_post(State(runtime): State<McpRuntime>, Json(value): Json<Value>) -> Json<Value> {
    Json(handle_json_rpc_value(&runtime, value).await)
}

fn json_rpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn json_rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[derive(Debug, thiserror::Error)]
pub enum McpRuntimeError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("{0}")]
    Tool(String),
}

fn ok<T: Serialize>(
    summary: impl Into<String>,
    value: T,
) -> Result<McpToolCallResult, McpRuntimeError> {
    Ok(McpToolCallResult {
        content: vec![McpContent {
            r#type: "text".to_string(),
            text: summary.into(),
        }],
        structured_content: serde_json::to_value(value)?,
        is_error: false,
    })
}

fn tool<Input, Output>(
    name: &str,
    description: &str,
    annotations: McpToolAnnotations,
) -> McpToolSpec
where
    Input: JsonSchema,
    Output: JsonSchema,
{
    McpToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema::<Input>(),
        output_schema: schema::<Output>(),
        annotations,
    }
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("schema serializes")
}
