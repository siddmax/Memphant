//! Contract for the MCP edge hardening: tool errors must not leak raw backend
//! detail (mirroring the REST edge), and the streamable-HTTP transport must
//! require a matching bearer token outside dev mode.

use memphant_core::service::ServiceError;
use memphant_core::{CoreError, StoreError};
use memphant_mcp::{constant_time_eq, mcp_error, mcp_http_authorized};

#[test]
fn mcp_error_hides_backend_detail_but_surfaces_caller_errors() {
    // Backend/store errors collapse to a generic message — no raw SQL leaks.
    let leaked = mcp_error(ServiceError::Core(CoreError::Store(StoreError::Backend(
        "relation memphant.SECRET does not exist".to_string(),
    ))));
    assert_eq!(leaked, "backend unavailable");
    assert!(!leaked.contains("SECRET"));

    // Caller-relevant errors keep their (safe) messages.
    assert!(
        mcp_error(ServiceError::Invalid("missing field".to_string())).contains("missing field")
    );
    assert!(
        mcp_error(ServiceError::Core(CoreError::Invalid(
            "bad shape".to_string()
        )))
        .contains("bad shape")
    );
    assert!(
        mcp_error(ServiceError::Core(CoreError::NotFound(
            "memory_unit".to_string()
        )))
        .contains("memory_unit")
    );
}

#[test]
fn deep_provider_errors_have_stable_safe_mcp_codes() {
    assert_eq!(
        mcp_error(ServiceError::Core(CoreError::DeepUnavailable)),
        "deep_unavailable: deep recall is unavailable"
    );
    assert_eq!(
        mcp_error(ServiceError::Core(CoreError::DeepProviderInvalidOutput)),
        "deep_provider_invalid_output: deep recall provider returned invalid output"
    );
}

#[test]
fn http_auth_requires_matching_bearer_outside_dev() {
    // Dev mode: auth explicitly disabled — everything allowed.
    assert!(mcp_http_authorized(true, None, None));

    // Key mode: only a correct bearer token is allowed.
    assert!(mcp_http_authorized(
        false,
        Some("mk_secret"),
        Some("Bearer mk_secret")
    ));
    assert!(mcp_http_authorized(
        false,
        Some("mk_secret"),
        Some("bearer mk_secret")
    ));
    assert!(!mcp_http_authorized(false, Some("mk_secret"), None));
    assert!(!mcp_http_authorized(
        false,
        Some("mk_secret"),
        Some("Bearer mk_wrong")
    ));
    assert!(!mcp_http_authorized(
        false,
        Some("mk_secret"),
        Some("Basic mk_secret")
    ));
}

#[test]
fn constant_time_eq_matches_string_equality() {
    assert!(constant_time_eq("abc", "abc"));
    assert!(constant_time_eq("", ""));
    assert!(!constant_time_eq("abc", "abd"));
    assert!(!constant_time_eq("abc", "abcd"));
}
