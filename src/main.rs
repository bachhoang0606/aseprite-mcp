//! Aseprite MCP server entry point.
//!
//! Brings up stderr logging (stdout is reserved for the MCP JSON-RPC stream),
//! constructs the server, and serves it over the stdio transport until the peer
//! disconnects.

mod ascii_view;
mod aseprite;
mod autotile;
mod color_ops;
mod filmstrip;
mod gutter;
mod live;
mod marks;
mod preview;
mod reference;
mod server;
mod tileset_export;
mod tools;
mod utils;

use anyhow::Result;
use rmcp::ServiceExt;
use server::AsepriteServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // stdout carries the JSON-RPC protocol, so every log line goes to stderr.
    // Honour RUST_LOG when set, otherwise default to `info`.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("aseprite_mcp v{} starting", env!("CARGO_PKG_VERSION"));

    // Constructing the server also resolves the Aseprite executable up front.
    let server = AsepriteServer::new()?;
    let service = server.serve(rmcp::transport::io::stdio()).await?;
    tracing::info!("aseprite_mcp ready — awaiting requests");

    service.waiting().await?;
    tracing::info!("aseprite_mcp shut down");
    Ok(())
}
