use std::process::ExitCode;
use std::sync::Arc;

use memphant_mcp::MemphantMcp;
use rmcp::ServiceExt;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--list-tools-json") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&memphant_mcp::tools_artifact())
                    .expect("MCP tools serialize")
            );
            ExitCode::SUCCESS
        }
        Some("stdio") | None => run_stdio().await,
        Some("streamable-http") => run_streamable_http().await,
        Some(_) => {
            eprintln!("usage: memphant-mcp [--list-tools-json|stdio|streamable-http]");
            ExitCode::from(2)
        }
    }
}

/// Builds the store, resolves the fixed tenant (refusing to start without a
/// valid key or dev tenant) and returns the tool handler.
async fn build_handler() -> Result<MemphantMcp, String> {
    let store = memphant_runtime::build_store()
        .await
        .map_err(|error| error.to_string())?;
    let bound = memphant_mcp::resolve_tenant(&store).await?;
    let service = memphant_runtime::build_service(store);
    Ok(MemphantMcp::new(service, bound))
}

/// Persistent stdio session: serves JSON-RPC over stdin/stdout until the
/// client disconnects.
async fn run_stdio() -> ExitCode {
    let handler = match build_handler().await {
        Ok(handler) => handler,
        Err(error) => {
            eprintln!("memphant-mcp: {error}");
            return ExitCode::from(1);
        }
    };
    let running = match handler.serve(rmcp::transport::io::stdio()).await {
        Ok(running) => running,
        Err(error) => {
            eprintln!("memphant-mcp: {error}");
            return ExitCode::from(1);
        }
    };
    match running.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("memphant-mcp: {error}");
            ExitCode::from(1)
        }
    }
}

/// Streamable-HTTP transport (MCP 2025-11-25) on `MEMPHANT_MCP_BIND`
/// (default 127.0.0.1:3333), path `/mcp`.
async fn run_streamable_http() -> ExitCode {
    let handler = match build_handler().await {
        Ok(handler) => handler,
        Err(error) => {
            eprintln!("memphant-mcp: {error}");
            return ExitCode::from(1);
        }
    };
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let bind = std::env::var("MEMPHANT_MCP_BIND").unwrap_or_else(|_| "127.0.0.1:3333".to_string());
    match tokio::net::TcpListener::bind(&bind).await {
        Ok(listener) => {
            eprintln!("memphant-mcp: streamable-http on http://{bind}/mcp");
            if let Err(error) = axum::serve(listener, router).await {
                eprintln!("memphant-mcp: {error}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("memphant-mcp: {error}");
            ExitCode::from(1)
        }
    }
}
