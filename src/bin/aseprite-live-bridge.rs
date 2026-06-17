//! Standalone Aseprite live WebSocket bridge (SPEC-001 / ADR-0002).
//!
//! A long-lived singleton process that owns the plugin port (default 9876) and a
//! control port (default 9877). The Aseprite Lua plugin connects to the plugin
//! port exactly as before; MCP server processes connect to the control port as
//! clients. The bridge is a *dumb relay*: it forwards frames between the single
//! plugin and any number of control clients, routing responses by id-namespacing.
//!
//! Because this process is decoupled from any MCP server's lifecycle, restarting
//! or duplicating the MCP server no longer drops the plugin connection — which is
//! the whole point (kills the live-bridge churn).

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const DEFAULT_PLUGIN_PORT: u16 = 9876;
/// Separator used to namespace request ids per control client so plugin
/// responses can be routed back to the originating MCP process.
const ID_SEP: &str = "@@";

type Tx = mpsc::UnboundedSender<Message>;

/// Bind address for a bridge port. Always the IPv4 loopback so the bridge is
/// **never** reachable off-host by default (checklist 10.2): the plugin and the
/// MCP control clients are local-only. Centralised so the loopback guarantee is
/// regression-tested rather than re-typed at each bind site.
fn loopback_addr(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

#[derive(Default)]
struct Bridge {
    /// Sender to the currently-connected plugin (if any).
    plugin_tx: RwLock<Option<Tx>>,
    /// Last `hello` payload from the plugin, surfaced to clients via bridge_state.
    last_hello: RwLock<Option<Value>>,
    /// Connected control clients (MCP processes) by connection id.
    clients: RwLock<HashMap<u64, Tx>>,
    next_client_id: AtomicU64,
}

impl Bridge {
    fn plugin_port() -> u16 {
        std::env::var("ASEPRITE_MCP_LIVE_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_PLUGIN_PORT)
    }

    fn control_port() -> u16 {
        std::env::var("ASEPRITE_MCP_LIVE_CONTROL_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| Self::plugin_port().wrapping_add(1))
    }

    async fn state_frame(&self) -> Message {
        let connected = self.plugin_tx.read().await.is_some();
        let last_hello = self.last_hello.read().await.clone();
        Message::Text(
            json!({
                "type": "bridge_state",
                "pluginConnected": connected,
                "lastHello": last_hello,
            })
            .to_string()
            .into(),
        )
    }

    async fn broadcast_state(&self) {
        let frame = self.state_frame().await;
        let clients = self.clients.read().await;
        for tx in clients.values() {
            let _ = tx.send(frame.clone());
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let plugin_addr = loopback_addr(Bridge::plugin_port());
    let control_addr = loopback_addr(Bridge::control_port());

    // Singleton via port ownership: if either bind fails, another bridge already
    // owns the ports — exit cleanly so the racing/duplicate instance just quits.
    let plugin_listener = match TcpListener::bind(&plugin_addr).await {
        Ok(l) => l,
        Err(err) => {
            info!(
                "another bridge already owns {} ({}); exiting",
                plugin_addr, err
            );
            return Ok(());
        }
    };
    let control_listener = match TcpListener::bind(&control_addr).await {
        Ok(l) => l,
        Err(err) => {
            info!(
                "another bridge already owns {} ({}); exiting",
                control_addr, err
            );
            return Ok(());
        }
    };

    info!(
        "Aseprite live bridge: plugin ws://{} | control ws://{}",
        plugin_addr, control_addr
    );

    let bridge = Arc::new(Bridge::default());

    let plugin_bridge = bridge.clone();
    let plugin_loop = tokio::spawn(async move {
        loop {
            match plugin_listener.accept().await {
                Ok((stream, peer)) => {
                    info!("plugin connected from {}", peer);
                    let b = plugin_bridge.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_plugin(b, stream).await {
                            warn!("plugin connection ended: {}", err);
                        }
                    });
                }
                Err(err) => {
                    warn!("plugin accept error: {}", err);
                }
            }
        }
    });

    let control_bridge = bridge.clone();
    let control_loop = tokio::spawn(async move {
        loop {
            match control_listener.accept().await {
                Ok((stream, _peer)) => {
                    let b = control_bridge.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_client(b, stream).await {
                            warn!("control client ended: {}", err);
                        }
                    });
                }
                Err(err) => {
                    warn!("control accept error: {}", err);
                }
            }
        }
    });

    let _ = tokio::try_join!(plugin_loop, control_loop);
    Ok(())
}

/// Handle the (single) plugin connection: relay plugin→client responses and
/// track hello/connection state. At most one plugin is active; a new plugin
/// connection replaces the previous sender.
async fn handle_plugin(bridge: Arc<Bridge>, stream: TcpStream) -> anyhow::Result<()> {
    let ws = accept_async(stream).await?;
    let (mut write, mut read) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    *bridge.plugin_tx.write().await = Some(tx);
    bridge.broadcast_state().await;

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = read.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => route_from_plugin(&bridge, text.to_string()).await,
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    *bridge.plugin_tx.write().await = None;
    *bridge.last_hello.write().await = None;
    bridge.broadcast_state().await;
    Ok(())
}

/// A frame arrived from the plugin. If it is a `hello` (or has no id), update
/// state and broadcast. Otherwise it is a response carrying a namespaced id of
/// the form `c{clientId}@@{origId}`; restore the original id and route it to the
/// originating client.
async fn route_from_plugin(bridge: &Arc<Bridge>, text: String) {
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        warn!("invalid JSON from plugin: {}", text);
        return;
    };

    let kind = value.get("type").and_then(|v| v.as_str());
    let id = value.get("id").and_then(|v| v.as_str()).map(str::to_string);

    if kind == Some("hello") || id.is_none() {
        if kind == Some("hello") {
            *bridge.last_hello.write().await =
                Some(value.get("result").cloned().unwrap_or_else(|| json!({})));
        }
        bridge.broadcast_state().await;
        return;
    }

    let id = id.unwrap();
    let Some((client_id, orig_id)) = split_namespaced_id(&id) else {
        warn!("plugin response with unroutable id: {}", id);
        return;
    };

    // Restore the original id for the client.
    if let Some(obj) = value.as_object_mut() {
        obj.insert("id".to_string(), Value::String(orig_id));
    }

    let clients = bridge.clients.read().await;
    if let Some(tx) = clients.get(&client_id) {
        let _ = tx.send(Message::Text(value.to_string().into()));
    } else {
        warn!("no client {} for plugin response", client_id);
    }
}

