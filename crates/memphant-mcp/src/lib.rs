use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use std::sync::Arc;

use memphant_core::service::{MemoryService, ServiceError};
use memphant_core::{CoreError, InMemoryStore, NoopEmbedding, SystemClock};
use memphant_types::{
    CorrectRequest, CorrectResult, ENGINE_VERSION, ForgetRequest, ForgetResult, MarkRequest,
    MarkResult, McpToolAnnotations, McpToolSpec, RecallHttpRequest, RecallResponse, ReflectRequest,
    ReflectResult, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, RetrievalTrace,
    TraceRequest,
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

#[derive(Clone)]
pub struct McpRuntime {
    service: MemoryService<InMemoryStore>,
}

impl Default for McpRuntime {
    fn default() -> Self {
        Self::new_in_memory()
    }
}

impl McpRuntime {
    pub fn new_in_memory() -> Self {
        Self {
            service: MemoryService::new(
                Arc::new(InMemoryStore::default()),
                Arc::new(SystemClock),
                Arc::new(NoopEmbedding),
            ),
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
                let tenant = request.tenant_id;
                ok(
                    "Retained memory and enqueued reflection.",
                    self.service.retain(tenant, request).await?,
                )
            }
            "reflect" => {
                let request: ReflectRequest = serde_json::from_value(arguments)?;
                ok(
                    "Reflected pending episodes for scope.",
                    self.service
                        .reflect(
                            request.tenant_id,
                            request.scope_id,
                            request.compiler_version,
                        )
                        .await?,
                )
            }
            "recall" => {
                let request: RecallHttpRequest = serde_json::from_value(arguments)?;
                let tenant = request.tenant_id;
                ok(
                    "Returned cited memory evidence.",
                    self.service.recall(tenant, request).await?,
                )
            }
            "correct" => {
                let request: CorrectRequest = serde_json::from_value(arguments)?;
                let tenant = request.tenant_id;
                ok(
                    "Corrected selected memory.",
                    self.service.correct(tenant, request).await?,
                )
            }
            "forget" => {
                let request: ForgetRequest = serde_json::from_value(arguments)?;
                let tenant = request.tenant_id;
                ok(
                    "Forgot selected memory.",
                    self.service.forget(tenant, request).await?,
                )
            }
            "trace" => {
                let request: TraceRequest = serde_json::from_value(arguments)?;
                let trace = self
                    .service
                    .trace(request.tenant_id, request.trace_id)
                    .await?
                    .ok_or_else(|| McpRuntimeError::Tool("trace not found".to_string()))?;
                ok("Returned retrieval trace.", trace)
            }
            "mark" => {
                let request: MarkRequest = serde_json::from_value(arguments)?;
                let tenant = request.tenant_id;
                ok(
                    "Recorded recall outcome feedback.",
                    self.service.mark(tenant, request).await?,
                )
            }
            other => Err(McpRuntimeError::Tool(format!("unknown tool: {other}"))),
        }
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
    #[error(transparent)]
    Service(#[from] ServiceError),
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
