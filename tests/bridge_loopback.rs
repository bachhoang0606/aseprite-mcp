//! Loopback integration test for the standalone bridge (SPEC-001 / ADR-0002).
//!
//! Spawns the real `aseprite-live-bridge` binary on ephemeral ports, then plays
//! the roles of the Aseprite plugin and two MCP control clients to verify the
//! relay + id-namespacing without needing Aseprite at all.

use std::process::{Child, Command};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

struct BridgeProc(Child);
impl Drop for BridgeProc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn connect_retry(url: &str) -> Ws {
    for _ in 0..50 {
        if let Ok((ws, _)) = connect_async(url).await {
            return ws;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("could not connect to {url}");
}

async fn send(ws: &mut Ws, value: Value) {
    ws.send(Message::Text(value.to_string().into())).await.unwrap();
}

/// Read the next text frame within a timeout and parse it as JSON.
async fn recv(ws: &mut Ws) -> Value {
    let fut = async {
        while let Some(msg) = ws.next().await {
            if let Ok(Message::Text(t)) = msg {
                return serde_json::from_str::<Value>(&t).unwrap();
            }
        }
        panic!("socket closed before a text frame arrived");
    };
    tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .expect("timed out waiting for a frame")
}

#[tokio::test]
async fn bridge_relays_and_namespaces() {
    let plugin_port = free_port();
    let control_port = free_port();

    let child = Command::new(env!("CARGO_BIN_EXE_aseprite-live-bridge"))
        .env("ASEPRITE_MCP_LIVE_PORT", plugin_port.to_string())
        .env("ASEPRITE_MCP_LIVE_CONTROL_PORT", control_port.to_string())
        .env("RUST_LOG", "warn")
        .spawn()
        .expect("spawn bridge");
    let _guard = BridgeProc(child);

    let plugin_url = format!("ws://127.0.0.1:{plugin_port}/");
    let control_url = format!("ws://127.0.0.1:{control_port}/");

    // 1) Plugin connects first.
    let mut plugin = connect_retry(&plugin_url).await;

    // 2) Two control clients connect; each should immediately learn the plugin
    //    is present via a bridge_state frame.
    let mut client_a = connect_retry(&control_url).await;
    let mut client_b = connect_retry(&control_url).await;

    let state_a = recv(&mut client_a).await;
    assert_eq!(state_a["type"], "bridge_state");
    assert_eq!(state_a["pluginConnected"], json!(true));
    let state_b = recv(&mut client_b).await;
    assert_eq!(state_b["pluginConnected"], json!(true));

    // 3) Client A issues a request. The plugin must receive it with a namespaced
    //    id, echo a response, and the bridge must route it back to A as live-1.
    send(
        &mut client_a,
        json!({ "protocol": "aseprite-live-edit", "version": 1, "id": "live-1", "type": "get_capabilities" }),
    )
    .await;

    let on_plugin = recv(&mut plugin).await;
    let ns_id = on_plugin["id"].as_str().unwrap().to_string();
    assert!(ns_id.contains("@@live-1"), "expected namespaced id, got {ns_id}");

    // Plugin echoes the same (namespaced) id back as a response.
    send(
        &mut plugin,
        json!({ "id": ns_id, "type": "response", "ok": true, "result": { "ok": 1 } }),
    )
    .await;

    let resp_a = recv(&mut client_a).await;
    assert_eq!(resp_a["id"], json!("live-1"), "id must be restored for the caller");
    assert_eq!(resp_a["ok"], json!(true));

    // 4) Client B must NOT have received A's response (no frame mentioning live-1).
    let b_quiet = tokio::time::timeout(Duration::from_millis(500), client_b.next()).await;
    if let Ok(Some(Ok(Message::Text(t)))) = b_quiet {
        assert!(
            !t.contains("live-1"),
            "client B wrongly received A's response: {t}"
        );
    }

    // 5) Plugin disconnects → both clients are told the plugin is gone.
    plugin.close(None).await.unwrap();
    drop(plugin);

    let down_a = wait_for_plugin_state(&mut client_a, false).await;
    assert_eq!(down_a, false);
    let down_b = wait_for_plugin_state(&mut client_b, false).await;
    assert_eq!(down_b, false);
}

/// Drain frames until a bridge_state announces the desired pluginConnected value.
async fn wait_for_plugin_state(ws: &mut Ws, want: bool) -> bool {
    for _ in 0..10 {
        let v = recv(ws).await;
        if v["type"] == "bridge_state" {
            if v["pluginConnected"] == json!(want) {
                return want;
            }
        }
    }
    panic!("never observed pluginConnected={want}");
}