/// Handle one control client (an MCP process). Forward its requests to the
/// plugin with a namespaced id; if no plugin is connected, answer with a loud
/// `live_not_connected` error so the caller never silently falls back to batch.
async fn handle_client(bridge: Arc<Bridge>, stream: TcpStream) -> anyhow::Result<()> {
    let ws = accept_async(stream).await?;
    let (mut write, mut read) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    let client_id = bridge.next_client_id.fetch_add(1, Ordering::Relaxed);
    bridge.clients.write().await.insert(client_id, tx.clone());

    // Send the current state immediately so a freshly-connected client knows
    // whether the plugin is present without waiting for an event.
    let _ = tx.send(bridge.state_frame().await);

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = read.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                forward_from_client(&bridge, client_id, &tx, text.to_string()).await
            }
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    bridge.clients.write().await.remove(&client_id);
    Ok(())
}

/// A request arrived from a control client. Namespace its id and forward to the
/// plugin; if there is no plugin, reply with an error response to this client.
async fn forward_from_client(bridge: &Arc<Bridge>, client_id: u64, client_tx: &Tx, text: String) {
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        warn!("invalid JSON from client {}: {}", client_id, text);
        return;
    };

    let orig_id = value
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let plugin_tx = bridge.plugin_tx.read().await.clone();
    let Some(plugin_tx) = plugin_tx else {
        // No plugin: answer the client directly so its pending request resolves.
        if let Some(orig_id) = orig_id {
            let err = json!({
                "id": orig_id,
                "type": "response",
                "ok": false,
                "error": {
                    "code": "live_not_connected",
                    "message": "no Aseprite plugin connected to the bridge",
                    "details": { "doNotFallBackToBatch": true }
                }
            });
            let _ = client_tx.send(Message::Text(err.to_string().into()));
        }
        return;
    };

    if let Some(orig_id) = &orig_id {
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "id".to_string(),
                Value::String(make_namespaced_id(client_id, orig_id)),
            );
        }
    }
    let _ = plugin_tx.send(Message::Text(value.to_string().into()));
}

fn make_namespaced_id(client_id: u64, orig_id: &str) -> String {
    format!("c{}{}{}", client_id, ID_SEP, orig_id)
}

fn split_namespaced_id(id: &str) -> Option<(u64, String)> {
    let rest = id.strip_prefix('c')?;
    let (client, orig) = rest.split_once(ID_SEP)?;
    let client_id = client.parse::<u64>().ok()?;
    Some((client_id, orig.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_id_roundtrip() {
        let ns = make_namespaced_id(7, "live-42");
        assert_eq!(ns, "c7@@live-42");
        assert_eq!(split_namespaced_id(&ns), Some((7, "live-42".to_string())));
    }

    #[test]
    fn split_rejects_garbage() {
        assert_eq!(split_namespaced_id("live-42"), None);
        assert_eq!(split_namespaced_id("cx@@live-1"), None);
    }

    #[test]
    fn bind_address_is_loopback_only() {
        let addr = loopback_addr(9876);
        assert_eq!(addr, "127.0.0.1:9876");
        // Must never bind a routable/all-interfaces address (checklist 10.2).
        assert!(addr.starts_with("127.0.0.1:"));
        assert!(!addr.starts_with("0.0.0.0"));
    }
}
