use std::net::SocketAddr;

use memphant_types::TenantId;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--openapi-json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&memphant_server::openapi_document())
                .expect("OpenAPI serializes")
        );
        return;
    }

    let store = memphant_runtime::build_store()
        .await
        .expect("memphant-server: store construction failed");
    let store_name = store.name();
    eprintln!("memphant-server: store={store_name}");
    let service = memphant_runtime::build_service(store);
    let mut state = memphant_server::AppState::from_service(service, store_name);
    if let Ok(dev_tenant) = std::env::var("MEMPHANT_DEV_TENANT") {
        let uuid = Uuid::parse_str(&dev_tenant).expect("MEMPHANT_DEV_TENANT must be a UUID");
        state = state.with_dev_tenant(TenantId::from_u128(uuid.as_u128()));
    }

    let bind = std::env::var("MEMPHANT_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let addr: SocketAddr = bind.parse().expect("valid bind address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind memphant-server");
    axum::serve(listener, memphant_server::app(state))
        .await
        .expect("serve memphant-server");
}
