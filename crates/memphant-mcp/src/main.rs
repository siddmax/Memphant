use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--list-tools-json") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&memphant_mcp::tool_specs_json())
                    .expect("MCP specs serialize")
            );
            ExitCode::SUCCESS
        }
        Some("stdio") | None => run_stdio_once().await,
        Some("streamable-http") => run_streamable_http().await,
        Some(_) => {
            eprintln!("usage: memphant-mcp [--list-tools-json|stdio|streamable-http]");
            ExitCode::from(2)
        }
    }
}

async fn run_stdio_once() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    if input.trim().is_empty() {
        eprintln!("expected one JSON-RPC request on stdin");
        return ExitCode::from(2);
    }
    let value = match serde_json::from_str(&input) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let response =
        memphant_mcp::handle_json_rpc_value(&memphant_mcp::McpRuntime::new_in_memory(), value)
            .await;
    println!(
        "{}",
        serde_json::to_string(&response).expect("JSON-RPC response serializes")
    );
    ExitCode::SUCCESS
}

async fn run_streamable_http() -> ExitCode {
    let bind = std::env::var("MEMPHANT_MCP_BIND").unwrap_or_else(|_| "127.0.0.1:3333".to_string());
    match tokio::net::TcpListener::bind(&bind).await {
        Ok(listener) => {
            if let Err(error) = axum::serve(
                listener,
                memphant_mcp::streamable_http_app(memphant_mcp::McpRuntime::new_in_memory()),
            )
            .await
            {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
