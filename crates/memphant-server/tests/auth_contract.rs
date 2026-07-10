use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use memphant_core::ApiKeyRow;
use memphant_server::{AppState, api_key_hash};
use memphant_types::{
    ActorId, RecallResponse, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, ScopeId,
    TenantId, TrustLevel,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

const KEY_A: &str = "mk_tenant_a_key";
const KEY_B: &str = "mk_tenant_b_key";
const KEY_REVOKED: &str = "mk_revoked_key";

fn tenant(value: u128) -> TenantId {
    TenantId::from_u128(value)
}

fn scope(value: u128) -> ScopeId {
    ScopeId::from_u128(value)
}

fn actor(value: u128) -> ActorId {
    ActorId::from_u128(value)
}

fn key_row(token: &str, tenant_id: TenantId, max_trust: TrustLevel, revoked: bool) -> ApiKeyRow {
    ApiKeyRow {
        id: Uuid::now_v7(),
        tenant_id,
        key_hash: api_key_hash(token),
        label: token.to_string(),
        max_trust,
        revoked,
    }
}

/// Two tenants, three keys (A, B, revoked-on-A).
fn authed_app(tenant_a: TenantId, tenant_b: TenantId) -> axum::Router {
    let state = AppState::new_in_memory();
    state
        .store()
        .insert_api_key(key_row(KEY_A, tenant_a, TrustLevel::TrustedUser, false));
    state
        .store()
        .insert_api_key(key_row(KEY_B, tenant_b, TrustLevel::TrustedUser, false));
    state.store().insert_api_key(key_row(
        KEY_REVOKED,
        tenant_a,
        TrustLevel::TrustedUser,
        true,
    ));
    memphant_server::app(state)
}

fn retain_body(tenant_id: TenantId, scope_id: ScopeId, actor_id: ActorId) -> Value {
    serde_json::to_value(RetainEpisodeHttpRequest {
        tenant_id,
        scope_id,
        actor_id,
        source_kind: "user".to_string(),
        source_trust: TrustLevel::TrustedUser,
        subject_hint: None,
        subject: Some("auth fact".to_string()),
        predicate: Some("value".to_string()),
        body: Some("Auth contract fact body.".to_string()),
        resource: None,
        unit: None,
        compiler_version: None,
    })
    .expect("serialize")
}

async fn send(
    app: &axum::Router,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let request = if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("body")))
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn missing_key_yields_401_with_error_envelope() {
    let tenant_a = tenant(80_000);
    let app = authed_app(tenant_a, tenant(80_001));

    let (status, body) = send(
        &app,
        "POST",
        "/v1/episodes",
        None,
        Some(retain_body(tenant_a, scope(80_002), actor(80_003))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "auth_required");
    assert!(body["error"]["message"].is_string());
    assert!(body["error"]["request_id"].is_string());
}

#[tokio::test]
async fn invalid_key_yields_401() {
    let tenant_a = tenant(80_100);
    let app = authed_app(tenant_a, tenant(80_101));

    let (status, body) = send(
        &app,
        "POST",
        "/v1/episodes",
        Some("mk_not_a_real_key"),
        Some(retain_body(tenant_a, scope(80_102), actor(80_103))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "auth_required");
}

#[tokio::test]
async fn revoked_key_yields_401() {
    let tenant_a = tenant(80_200);
    let app = authed_app(tenant_a, tenant(80_201));

    let (status, body) = send(
        &app,
        "POST",
        "/v1/episodes",
        Some(KEY_REVOKED),
        Some(retain_body(tenant_a, scope(80_202), actor(80_203))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "auth_required");
}

#[tokio::test]
async fn wrong_tenant_body_yields_403_tenant_mismatch() {
    let tenant_a = tenant(80_300);
    let tenant_b = tenant(80_301);
    let app = authed_app(tenant_a, tenant_b);

    let (status, body) = send(
        &app,
        "POST",
        "/v1/episodes",
        Some(KEY_A),
        Some(retain_body(tenant_b, scope(80_302), actor(80_303))),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "tenant_mismatch");
}

#[tokio::test]
async fn tenant_b_key_cannot_fetch_tenant_a_trace() {
    let tenant_a = tenant(80_400);
    let tenant_b = tenant(80_401);
    let scope_a = scope(80_402);
    let actor_a = actor(80_403);
    let app = authed_app(tenant_a, tenant_b);

    let (status, _) = send(
        &app,
        "POST",
        "/v1/episodes",
        Some(KEY_A),
        Some(retain_body(tenant_a, scope_a, actor_a)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let recall_body = serde_json::json!({
        "tenant_id": tenant_a,
        "scope_id": scope_a,
        "actor_id": actor_a,
        "query": "auth fact"
    });
    let (status, recalled) = send(&app, "POST", "/v1/recall", Some(KEY_A), Some(recall_body)).await;
    assert_eq!(status, StatusCode::OK);
    let recalled: RecallResponse = serde_json::from_value(recalled).expect("recall response");
    let trace_path = format!("/v1/traces/{}", recalled.trace_id.as_uuid());

    let (own_status, _) = send(&app, "GET", &trace_path, Some(KEY_A), None).await;
    assert_eq!(own_status, StatusCode::OK, "owner tenant sees its trace");

    let (cross_status, cross_body) = send(&app, "GET", &trace_path, Some(KEY_B), None).await;
    assert_eq!(cross_status, StatusCode::NOT_FOUND);
    assert_eq!(cross_body["error"]["code"], "not_found");

    // Scope pages are tenant-bound the same way: tenant B sees nothing.
    let scope_path = format!("/v1/scopes/{}/memory", scope_a.as_uuid());
    let (_, cross_scope) = send(&app, "GET", &scope_path, Some(KEY_B), None).await;
    assert_eq!(cross_scope["items"], serde_json::json!([]));
}

#[tokio::test]
async fn declared_trust_is_clamped_to_the_key_max_trust() {
    let tenant_a = tenant(80_500);
    let app = authed_app(tenant_a, tenant(80_501));

    let mut body = retain_body(tenant_a, scope(80_502), actor(80_503));
    body["source_trust"] = Value::String("trusted_system".to_string());
    let (status, retained) = send(&app, "POST", "/v1/episodes", Some(KEY_A), Some(body)).await;
    assert_eq!(status, StatusCode::OK);
    let retained: RetainEpisodeHttpResponse =
        serde_json::from_value(retained).expect("retain response");
    assert_eq!(
        retained.assigned_trust,
        Some(TrustLevel::TrustedUser),
        "trusted_system declaration must clamp to the key's trusted_user ceiling"
    );
}

#[tokio::test]
async fn dev_mode_binds_all_requests_to_the_dev_tenant() {
    let dev_tenant = tenant(80_600);
    let other_tenant = tenant(80_601);
    let scope_id = scope(80_602);
    let actor_id = actor(80_603);
    let state = AppState::new_in_memory().with_dev_tenant(dev_tenant);
    let store = state.store().clone();
    let app = memphant_server::app(state);

    // No auth header, and a body naming a DIFFERENT tenant: dev mode ignores
    // the body tenant and binds to the dev tenant.
    let (status, _) = send(
        &app,
        "POST",
        "/v1/episodes",
        None,
        Some(retain_body(other_tenant, scope_id, actor_id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(store.episodes(dev_tenant).len(), 1);
    assert!(store.episodes(other_tenant).is_empty());
}
