use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use memphant_types::{
    ActorId, CorrectRequest, CorrectSelector, CorrectionPayload, ForgetRequest, ForgetSelector,
    HealthResponse, MarkOutcome, MarkRequest, RecallHttpRequest, RecallResponse, ReflectRequest,
    RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, ScopeId, TenantId, TrustLevel,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tower::ServiceExt;

fn tenant(value: u128) -> TenantId {
    TenantId::from_u128(value)
}

fn scope(value: u128) -> ScopeId {
    ScopeId::from_u128(value)
}

fn actor(value: u128) -> ActorId {
    ActorId::from_u128(value)
}

#[tokio::test]
async fn rest_examples_round_trip_through_retain_reflect_recall_trace_and_mutations() {
    let state = memphant_server::AppState::new_in_memory();
    let app = memphant_server::app(state);
    let tenant_id = tenant(90_000);
    let scope_id = scope(90_001);
    let actor_id = actor(90_002);

    let health: HealthResponse = json_request(&app, "GET", "/v1/health", None::<()>).await.1;
    assert_eq!(health.status, "ok");

    let retained: RetainEpisodeHttpResponse = json_request(
        &app,
        "POST",
        "/v1/episodes",
        Some(RetainEpisodeHttpRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "system".to_string(),
            source_trust: TrustLevel::TrustedSystem,
            subject_hint: Some("release region".to_string()),
            body: "Release region is Taipei.".to_string(),
            compiler_version: None,
        }),
    )
    .await
    .1;
    assert_eq!(retained.enqueued, vec!["reflect_episode"]);

    let reflected: Value = json_request(
        &app,
        "POST",
        "/v1/reflect",
        Some(ReflectRequest {
            tenant_id,
            scope_id,
            actor_id,
            compiler_version: None,
        }),
    )
    .await
    .1;
    assert_eq!(reflected["episodes_consumed"], 1);

    let recalled: RecallResponse = json_request(
        &app,
        "POST",
        "/v1/recall",
        Some(RecallHttpRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: Some(vec![scope_id]),
            query: "Where is the release region?".to_string(),
            limit: Some(4),
            budget_tokens: Some(80),
            mode: None,
            include_beliefs: None,
            include_trace: Some(true),
        }),
    )
    .await
    .1;
    assert_eq!(recalled.items[0].body, "Release region is Taipei.");

    let trace_path = format!("/v1/traces/{}", recalled.trace_id.as_uuid());
    let trace: Value = json_request(&app, "GET", &trace_path, None::<()>).await.1;
    assert_eq!(trace["id"], recalled.trace_id.as_uuid().to_string());

    let corrected: Value = json_request(
        &app,
        "POST",
        "/v1/correct",
        Some(CorrectRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: CorrectSelector {
                memory_unit_id: recalled.items[0].unit_id,
            },
            correction: CorrectionPayload {
                value: "Release region is Singapore.".to_string(),
                reason: "stale_fact".to_string(),
                valid_from: None,
                valid_to: None,
            },
        }),
    )
    .await
    .1;
    assert_eq!(
        corrected["superseded"][0],
        recalled.items[0].unit_id.as_uuid().to_string()
    );

    let forgotten: Value = json_request(
        &app,
        "POST",
        "/v1/forget",
        Some(ForgetRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: ForgetSelector {
                memory_unit_id: Some(recalled.items[0].unit_id),
                scope_id: None,
            },
            reason: "user_request".to_string(),
        }),
    )
    .await
    .1;
    assert_eq!(
        forgotten["verification"],
        "no_recall_path_returns_forgotten"
    );

    let marked: Value = json_request(
        &app,
        "POST",
        "/v1/mark",
        Some(MarkRequest {
            tenant_id,
            trace_id: recalled.trace_id,
            caller_id: "rest-contract".to_string(),
            used_ids: vec![recalled.items[0].unit_id],
            outcome: MarkOutcome::Success,
        }),
    )
    .await
    .1;
    assert_eq!(marked["accepted"], true);
}

#[test]
fn openapi_document_contains_wsd_paths_and_component_schemas() {
    let document = memphant_server::openapi_document();
    for path in [
        "/v1/episodes",
        "/v1/recall",
        "/v1/reflect",
        "/v1/correct",
        "/v1/forget",
        "/v1/mark",
        "/v1/traces/{id}",
        "/v1/scopes/{id}/memory",
        "/v1/health",
    ] {
        assert!(document["paths"].get(path).is_some(), "missing {path}");
    }
    for schema in [
        "RetainEpisodeHttpRequest",
        "RecallHttpRequest",
        "RecallResponse",
        "CorrectRequest",
        "ForgetRequest",
        "MarkRequest",
    ] {
        assert!(
            document["components"]["schemas"].get(schema).is_some(),
            "missing schema {schema}"
        );
    }
}

async fn json_request<T, R>(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<T>,
) -> (StatusCode, R)
where
    T: Serialize,
    R: DeserializeOwned,
{
    let mut builder = Request::builder().method(method).uri(path);
    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(
                serde_json::to_vec(&body).expect("serialize body"),
            ))
            .expect("request")
    } else {
        builder.body(Body::empty()).expect("request")
    };
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert!(
        status.is_success(),
        "status={status} body={}",
        String::from_utf8_lossy(&bytes)
    );
    (
        status,
        serde_json::from_slice(&bytes).expect("deserialize response"),
    )
}
