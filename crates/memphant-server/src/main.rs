use std::net::SocketAddr;

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

    let addr: SocketAddr = "127.0.0.1:3000".parse().expect("valid bind address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind memphant-server");
    axum::serve(
        listener,
        memphant_server::app(memphant_server::AppState::new_in_memory()),
    )
    .await
    .expect("serve memphant-server");
}
