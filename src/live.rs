use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

use crate::color_ops::{self, ColorOp, Rgba};
use crate::utils::validate_hex_color;

pub const LIVE_PROTOCOL: &str = "aseprite-live-edit";
pub const LIVE_VERSION: u32 = 1;
const DEFAULT_PORT: u16 = 9876;
// A connected Aseprite session processes live commands fine in the background
// (verified 2026-06-11; upstream aseprite#3009), but its UI-thread scheduling
// can still hiccup — long transactions, modal dialogs, minimized windows, or a
// reconnect that waits for one focus. 30s tolerates those without returning a
// spurious live_timeout for commands that would have landed.
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
// Floor for the env override: below this, normal UI-thread scheduling jitter
// alone would produce spurious live_timeout errors.
const MIN_REQUEST_TIMEOUT_MS: u64 = 1_000;

/// Live request timeout, tunable via `ASEPRITE_MCP_LIVE_TIMEOUT_MS` (checklist
/// 2.5). Unparsable or sub-1s values fall back to the 30s default.
fn request_timeout_ms() -> u64 {
    std::env::var("ASEPRITE_MCP_LIVE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&ms| ms >= MIN_REQUEST_TIMEOUT_MS)
        .unwrap_or(DEFAULT_REQUEST_TIMEOUT_MS)
}

/// Loud, actionable message returned whenever a live edit is attempted while the
/// Aseprite WebSocket plugin is not connected. The wording explicitly forbids the
/// silent batch fallback that defeats the whole point of a live-first workflow.
pub const LIVE_DISCONNECTED_HINT: &str = "Live Aseprite plugin is NOT connected. \
     Do NOT fall back to batch/file drawing tools to 'work around' this: batch tools edit files \
     on disk and the change will NOT appear in the open Aseprite window. Stop and tell the user. \
     To fix: (1) make sure Aseprite is running with the aseprite-mcp-plugin extension installed \
     and enabled, (2) restart the MCP client so this server and its WebSocket bridge on port 9876 \
     are loaded, (3) call live_preflight until ready=true.";

#[derive(Debug, Clone)]
pub struct LiveBridge {
    /// Plugin port (where the Aseprite plugin connects to the standalone bridge).
    port: u16,
    /// Control port (where this MCP process connects to the bridge as a client).
    control_port: u16,
    /// Sender to the bridge control socket (None while the control link is down).
    sender: Arc<RwLock<Option<mpsc::UnboundedSender<Message>>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<LiveResponse>>>>,
    last_hello: Arc<RwLock<Option<Value>>>,
    /// Whether the bridge reports an Aseprite plugin currently connected to it.
    plugin_connected: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
    command_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Serialize)]
struct LiveRequest {
    protocol: &'static str,
    version: u32,
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LiveResponse {
    pub protocol: Option<String>,
    pub version: Option<u32>,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub ok: Option<bool>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveLayerParams {
    /// Layer to create or reuse. Defaults to "AI Draft".
    pub layer: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveEnsureFramesParams {
    /// Minimum number of frames required in the active sprite.
    pub count: u32,
    /// Frame duration in seconds. Defaults to 0.12.
    pub duration: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveClearCelParams {
    /// Target layer. Defaults to "AI Draft".
    pub layer: Option<String>,
    /// Target frame number, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveDrawPixelsParams {
    /// Pixel batch to draw into the active Aseprite sprite.
    pub pixels: Vec<LivePixel>,
    /// Target layer. Defaults to "AI Draft".
    pub layer: Option<String>,
    /// Target frame number, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LivePixel {
    pub x: i32,
    pub y: i32,
    /// Hex color in #rrggbb or #rrggbbaa format.
    pub color: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveUseToolParams {
    /// Tool name: pencil, line, rectangle, filled_rectangle, ellipse, filled_ellipse, paint_bucket, or eraser.
    pub tool: String,
    /// Points used by the Aseprite tool.
    pub points: Vec<LivePoint>,
    /// Hex color in #rrggbb or #rrggbbaa format.
    pub color: String,
    /// Brush size. Defaults to 1.
    pub brush_size: Option<u32>,
    /// Target layer. Defaults to "AI Draft".
    pub layer: Option<String>,
    /// Target frame number, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LivePoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveEmptyParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSpriteSelectorParams {
    pub filename: Option<String>,
    pub index: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveOpenSpriteParams {
    pub filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSaveSpriteAsParams {
    pub filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSavePreviewParams {
    /// Output PNG path for the upscaled preview.
    pub filename: String,
    /// Integer nearest-neighbor upscale factor. Omit to auto-pick one that lands
    /// the sprite's long edge near ~1024px (capped at 16x).
    pub scale: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveAsciiViewParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSaveFilmstripParams {
    /// Output PNG path for the upscaled film-strip.
    pub filename: String,
    /// Integer nearest-neighbor upscale factor. Omit to auto-pick one that lands
    /// the strip's long edge near ~1024px (capped at 16x).
    pub scale: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveCloseSpriteParams {
    pub filename: Option<String>,
    pub index: Option<u32>,
    pub save: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveResizeCanvasParams {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LiveRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LiveSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSpritePropertiesParams {
    pub data: Option<String>,
    pub transparent_color: Option<u32>,
    pub color: Option<String>,
    pub grid_bounds: Option<LiveRect>,
    pub pixel_ratio: Option<LiveSize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveLayerNameParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveRenameLayerParams {
    pub name: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveCreateGroupLayerParams {
    pub name: String,
    pub parent: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetLayerVisibilityParams {
    pub name: String,
    pub visible: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetLayerPropertiesParams {
    pub name: String,
    pub visible: Option<bool>,
    pub editable: Option<bool>,
    pub opacity: Option<u8>,
    pub blend_mode: Option<String>,
    pub stack_index: Option<u32>,
    pub parent: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveFrameSelectorParams {
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetFramePropertiesParams {
    pub frame: u32,
    pub duration: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveNewEmptyFrameParams {
    pub index: Option<u32>,
    pub duration: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveNewFrameParams {
    pub frame: Option<u32>,
    pub source_frame: Option<u32>,
    pub duration: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveListCelsParams {
    pub layer: Option<String>,
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveNewCelParams {
    pub layer: String,
    pub frame: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub opacity: Option<u8>,
    pub replace: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetCelPropertiesParams {
    pub layer: String,
    pub frame: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub opacity: Option<u8>,
    pub z_index: Option<i32>,
    pub data: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveDeleteCelParams {
    pub layer: String,
    pub frame: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveTagNameParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveNewTagParams {
    pub name: String,
    pub from_frame: u32,
    pub to_frame: Option<u32>,
    pub repeats: Option<u32>,
    pub data: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetTagPropertiesParams {
    pub name: String,
    pub new_name: Option<String>,
    pub repeats: Option<u32>,
    pub data: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LivePointPayload {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LiveSliceCenter {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveNewSliceParams {
    pub name: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub data: Option<String>,
    pub color: Option<String>,
    pub pivot: Option<LivePointPayload>,
    pub center: Option<LiveSliceCenter>,
    pub replace: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetSlicePropertiesParams {
    pub name: String,
    pub new_name: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub data: Option<String>,
    pub color: Option<String>,
    pub pivot: Option<LivePointPayload>,
    pub center: Option<LiveSliceCenter>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetSelectionParams {
    pub mode: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveListPaletteParams {
    pub palette: Option<u32>,
    pub from: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetPaletteColorParams {
    pub palette: Option<u32>,
    pub index: u32,
    pub color: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveResizePaletteParams {
    pub palette: Option<u32>,
    pub count: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveRunAppCommandParams {
    pub name: String,
    /// Optional key/value parameters forwarded to the Aseprite app command.
    /// Typed as an object map so the generated JSON Schema is a valid object
    /// schema (a bare `serde_json::Value` emits a boolean schema that some MCP
    /// clients reject during tools/list validation).
    #[schemars(with = "Option<std::collections::HashMap<String, serde_json::Value>>")]
    pub params: Option<Value>,
}

// ---------------------------------------------------------------------------
// SPEC-003 — tilemap / tileset / autotile tool family.
// A tilemap cel is an image whose "pixels" are tile indices, so tile placement
// reuses the live_draw_pixels coordinate-batch shape almost verbatim.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveCreateTilemapLayerParams {
    /// Name for the new tilemap layer.
    pub name: String,
    /// Tile width in pixels (sets the tileset grid). Defaults to 16.
    pub tile_width: Option<u32>,
    /// Tile height in pixels. Defaults to tile_width.
    pub tile_height: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveGetTilesetParams {
    /// 1-based index into the sprite's tilesets. Omit to resolve via `layer`.
    pub index: Option<u32>,
    /// Tilemap layer name; its tileset is used when `index` is omitted.
    pub layer: Option<String>,
    /// When set, also write a vision-legible packed PNG of the tiles here,
    /// nearest-neighbor upscaled like live_save_preview so the agent can SEE
    /// them. Omit for metadata only.
    pub filename: Option<String>,
    /// Integer upscale for the packed preview. Omit for an automatic factor.
    pub scale: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveTile {
    /// Tile-grid column (0-based) in the tilemap cel.
    pub x: i32,
    /// Tile-grid row (0-based) in the tilemap cel.
    pub y: i32,
    /// Tile index to place (0 = empty). Must exist in the layer's tileset.
    pub tile_index: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveStampTilesParams {
    /// Tile placements — the tile-grid analogue of live_draw_pixels.
    pub tiles: Vec<LiveTile>,
    /// Target tilemap layer (required; tilemaps have no AI Draft default).
    pub layer: String,
    /// Target frame number, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSetTileDataParams {
    /// Tile index whose user data to set.
    pub tile_index: u32,
    /// 1-based tileset index. Omit to resolve via `layer`.
    pub tileset_index: Option<u32>,
    /// Tilemap layer whose tileset is used when `tileset_index` is omitted.
    pub layer: Option<String>,
    /// User-data string stored on the tile (e.g. a terrain/collision tag).
    pub data: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LivePackSimilarTilesParams {
    /// Tile width in pixels to slice the mockup into.
    pub tile_width: u32,
    /// Tile height in pixels. Defaults to tile_width.
    pub tile_height: Option<u32>,
    /// Source layer holding the painted mockup. Omit for the active layer.
    pub layer: Option<String>,
    /// Name for the generated tilemap layer. Defaults to "Tilemap".
    pub tilemap_layer: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveExportTilesetParams {
    /// Engine target: "tiled" (.tsj), "godot" (.tres), or "json".
    pub target: String,
    /// Output file path. A sibling packed-tileset PNG is written alongside it.
    pub path: String,
    /// Tilemap layer to export. Omit for the active layer.
    pub layer: Option<String>,
    /// Frame number, 1-based. Omit for the active frame.
    pub frame: Option<u32>,
    /// Autotile layout for terrain/wangset emission: "none" (default), "blob47", or "wang16".
    pub layout: Option<String>,
    /// Tiles per row in the packed PNG. Omit for an automatic near-square pack.
    pub image_columns: Option<u32>,
}

// ---------------------------------------------------------------------------
// SPEC-004 — constrained / semantic colour ops. The colour MATH is pure Rust
// (`crate::color_ops`); these tools fetch a region's UNIQUE colours, build a
// colour→colour map, and apply it via one plugin replace pass.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LivePaletteSnapParams {
    /// Target layer. Omit for the active layer.
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit for the active frame.
    pub frame: Option<u32>,
    /// Limit to the active selection (else the whole layer). Defaults to false.
    pub selection_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveAdjustPixelsParams {
    /// Intent op: "darken", "lighten", "hue_shift", "colorize", or "snap".
    pub op: String,
    /// darken/lighten: fraction 0..1; hue_shift: degrees. Ignored by colorize/snap.
    pub amount: Option<f64>,
    /// colorize: target hue in degrees (0..360).
    pub hue: Option<f64>,
    /// Snap the result to the nearest palette colour (legal by construction).
    /// Defaults to true; pass false to free-shade an open palette.
    pub clamp_to_palette: Option<bool>,
    /// Target layer. Omit for the active layer.
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit for the active frame.
    pub frame: Option<u32>,
    /// Limit to the active selection. Defaults to false.
    pub selection_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveSnapColorsParams {
    /// Hex colours (#rrggbb / #rrggbbaa) to snap to the active sprite's palette.
    pub colors: Vec<String>,
}

impl LiveBridge {
    pub fn start_from_env() -> Arc<Self> {
        let port = std::env::var("ASEPRITE_MCP_LIVE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let control_port = std::env::var("ASEPRITE_MCP_LIVE_CONTROL_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| port.wrapping_add(1));

        let bridge = Arc::new(Self {
            port,
            control_port,
            sender: Arc::new(RwLock::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            last_hello: Arc::new(RwLock::new(None)),
            plugin_connected: Arc::new(AtomicBool::new(false)),
            next_id: Arc::new(AtomicU64::new(1)),
            command_lock: Arc::new(Mutex::new(())),
        });

        // Connect to the standalone bridge as a control client (spawning the
        // bridge if it is not already running) and keep the link self-healing.
        let task_bridge = bridge.clone();
        tokio::spawn(async move {
            task_bridge.run_client_loop().await;
        });

        bridge
    }

    pub async fn status_json(&self) -> String {
        let bridge_linked = self.sender.read().await.is_some();
        let connected = bridge_linked && self.plugin_connected.load(Ordering::Relaxed);
        let last_hello = self.last_hello.read().await.clone();
        let pending = self.pending.lock().await.len();
        json!({
            "status": if connected { "connected" } else { "disconnected" },
            "connected": connected,
            "ready": connected,
            "bridgeLinked": bridge_linked,
            "host": "127.0.0.1",
            "port": self.port,
            "controlPort": self.control_port,
            "protocol": LIVE_PROTOCOL,
            "protocolVersion": LIVE_VERSION,
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "pending": pending,
            "lastHello": last_hello,
        })
        .to_string()
    }

    /// Preflight guard for live-editing workflows. Callers MUST invoke this (or
    /// `status_json`) before any drawing/edit work and only proceed when
    /// `ready == true`. When not ready, the directive explicitly forbids the
    /// silent batch fallback so a live-first workflow never degrades quietly.
    pub async fn preflight(&self) -> String {
        let bridge_linked = self.sender.read().await.is_some();
        let connected = bridge_linked && self.plugin_connected.load(Ordering::Relaxed);
        let last_hello = self.last_hello.read().await.clone();
        let directive = if connected {
            "READY: live Aseprite session is connected. Proceed with live_* tools.".to_string()
        } else {
            format!("BLOCKED: {}", LIVE_DISCONNECTED_HINT)
        };
        let remediation = if connected {
            json!([])
        } else {
            json!([
                "Ensure Aseprite is running with the aseprite-mcp-plugin extension installed and enabled",
                "Restart the MCP client so this server (and its WebSocket bridge on port 9876) is loaded",
                "Re-run live_preflight until ready=true",
                "Never substitute batch/file drawing tools to 'work around' a disconnected live session"
            ])
        };
        json!({
            "ready": connected,
            "connected": connected,
            "bridgeLinked": bridge_linked,
            "host": "127.0.0.1",
            "port": self.port,
            "controlPort": self.control_port,
            "protocol": LIVE_PROTOCOL,
            "protocolVersion": LIVE_VERSION,
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "lastHello": last_hello,
            "directive": directive,
            "remediation": remediation,
        })
        .to_string()
    }

    pub async fn get_capabilities(&self) -> Result<String, String> {
        self.command("get_capabilities", None, None).await
    }

    pub async fn list_sprites(&self) -> Result<String, String> {
        self.command("list_sprites", None, None).await
    }

    pub async fn open_sprite(&self, params: LiveOpenSpriteParams) -> Result<String, String> {
        validate_non_empty("filename", &params.filename)?;
        self.command(
            "open_sprite",
            None,
            Some(json!({ "filename": params.filename })),
        )
        .await
    }

    pub async fn activate_sprite(
        &self,
        params: LiveSpriteSelectorParams,
    ) -> Result<String, String> {
        if params.filename.as_deref().unwrap_or("").is_empty() && params.index.is_none() {
            return Err(live_error(
                "missing_sprite_selector",
                "filename or index is required",
                None,
            ));
        }
        self.command(
            "activate_sprite",
            None,
            Some(json_strip_nulls(json!({
                "filename": params.filename,
                "index": params.index,
            }))),
        )
        .await
    }

    pub async fn get_active_site(&self) -> Result<String, String> {
        self.command("get_active_site", None, None).await
    }

    pub async fn get_sprite_info(&self) -> Result<String, String> {
        self.command("get_sprite_info", None, None).await
    }

    pub async fn set_sprite_properties(
        &self,
        params: LiveSpritePropertiesParams,
    ) -> Result<String, String> {
        if let Some(color) = &params.color {
            validate_hex_color(color).map_err(|err| {
                live_error("invalid_color", &format!("invalid color: {}", err), None)
            })?;
        }
        self.command(
            "set_sprite_properties",
            None,
            Some(json_strip_nulls(json!({
                "data": params.data,
                "transparentColor": params.transparent_color,
                "color": params.color,
                "gridBounds": params.grid_bounds,
                "pixelRatio": params.pixel_ratio,
            }))),
        )
        .await
    }

    pub async fn save_sprite(&self) -> Result<String, String> {
        self.command("save_sprite", None, None).await
    }

    pub async fn save_sprite_as(&self, params: LiveSaveSpriteAsParams) -> Result<String, String> {
        validate_non_empty("filename", &params.filename)?;
        self.command(
            "save_sprite_as",
            None,
            Some(json!({ "filename": params.filename })),
        )
        .await
    }

    pub async fn save_copy_as(&self, params: LiveSaveSpriteAsParams) -> Result<String, String> {
        validate_non_empty("filename", &params.filename)?;
        self.command(
            "save_copy_as",
            None,
            Some(json!({ "filename": params.filename })),
        )
        .await
    }

    /// Save a vision-legible preview: have the plugin write a faithful 1x copy to
    /// a sibling temp path, then nearest-neighbor upscale it in-process so the
    /// sprite's long edge lands near ~1024px (raw 1x previews are too small for a
    /// vision model to read). The upscale is pure Rust — the live document is
    /// never touched, so this adds no undo entries and needs no plugin redeploy.
    pub async fn save_preview(&self, params: LiveSavePreviewParams) -> Result<String, String> {
        validate_preview_request(&params.filename, params.scale)?;

        // 1) Render the active frame to a single-frame PNG in the system temp dir
        // (NOT the user's project tree), so a hard crash between save and cleanup
        // cannot leak a file into their repo; the id keeps concurrent previews
        // distinct. The plugin's `save_preview` renders the active frame into a
        // standalone Image and Image:saveAs's it — this is modal-free even on a
        // multi-frame sprite (ADR-0004), unlike `save_copy_as`/saveCopyAs which
        // pops Aseprite's "format doesn't support multiple frames" dialog.
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_preview_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_str = temp.to_string_lossy().to_string();
        self.command("save_preview", None, Some(json!({ "filename": temp_str })))
            .await?;

        // 2) Upscale in-process; clean up the temp regardless of outcome.
        let info = crate::preview::render_preview(
            &temp,
            std::path::Path::new(&params.filename),
            params.scale,
        );
        let _ = std::fs::remove_file(&temp);
        let info = info.map_err(|e| live_error("preview_render_failed", &e, None))?;

        Ok(json!({
            "changed": true,
            "filename": params.filename,
            "scale": info.scale,
            "source": { "width": info.source_width, "height": info.source_height },
            "preview": { "width": info.preview_width, "height": info.preview_height },
        })
        .to_string())
    }

    /// Return the active frame as a one-glyph-per-pixel TEXT grid (+ legend), which
    /// an LLM reads far more reliably than a small image (research Path 1 / §A).
    /// Reuses the modal-free `save_preview` render to a 1x temp PNG, then converts
    /// to text in-process; the live document is untouched.
    pub async fn ascii_view(&self, _params: LiveAsciiViewParams) -> Result<String, String> {
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_ascii_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_str = temp.to_string_lossy().to_string();
        self.command("save_preview", None, Some(json!({ "filename": temp_str })))
            .await?;

        let result = (|| -> Result<String, String> {
            let img = image::open(&temp)
                .map_err(|e| format!("failed to decode the rendered frame: {e}"))?
                .to_rgba8();
            crate::ascii_view::image_to_ascii(&img)
        })();
        let _ = std::fs::remove_file(&temp);
        result.map_err(|e| live_error("ascii_view_failed", &e, None))
    }

    /// Composite every animation frame into ONE upscaled image-grid so the agent
    /// can review motion — the Claude API only sees the first frame of a GIF, so a
    /// strip is the only way to review a walk/attack cycle (research Path 1 / §A).
    /// Renders each frame via the modal-free `save_preview`, composites + upscales
    /// in-process, and restores the user's active frame. Live document untouched.
    pub async fn save_filmstrip(
        &self,
        params: LiveSaveFilmstripParams,
    ) -> Result<String, String> {
        validate_non_empty("filename", &params.filename)?;

        // Active frame (to restore) + frame count, both from one site query.
        let site: Value = serde_json::from_str(&self.command("get_active_site", None, None).await?)
            .unwrap_or_else(|_| json!({}));
        let frames = site
            .get("sprite")
            .and_then(|s| s.get("frames"))
            .and_then(|f| f.as_u64())
            .unwrap_or(0) as u32;
        if frames == 0 {
            return Err(live_error("no_sprite", "No active sprite to film-strip", None));
        }
        let orig_frame = site.get("frame").and_then(|f| f.as_u64()).unwrap_or(1) as u32;

        // Render each frame to a 1x temp PNG (set the active frame first; the
        // save_preview handler renders whatever is active).
        let mut imgs: Vec<image::RgbaImage> = Vec::with_capacity(frames as usize);
        let mut temps: Vec<std::path::PathBuf> = Vec::new();
        let mut err: Option<String> = None;
        for i in 1..=frames {
            if frames > 1 {
                if let Err(e) = self
                    .command("set_active_frame", None, Some(json!({ "frame": i })))
                    .await
                {
                    err = Some(e);
                    break;
                }
            }
            let temp = std::env::temp_dir().join(format!(
                "aseprite_mcp_filmstrip_{}_{i}.png",
                self.next_id.fetch_add(1, Ordering::Relaxed)
            ));
            temps.push(temp.clone());
            if let Err(e) = self
                .command("save_preview", None, Some(json!({ "filename": temp.to_string_lossy() })))
                .await
            {
                err = Some(e);
                break;
            }
            match image::open(&temp) {
                Ok(im) => imgs.push(im.to_rgba8()),
                Err(e) => {
                    err = Some(live_error("filmstrip_decode_failed", &format!("{e}"), None));
                    break;
                }
            }
        }
        // Restore the user's active frame (best-effort) and clean up temps.
        if frames > 1 {
            let _ = self
                .command("set_active_frame", None, Some(json!({ "frame": orig_frame })))
                .await;
        }
        for t in &temps {
            let _ = std::fs::remove_file(t);
        }
        if let Some(e) = err {
            return Err(e);
        }

        // Compose the grid, then nearest-neighbor upscale toward ~1024px.
        let (strip, cols, rows) = crate::filmstrip::compose_grid(&imgs)
            .map_err(|e| live_error("filmstrip_compose_failed", &e, None))?;
        let (sw, sh) = strip.dimensions();
        let scale = params
            .scale
            .map(|s| s.max(1))
            .unwrap_or_else(|| crate::preview::auto_preview_scale(sw, sh));
        let scale = crate::preview::clamp_scale_to_max_edge(sw, sh, scale);
        let out = if scale == 1 {
            strip
        } else {
            image::imageops::resize(&strip, sw * scale, sh * scale, image::imageops::FilterType::Nearest)
        };
        out.save_with_format(&params.filename, image::ImageFormat::Png)
            .map_err(|e| live_error("filmstrip_write_failed", &format!("{e}"), None))?;

        Ok(json!({
            "changed": true,
            "filename": params.filename,
            "frames": frames,
            "cols": cols,
            "rows": rows,
            "scale": scale,
            "strip": { "width": sw * scale, "height": sh * scale },
            "note": "frames are row-major (left→right, top→bottom); a gray gap separates cells",
        })
        .to_string())
    }

    pub async fn close_sprite(&self, params: LiveCloseSpriteParams) -> Result<String, String> {
        self.command(
            "close_sprite",
            None,
            Some(json_strip_nulls(json!({
                "filename": params.filename,
                "index": params.index,
                "save": params.save,
            }))),
        )
        .await
    }

    pub async fn resize_canvas(&self, params: LiveResizeCanvasParams) -> Result<String, String> {
        if params.width == 0 || params.height == 0 {
            return Err(live_error(
                "invalid_size",
                "width and height must be greater than zero",
                None,
            ));
        }
        self.command(
            "resize_canvas",
            None,
            Some(json!({
                "width": params.width,
                "height": params.height,
            })),
        )
        .await
    }

    pub async fn ensure_ai_draft_layer(&self, layer: Option<String>) -> Result<String, String> {
        let layer = layer.unwrap_or_else(default_layer);
        self.ensure_layer(LiveLayerNameParams { name: layer }).await
    }

    pub async fn list_layers(&self) -> Result<String, String> {
        self.command("list_layers", None, None).await
    }

    pub async fn ensure_layer(&self, params: LiveLayerNameParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command("ensure_layer", None, Some(json!({ "name": params.name })))
            .await
    }

    pub async fn set_active_layer(&self, params: LiveLayerNameParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command(
            "set_active_layer",
            None,
            Some(json!({ "name": params.name })),
        )
        .await
    }

    pub async fn rename_layer(&self, params: LiveRenameLayerParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        validate_non_empty("new_name", &params.new_name)?;
        self.command(
            "rename_layer",
            None,
            Some(json!({
                "name": params.name,
                "newName": params.new_name,
            })),
        )
        .await
    }

    pub async fn create_group_layer(
        &self,
        params: LiveCreateGroupLayerParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command(
            "create_group_layer",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "parent": params.parent,
            }))),
        )
        .await
    }

    pub async fn set_layer_visibility(
        &self,
        params: LiveSetLayerVisibilityParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command(
            "set_layer_visibility",
            None,
            Some(json!({
                "name": params.name,
                "visible": params.visible,
            })),
        )
        .await
    }

    pub async fn set_layer_properties(
        &self,
        params: LiveSetLayerPropertiesParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command(
            "set_layer_properties",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "visible": params.visible,
                "editable": params.editable,
                "opacity": params.opacity,
                "blendMode": params.blend_mode,
                "stackIndex": params.stack_index,
                "parent": params.parent,
            }))),
        )
        .await
    }

    pub async fn delete_layer(&self, params: LiveLayerNameParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command("delete_layer", None, Some(json!({ "name": params.name })))
            .await
    }

    pub async fn ensure_frames(&self, params: LiveEnsureFramesParams) -> Result<String, String> {
        if params.count == 0 {
            return Err(live_error(
                "invalid_frame_count",
                "frame count must be greater than zero",
                None,
            ));
        }

        self.command(
            "ensure_frames",
            None,
            Some(json!({
                "count": params.count,
                "duration": params.duration.unwrap_or(0.12),
            })),
        )
        .await
    }

    pub async fn list_frames(&self) -> Result<String, String> {
        self.command("list_frames", None, None).await
    }

    pub async fn set_active_frame(
        &self,
        params: LiveFrameSelectorParams,
    ) -> Result<String, String> {
        self.command(
            "set_active_frame",
            None,
            Some(json_strip_nulls(json!({
                "frame": params.frame,
            }))),
        )
        .await
    }

    pub async fn set_frame_properties(
        &self,
        params: LiveSetFramePropertiesParams,
    ) -> Result<String, String> {
        validate_frame(params.frame)?;
        if let Some(duration) = params.duration {
            if duration < 0.0 {
                return Err(live_error(
                    "invalid_duration",
                    "duration must be non-negative",
                    None,
                ));
            }
        }
        self.command(
            "set_frame_properties",
            None,
            Some(json_strip_nulls(json!({
                "frame": params.frame,
                "duration": params.duration,
            }))),
        )
        .await
    }

    pub async fn new_empty_frame(&self, params: LiveNewEmptyFrameParams) -> Result<String, String> {
        self.command(
            "new_empty_frame",
            None,
            Some(json_strip_nulls(json!({
                "index": params.index,
                "duration": params.duration,
            }))),
        )
        .await
    }

    pub async fn new_frame(&self, params: LiveNewFrameParams) -> Result<String, String> {
        self.command(
            "new_frame",
            None,
            Some(json_strip_nulls(json!({
                "frame": params.frame,
                "sourceFrame": params.source_frame,
                "duration": params.duration,
            }))),
        )
        .await
    }

    pub async fn delete_frame(&self, params: LiveFrameSelectorParams) -> Result<String, String> {
        self.command(
            "delete_frame",
            None,
            Some(json_strip_nulls(json!({
                "frame": params.frame,
            }))),
        )
        .await
    }

    pub async fn clear_cel(&self, params: LiveClearCelParams) -> Result<String, String> {
        let target = live_target(params.layer, params.frame);
        self.command("clear_cel", Some(target), None).await
    }

    pub async fn list_cels(&self, params: LiveListCelsParams) -> Result<String, String> {
        self.command(
            "list_cels",
            None,
            Some(json_strip_nulls(json!({
                "layer": params.layer,
                "frame": params.frame,
            }))),
        )
        .await
    }

    pub async fn new_cel(&self, params: LiveNewCelParams) -> Result<String, String> {
        validate_non_empty("layer", &params.layer)?;
        validate_frame(params.frame)?;
        self.command(
            "new_cel",
            None,
            Some(json_strip_nulls(json!({
                "layer": params.layer,
                "frame": params.frame,
                "x": params.x,
                "y": params.y,
                "opacity": params.opacity,
                "replace": params.replace,
            }))),
        )
        .await
    }

    pub async fn set_cel_properties(
        &self,
        params: LiveSetCelPropertiesParams,
    ) -> Result<String, String> {
        validate_non_empty("layer", &params.layer)?;
        validate_frame(params.frame)?;
        self.command(
            "set_cel_properties",
            None,
            Some(json_strip_nulls(json!({
                "layer": params.layer,
                "frame": params.frame,
                "x": params.x,
                "y": params.y,
                "opacity": params.opacity,
                "zIndex": params.z_index,
                "data": params.data,
            }))),
        )
        .await
    }

    pub async fn delete_cel(&self, params: LiveDeleteCelParams) -> Result<String, String> {
        validate_non_empty("layer", &params.layer)?;
        validate_frame(params.frame)?;
        self.command(
            "delete_cel",
            None,
            Some(json!({
                "layer": params.layer,
                "frame": params.frame,
            })),
        )
        .await
    }

    pub async fn draw_pixels(&self, params: LiveDrawPixelsParams) -> Result<String, String> {
        if params.pixels.is_empty() {
            return Err(live_error(
                "invalid_payload",
                "pixels cannot be empty",
                None,
            ));
        }
        for pixel in &params.pixels {
            validate_hex_color(&pixel.color).map_err(|err| {
                live_error(
                    "invalid_color",
                    &format!("invalid pixel color '{}': {}", pixel.color, err),
                    None,
                )
            })?;
        }

        let target = live_target(params.layer, params.frame);
        self.command(
            "draw_pixels",
            Some(target),
            Some(json!({ "pixels": params.pixels })),
        )
        .await
    }

    pub async fn use_tool(&self, params: LiveUseToolParams) -> Result<String, String> {
        if params.points.is_empty() {
            return Err(live_error(
                "invalid_payload",
                "points cannot be empty",
                None,
            ));
        }
        validate_hex_color(&params.color).map_err(|err| {
            live_error(
                "invalid_color",
                &format!("invalid color '{}': {}", params.color, err),
                None,
            )
        })?;
        if !matches!(
            params.tool.as_str(),
            "pencil"
                | "line"
                | "rectangle"
                | "filled_rectangle"
                | "ellipse"
                | "filled_ellipse"
                | "paint_bucket"
                | "eraser"
        ) {
            return Err(live_error(
                "unsupported_tool",
                &format!("unsupported live tool '{}'", params.tool),
                None,
            ));
        }

        let target = live_target(params.layer, params.frame);
        self.command(
            "use_tool",
            Some(target),
            Some(json!({
                "tool": params.tool,
                "points": params.points,
                "color": params.color,
                "brushSize": params.brush_size.unwrap_or(1),
            })),
        )
        .await
    }

    pub async fn list_tags(&self) -> Result<String, String> {
        self.command("list_tags", None, None).await
    }

    pub async fn new_tag(&self, params: LiveNewTagParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        validate_frame(params.from_frame)?;
        if let Some(to_frame) = params.to_frame {
            validate_frame(to_frame)?;
        }
        if let Some(color) = &params.color {
            validate_hex_color(color).map_err(|err| {
                live_error("invalid_color", &format!("invalid color: {}", err), None)
            })?;
        }
        self.command(
            "new_tag",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "fromFrame": params.from_frame,
                "toFrame": params.to_frame,
                "repeats": params.repeats,
                "data": params.data,
                "color": params.color,
            }))),
        )
        .await
    }

    pub async fn set_tag_properties(
        &self,
        params: LiveSetTagPropertiesParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        if let Some(color) = &params.color {
            validate_hex_color(color).map_err(|err| {
                live_error("invalid_color", &format!("invalid color: {}", err), None)
            })?;
        }
        self.command(
            "set_tag_properties",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "newName": params.new_name,
                "repeats": params.repeats,
                "data": params.data,
                "color": params.color,
            }))),
        )
        .await
    }

    pub async fn delete_tag(&self, params: LiveTagNameParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command("delete_tag", None, Some(json!({ "name": params.name })))
            .await
    }

    pub async fn list_slices(&self) -> Result<String, String> {
        self.command("list_slices", None, None).await
    }

    pub async fn new_slice(&self, params: LiveNewSliceParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        if let Some(color) = &params.color {
            validate_hex_color(color).map_err(|err| {
                live_error("invalid_color", &format!("invalid color: {}", err), None)
            })?;
        }
        self.command(
            "new_slice",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "x": params.x,
                "y": params.y,
                "width": params.width,
                "height": params.height,
                "data": params.data,
                "color": params.color,
                "pivot": params.pivot,
                "center": params.center,
                "replace": params.replace,
            }))),
        )
        .await
    }

    pub async fn set_slice_properties(
        &self,
        params: LiveSetSlicePropertiesParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        if let Some(color) = &params.color {
            validate_hex_color(color).map_err(|err| {
                live_error("invalid_color", &format!("invalid color: {}", err), None)
            })?;
        }
        self.command(
            "set_slice_properties",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "newName": params.new_name,
                "x": params.x,
                "y": params.y,
                "width": params.width,
                "height": params.height,
                "data": params.data,
                "color": params.color,
                "pivot": params.pivot,
                "center": params.center,
            }))),
        )
        .await
    }

    pub async fn delete_slice(&self, params: LiveTagNameParams) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        self.command("delete_slice", None, Some(json!({ "name": params.name })))
            .await
    }

    pub async fn get_selection(&self) -> Result<String, String> {
        self.command("get_selection", None, None).await
    }

    pub async fn set_selection(&self, params: LiveSetSelectionParams) -> Result<String, String> {
        self.command(
            "set_selection",
            None,
            Some(json_strip_nulls(json!({
                "mode": params.mode,
                "x": params.x,
                "y": params.y,
                "width": params.width,
                "height": params.height,
            }))),
        )
        .await
    }

    pub async fn list_palette(&self, params: LiveListPaletteParams) -> Result<String, String> {
        self.command(
            "list_palette",
            None,
            Some(json_strip_nulls(json!({
                "palette": params.palette,
                "from": params.from,
                "limit": params.limit,
            }))),
        )
        .await
    }

    pub async fn set_palette_color(
        &self,
        params: LiveSetPaletteColorParams,
    ) -> Result<String, String> {
        validate_hex_color(&params.color)
            .map_err(|err| live_error("invalid_color", &format!("invalid color: {}", err), None))?;
        self.command(
            "set_palette_color",
            None,
            Some(json_strip_nulls(json!({
                "palette": params.palette,
                "index": params.index,
                "color": params.color,
            }))),
        )
        .await
    }

    pub async fn resize_palette(&self, params: LiveResizePaletteParams) -> Result<String, String> {
        if params.count == 0 {
            return Err(live_error(
                "invalid_palette_size",
                "count must be greater than zero",
                None,
            ));
        }
        self.command(
            "resize_palette",
            None,
            Some(json_strip_nulls(json!({
                "palette": params.palette,
                "count": params.count,
            }))),
        )
        .await
    }

    pub async fn run_app_command(&self, params: LiveRunAppCommandParams) -> Result<String, String> {
        validate_identifier("name", &params.name)?;
        self.command(
            "run_app_command",
            None,
            Some(json_strip_nulls(json!({
                "name": params.name,
                "params": params.params,
            }))),
        )
        .await
    }

    // --- SPEC-003 tilemap / tileset / autotile ---

    pub async fn create_tilemap_layer(
        &self,
        params: LiveCreateTilemapLayerParams,
    ) -> Result<String, String> {
        validate_non_empty("name", &params.name)?;
        let tw = params.tile_width.unwrap_or(16);
        let th = params.tile_height.unwrap_or(tw);
        if tw == 0 || th == 0 {
            return Err(live_error(
                "invalid_tile_size",
                "tile_width and tile_height must be greater than zero",
                None,
            ));
        }
        self.command(
            "create_tilemap_layer",
            None,
            Some(json!({
                "name": params.name,
                "tileWidth": tw,
                "tileHeight": th,
            })),
        )
        .await
    }

    pub async fn list_tilesets(&self) -> Result<String, String> {
        self.command("list_tilesets", None, None).await
    }

    pub async fn get_tileset(&self, params: LiveGetTilesetParams) -> Result<String, String> {
        if params.index.is_none() && params.layer.as_deref().unwrap_or("").is_empty() {
            return Err(live_error(
                "missing_tileset_selector",
                "index or layer is required",
                None,
            ));
        }
        if params.scale == Some(0) {
            return Err(live_error(
                "invalid_scale",
                "scale must be a positive integer (omit it for an automatic factor)",
                None,
            ));
        }

        // No preview requested: return tileset metadata only.
        let Some(ref filename) = params.filename else {
            return self
                .command(
                    "get_tileset",
                    None,
                    Some(json_strip_nulls(json!({
                        "index": params.index,
                        "layer": params.layer,
                    }))),
                )
                .await;
        };

        validate_non_empty("filename", filename)?;
        // Dump a raw packed PNG to the system temp dir, then nearest-neighbor
        // upscale it in-process (same vision-legibility path as save_preview) so
        // the live document is never touched and no plugin redeploy is needed.
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_tileset_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_str = temp.to_string_lossy().to_string();
        let raw = self
            .command(
                "get_tileset",
                None,
                Some(json_strip_nulls(json!({
                    "index": params.index,
                    "layer": params.layer,
                    "dumpPath": temp_str,
                }))),
            )
            .await?;

        let info =
            crate::preview::render_preview(&temp, std::path::Path::new(filename), params.scale);
        let _ = std::fs::remove_file(&temp);
        let info = info.map_err(|e| live_error("preview_render_failed", &e, None))?;

        // Merge the tileset metadata with the rendered preview geometry so the
        // agent gets both the tile data and the preview's pixel mapping.
        let mut meta: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
        if let Value::Object(ref mut map) = meta {
            map.insert(
                "preview".into(),
                json!({
                    "filename": filename,
                    "scale": info.scale,
                    "width": info.preview_width,
                    "height": info.preview_height,
                    "sourceWidth": info.source_width,
                    "sourceHeight": info.source_height,
                }),
            );
        }
        Ok(meta.to_string())
    }

    pub async fn stamp_tiles(&self, params: LiveStampTilesParams) -> Result<String, String> {
        validate_non_empty("layer", &params.layer)?;
        if params.tiles.is_empty() {
            return Err(live_error("invalid_tiles", "tiles cannot be empty", None));
        }
        // Remap to the camelCase the Lua handler reads (every live payload uses
        // camelCase keys; the nested struct is forwarded explicitly so its
        // snake_case `tile_index` does not leak onto the wire as `tile_index`).
        let tiles: Vec<Value> = params
            .tiles
            .iter()
            .map(|t| json!({ "x": t.x, "y": t.y, "tileIndex": t.tile_index }))
            .collect();
        let target = live_target(Some(params.layer), params.frame);
        self.command("stamp_tiles", Some(target), Some(json!({ "tiles": tiles })))
            .await
    }

    pub async fn set_tile_data(&self, params: LiveSetTileDataParams) -> Result<String, String> {
        if params.tileset_index.is_none() && params.layer.as_deref().unwrap_or("").is_empty() {
            return Err(live_error(
                "missing_tileset_selector",
                "tileset_index or layer is required",
                None,
            ));
        }
        self.command(
            "set_tile_data",
            None,
            Some(json_strip_nulls(json!({
                "tileIndex": params.tile_index,
                "tilesetIndex": params.tileset_index,
                "layer": params.layer,
                "data": params.data,
            }))),
        )
        .await
    }

    pub async fn pack_similar_tiles(
        &self,
        params: LivePackSimilarTilesParams,
    ) -> Result<String, String> {
        let tw = params.tile_width;
        let th = params.tile_height.unwrap_or(tw);
        if tw == 0 || th == 0 {
            return Err(live_error(
                "invalid_tile_size",
                "tile_width and tile_height must be greater than zero",
                None,
            ));
        }
        self.command(
            "pack_similar_tiles",
            None,
            Some(json_strip_nulls(json!({
                "tileWidth": tw,
                "tileHeight": th,
                "layer": params.layer,
                "tilemapLayer": params.tilemap_layer,
            }))),
        )
        .await
    }

    /// Export the active tilemap to an engine file. The plugin reads the tilemap
    /// grid + packs the tileset PNG to a sibling path; the engine-format bytes
    /// are then serialized by the pure, unit-tested `tileset_export` module and
    /// written here, so every format rule is testable without Aseprite.
    pub async fn export_tileset(
        &self,
        params: LiveExportTilesetParams,
    ) -> Result<String, String> {
        let target = crate::tileset_export::Target::parse(&params.target)
            .map_err(|e| live_error("invalid_target", &e, None))?;
        let layout = crate::tileset_export::Layout::parse(params.layout.as_deref().unwrap_or("none"))
            .map_err(|e| live_error("invalid_layout", &e, None))?;
        validate_non_empty("path", &params.path)?;
        if params.image_columns == Some(0) {
            return Err(live_error(
                "invalid_image_columns",
                "image_columns must be a positive integer (omit it for an automatic pack)",
                None,
            ));
        }

        // Honor an explicit extension; supply the target's canonical one when the
        // path has none (so `path="level", target=tiled` writes `level.tsj`).
        let out_path = {
            let p = std::path::PathBuf::from(&params.path);
            if p.extension().is_none() {
                p.with_extension(target.extension())
            } else {
                p
            }
        };
        let png_path = out_path.with_extension("png");
        let png_str = png_path.to_string_lossy().to_string();
        let image_name = png_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "tileset.png".to_string());

        let raw = self
            .command(
                "export_tilemap",
                Some(live_target(params.layer.clone(), params.frame)),
                Some(json_strip_nulls(json!({
                    "imagePath": png_str,
                    "imageColumns": params.image_columns,
                }))),
            )
            .await?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| {
            live_error(
                "bad_export_payload",
                &format!("could not parse tilemap export data: {e}"),
                None,
            )
        })?;

        let export = build_tilemap_export(&value, &params.layer, image_name, layout)?;
        let bytes = crate::tileset_export::serialize(&export, target)
            .map_err(|e| live_error("export_serialize_failed", &e, None))?;
        std::fs::write(&out_path, &bytes).map_err(|e| {
            live_error(
                "export_write_failed",
                &format!("could not write {}: {e}", out_path.display()),
                None,
            )
        })?;

        Ok(json!({
            "changed": true,
            "target": params.target,
            "path": out_path.to_string_lossy(),
            "image": png_path.to_string_lossy(),
            "tileCount": export.tile_count,
            "columns": export.columns,
            "rows": export.rows,
            "layout": params.layout.unwrap_or_else(|| "none".to_string()),
        })
        .to_string())
    }

    // --- SPEC-004 constrained / semantic colour ops ---

    /// Fetch a region's unique colours + the sprite palette (both as hex). Also
    /// serves as a palette-only read — the palette comes back even when the layer
    /// has no cel or is not a regular image layer.
    async fn fetch_region_colors(
        &self,
        layer: Option<String>,
        frame: Option<u32>,
        selection_only: Option<bool>,
    ) -> Result<(Vec<Rgba>, Vec<Rgba>, bool), String> {
        let raw = self
            .command(
                "get_region_colors",
                Some(live_target(layer, frame)),
                Some(json_strip_nulls(json!({ "selectionOnly": selection_only }))),
            )
            .await?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| {
            live_error(
                "bad_region_payload",
                &format!("could not parse region colours: {e}"),
                None,
            )
        })?;
        // `imageLayer` is false for a group/tilemap target; default true if the
        // field is absent so a missing flag never spuriously blocks.
        let image_layer = value
            .get("imageLayer")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        Ok((hex_array(&value, "colors"), hex_array(&value, "palette"), image_layer))
    }

    /// Apply a colour→colour map to the region in one undoable transaction.
    async fn apply_color_map(
        &self,
        layer: Option<String>,
        frame: Option<u32>,
        selection_only: Option<bool>,
        map: &[(Rgba, Rgba)],
    ) -> Result<Value, String> {
        let map_json: Vec<Value> = map
            .iter()
            .map(|(f, t)| json!({ "from": f.to_hex(), "to": t.to_hex() }))
            .collect();
        let raw = self
            .command(
                "apply_color_map",
                Some(live_target(layer, frame)),
                Some(json_strip_nulls(
                    json!({ "selectionOnly": selection_only, "map": map_json }),
                )),
            )
            .await?;
        Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!({})))
    }

    pub async fn palette_snap(&self, params: LivePaletteSnapParams) -> Result<String, String> {
        let (colors, palette, image_layer) = self
            .fetch_region_colors(params.layer.clone(), params.frame, params.selection_only)
            .await?;
        if !image_layer {
            return Err(live_error(
                "not_an_image_layer",
                "colour ops need a regular image layer (not a group or tilemap)",
                None,
            ));
        }
        if palette.is_empty() {
            return Err(live_error("empty_palette", "the sprite has no palette to snap to", None));
        }
        let map = color_ops::build_color_map(&colors, ColorOp::Snap, &palette, true);
        if map.is_empty() {
            return Ok(json!({
                "changed": false,
                "snappedColors": 0,
                "uniqueColors": colors.len(),
            })
            .to_string());
        }
        let res = self
            .apply_color_map(params.layer, params.frame, params.selection_only, &map)
            .await?;
        Ok(json!({
            "changed": true,
            "snappedColors": map.len(),
            "uniqueColors": colors.len(),
            "pixels": res.get("pixels").cloned().unwrap_or(Value::Null),
            "mapping": map
                .iter()
                .map(|(f, t)| json!({ "from": f.to_hex(), "to": t.to_hex() }))
                .collect::<Vec<_>>(),
        })
        .to_string())
    }

    pub async fn adjust_pixels(&self, params: LiveAdjustPixelsParams) -> Result<String, String> {
        let op = ColorOp::parse(&params.op, params.amount.unwrap_or(0.0), params.hue)
            .map_err(|e| live_error("invalid_op", &e, None))?;
        let clamp = params.clamp_to_palette.unwrap_or(true);
        let (colors, palette, image_layer) = self
            .fetch_region_colors(params.layer.clone(), params.frame, params.selection_only)
            .await?;
        if !image_layer {
            return Err(live_error(
                "not_an_image_layer",
                "colour ops need a regular image layer (not a group or tilemap)",
                None,
            ));
        }
        // `snap` is always palette-clamped (the clamp flag does not apply to it),
        // so it needs a palette regardless of clamp_to_palette.
        if (clamp || op == ColorOp::Snap) && palette.is_empty() {
            return Err(live_error(
                "empty_palette",
                "this op needs a palette; add one or pass clamp_to_palette=false (non-snap ops)",
                None,
            ));
        }
        let map = color_ops::build_color_map(&colors, op, &palette, clamp);
        if map.is_empty() {
            return Ok(json!({
                "changed": false,
                "changedColors": 0,
                "uniqueColors": colors.len(),
            })
            .to_string());
        }
        let res = self
            .apply_color_map(params.layer, params.frame, params.selection_only, &map)
            .await?;
        Ok(json!({
            "changed": true,
            "op": params.op,
            "changedColors": map.len(),
            "uniqueColors": colors.len(),
            "pixels": res.get("pixels").cloned().unwrap_or(Value::Null),
        })
        .to_string())
    }

    pub async fn snap_colors(&self, params: LiveSnapColorsParams) -> Result<String, String> {
        if params.colors.is_empty() {
            return Err(live_error("invalid_colors", "colors cannot be empty", None));
        }
        let mut inputs = Vec::with_capacity(params.colors.len());
        for c in &params.colors {
            let rgba = Rgba::from_hex(c).map_err(|e| {
                live_error("invalid_color", &format!("invalid colour '{c}': {e}"), None)
            })?;
            inputs.push((c.clone(), rgba));
        }
        // The palette is layer-independent — read it via get_region_colors.
        let (_unused, palette, _image_layer) = self.fetch_region_colors(None, None, Some(false)).await?;
        if palette.is_empty() {
            return Err(live_error("empty_palette", "the sprite has no palette to snap to", None));
        }
        let snapped: Vec<Value> = inputs
            .iter()
            .map(|(orig, rgba)| {
                let s = color_ops::clamp_to_palette(*rgba, &palette);
                json!({ "input": orig, "snapped": s.to_hex() })
            })
            .collect();
        Ok(json!({ "snapped": snapped, "paletteSize": palette.len() }).to_string())
    }

    async fn command(
        &self,
        kind: &str,
        target: Option<Value>,
        payload: Option<Value>,
    ) -> Result<String, String> {
        let _guard = self.command_lock.lock().await;
        let id = format!("live-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let request = LiveRequest {
            protocol: LIVE_PROTOCOL,
            version: LIVE_VERSION,
            id: id.clone(),
            kind: kind.to_string(),
            target,
            payload,
        };

        let message = serde_json::to_string(&request)
            .map_err(|err| format!("failed to serialize live request: {}", err))?;

        // Refuse loudly (never silently batch-fallback) unless the control link
        // to the bridge is up AND the bridge reports a plugin connected.
        let control = self.sender.read().await.clone();
        let plugin_up = self.plugin_connected.load(Ordering::Relaxed);
        let sender = match (control, plugin_up) {
            (Some(sender), true) => sender,
            _ => {
                return Err(live_error(
                    "live_not_connected",
                    LIVE_DISCONNECTED_HINT,
                    Some(json!({
                        "connected": false,
                        "host": "127.0.0.1",
                        "port": self.port,
                        "controlPort": self.control_port,
                        "doNotFallBackToBatch": true,
                    })),
                ));
            }
        };

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        if sender.send(Message::Text(message.into())).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(live_error(
                "live_connection_lost",
                "failed to send live request to Aseprite",
                None,
            ));
        }

        let response =
            match tokio::time::timeout(Duration::from_millis(request_timeout_ms()), rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => {
                    return Err(live_error(
                        "live_connection_lost",
                        "live response channel closed",
                        None,
                    ))
                }
                Err(_) => {
                    self.pending.lock().await.remove(&id);
                    return Err(live_error(
                        "live_timeout",
                        "timed out waiting for Aseprite live response",
                        None,
                    ));
                }
            };

        if response.ok.unwrap_or(false) {
            Ok(
                serde_json::to_string(&response.result.unwrap_or_else(|| json!({})))
                    .unwrap_or_else(|_| "{}".to_string()),
            )
        } else {
            Err(serde_json::to_string(&response.error.unwrap_or_else(
                || json!({ "code": "live_error", "message": "Aseprite returned an error" }),
            ))
            .unwrap_or_else(|_| "Aseprite returned an error".to_string()))
        }
    }

    /// Maintain the control connection to the standalone bridge. If the bridge is
    /// not running, spawn it (spawn-if-absent), then keep reconnecting with
    /// capped backoff so MCP/bridge restarts self-heal without manual steps.
    async fn run_client_loop(self: Arc<Self>) {
        let mut backoff = Duration::from_millis(250);
        let max_backoff = Duration::from_secs(3);
        loop {
            match self.connect_once().await {
                Ok(()) => {
                    // Clean disconnect from the bridge; reconnect promptly.
                    backoff = Duration::from_millis(250);
                }
                Err(err) => {
                    warn!("live control link to bridge unavailable: {}", err);
                    // The bridge is probably not running yet — try to start it.
                    self.spawn_bridge();
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
            // Whatever happened, we are now disconnected: reset state so callers
            // see ready=false and pending requests don't hang forever.
            *self.sender.write().await = None;
            self.plugin_connected.store(false, Ordering::Relaxed);
            self.pending.lock().await.clear();
            tokio::time::sleep(backoff).await;
        }
    }

    /// Connect to the bridge control port and pump frames until the link drops.
    async fn connect_once(self: &Arc<Self>) -> anyhow::Result<()> {
        let url = format!("ws://127.0.0.1:{}/", self.control_port);
        let (ws, _resp) = connect_async(url).await?;
        info!(
            "live control link established to bridge on port {}",
            self.control_port
        );
        let (mut ws_write, mut ws_read) = ws.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        *self.sender.write().await = Some(tx);

        let writer = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if ws_write.send(message).await.is_err() {
                    break;
                }
            }
        });

        while let Some(message) = ws_read.next().await {
            let message = message?;
            if let Message::Text(text) = message {
                self.handle_text(text.to_string()).await;
            }
        }

        writer.abort();
        Ok(())
    }

    /// Start the standalone bridge binary (a sibling of the current executable).
    /// Idempotent in effect: if a bridge is already running it will lose the port
    /// race and exit, so spawning again is harmless.
    fn spawn_bridge(&self) {
        let Ok(exe) = std::env::current_exe() else {
            warn!("cannot locate current exe to spawn the live bridge");
            return;
        };
        let Some(dir) = exe.parent() else {
            return;
        };
        let bin_name = if cfg!(windows) {
            "aseprite-live-bridge.exe"
        } else {
            "aseprite-live-bridge"
        };
        let bridge_path = dir.join(bin_name);
        if !bridge_path.exists() {
            warn!("live bridge binary not found at {}", bridge_path.display());
            return;
        }

        let mut cmd = std::process::Command::new(&bridge_path);
        cmd.env("ASEPRITE_MCP_LIVE_PORT", self.port.to_string())
            .env("ASEPRITE_MCP_LIVE_CONTROL_PORT", self.control_port.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // On Windows, detach so the bridge survives this MCP process exiting
        // (that persistence is what keeps the plugin connected across restarts).
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
            cmd.creation_flags(
                DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB,
            );
            if cmd.spawn().is_ok() {
                info!("spawned standalone live bridge: {}", bridge_path.display());
                return;
            }
            // Some job objects forbid breakaway; retry without that flag.
            let mut fallback = std::process::Command::new(&bridge_path);
            fallback
                .env("ASEPRITE_MCP_LIVE_PORT", self.port.to_string())
                .env("ASEPRITE_MCP_LIVE_CONTROL_PORT", self.control_port.to_string())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            match fallback.spawn() {
                Ok(_) => info!("spawned standalone live bridge: {}", bridge_path.display()),
                Err(err) => warn!("failed to spawn live bridge: {}", err),
            }
            return;
        }

        #[cfg(not(windows))]
        match cmd.spawn() {
            Ok(_) => info!("spawned standalone live bridge: {}", bridge_path.display()),
            Err(err) => warn!("failed to spawn live bridge: {}", err),
        }
    }

    async fn handle_text(&self, text: String) {
        // The bridge pushes state frames (plugin presence + last hello) outside
        // the request/response flow; handle those first.
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if value.get("type").and_then(|t| t.as_str()) == Some("bridge_state") {
                let plugin_connected = value
                    .get("pluginConnected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.plugin_connected
                    .store(plugin_connected, Ordering::Relaxed);
                *self.last_hello.write().await =
                    value.get("lastHello").cloned().filter(|v| !v.is_null());
                return;
            }
        }

        match serde_json::from_str::<LiveResponse>(&text) {
            Ok(response) => {
                if response.kind.as_deref() == Some("hello") {
                    *self.last_hello.write().await =
                        Some(response.result.clone().unwrap_or_else(|| json!({})));
                    return;
                }

                if let Some(id) = response.id.clone() {
                    if let Some(tx) = self.pending.lock().await.remove(&id) {
                        let _ = tx.send(response);
                    } else {
                        warn!("Received live response for unknown id: {}", id);
                    }
                } else {
                    warn!("Received live message without id: {}", text);
                }
            }
            Err(err) => warn!("Invalid live JSON from Aseprite: {}; body={}", err, text),
        }
    }
}

fn live_target(layer: Option<String>, frame: Option<u32>) -> Value {
    json!({
        "layer": layer.unwrap_or_else(default_layer),
        "frame": frame.map_or_else(|| json!("active"), |value| json!(value)),
    })
}

fn default_layer() -> String {
    "AI Draft".to_string()
}

/// Parse a JSON array of hex-colour strings (under `key`) into `Rgba`s, dropping
/// any that fail to parse. Used by the SPEC-004 colour ops.
fn hex_array(value: &Value, key: &str) -> Vec<Rgba> {
    value
        .get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str())
                .filter_map(|s| Rgba::from_hex(s).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build a [`crate::tileset_export::TilemapExport`] from the plugin's
/// `export_tilemap` result. Tolerant of missing optional fields (they default to
/// empty); `TilemapExport::validate` (run inside `serialize`) is the real guard
/// against a malformed grid.
fn build_tilemap_export(
    value: &Value,
    layer: &Option<String>,
    image_name: String,
    layout: crate::tileset_export::Layout,
) -> Result<crate::tileset_export::TilemapExport, String> {
    let as_u32 = |k: &str| value.get(k).and_then(|x| x.as_u64()).map(|n| n as u32);
    let tile_width = as_u32("tileWidth").ok_or_else(|| {
        live_error("bad_export_payload", "missing tileWidth in export data", None)
    })?;
    let tile_height = as_u32("tileHeight").ok_or_else(|| {
        live_error("bad_export_payload", "missing tileHeight in export data", None)
    })?;
    let tile_count = as_u32("tileCount").unwrap_or(0);
    let columns = as_u32("columns").unwrap_or(0);
    let rows = as_u32("rows").unwrap_or(0);
    let image_columns = as_u32("imageColumns").unwrap_or_else(|| tile_count.max(1));
    let grid = value
        .get("grid")
        .and_then(|g| g.as_array())
        .map(|rows_arr| {
            rows_arr
                .iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| cells.iter().map(|c| c.as_i64().unwrap_or(0)).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let layer_name = value
        .get("layer")
        .and_then(|x| x.as_str())
        .map(String::from)
        .or_else(|| layer.clone())
        .unwrap_or_else(|| "Tilemap".to_string());

    Ok(crate::tileset_export::TilemapExport {
        layer_name,
        tile_width,
        tile_height,
        columns,
        rows,
        tile_count,
        image_name,
        image_columns,
        grid,
        layout,
    })
}

fn json_strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let value = json_strip_nulls(value);
                    if value.is_null() {
                        None
                    } else {
                        Some((key, value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(json_strip_nulls).collect()),
        value => value,
    }
}

fn live_error(code: &str, message: &str, details: Option<Value>) -> String {
    serde_json::to_string(&json_strip_nulls(json!({
        "code": code,
        "message": message,
        "details": details,
    })))
    .unwrap_or_else(|_| message.to_string())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(live_error(
            "missing_field",
            &format!("{} must be a non-empty string", field),
            Some(json!({ "field": field })),
        ));
    }
    Ok(())
}

fn validate_frame(frame: u32) -> Result<(), String> {
    if frame == 0 {
        return Err(live_error(
            "invalid_frame",
            "frame must be a 1-based positive integer",
            None,
        ));
    }
    Ok(())
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(live_error(
            "missing_field",
            &format!("{} is required", field),
            None,
        ));
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(live_error(
            "invalid_identifier",
            &format!("{} must start with a letter or underscore", field),
            Some(json!({ "field": field })),
        ));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        return Err(live_error(
            "invalid_identifier",
            &format!(
                "{} must contain only letters, numbers, and underscores",
                field
            ),
            Some(json!({ "field": field })),
        ));
    }
    Ok(())
}

/// Validate `live_save_preview` inputs without touching the bridge, so the guard
/// is unit-testable: an empty filename or an explicit `scale` of 0 is rejected.
fn validate_preview_request(filename: &str, scale: Option<u32>) -> Result<(), String> {
    validate_non_empty("filename", filename)?;
    if scale == Some(0) {
        return Err(live_error(
            "invalid_scale",
            "scale must be a positive integer (omit it for an automatic factor)",
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_request_rejects_empty_filename_and_zero_scale() {
        assert!(validate_preview_request("", None).is_err());
        assert!(validate_preview_request("preview.png", Some(0)).is_err());
        assert!(validate_preview_request("preview.png", None).is_ok());
        assert!(validate_preview_request("preview.png", Some(8)).is_ok());
        let err = validate_preview_request("preview.png", Some(0)).unwrap_err();
        assert!(err.contains("invalid_scale"), "got: {err}");
    }

    #[test]
    fn strips_null_fields_recursively() {
        let value = json_strip_nulls(json!({
            "keep": true,
            "drop": null,
            "nested": {
                "keep": 1,
                "drop": null
            },
            "array": [
                { "drop": null, "keep": "x" }
            ]
        }));

        assert_eq!(value["keep"], json!(true));
        assert!(value.get("drop").is_none());
        assert_eq!(value["nested"]["keep"], json!(1));
        assert!(value["nested"].get("drop").is_none());
        assert_eq!(value["array"][0]["keep"], json!("x"));
        assert!(value["array"][0].get("drop").is_none());
    }

    #[test]
    fn live_error_uses_protocol_error_shape() {
        let parsed: Value = serde_json::from_str(&live_error(
            "live_timeout",
            "timed out",
            Some(json!({ "id": "live-1" })),
        ))
        .unwrap();

        assert_eq!(parsed["code"], json!("live_timeout"));
        assert_eq!(parsed["message"], json!("timed out"));
        assert_eq!(parsed["details"]["id"], json!("live-1"));
    }

    #[test]
    fn build_tilemap_export_maps_plugin_payload_and_serializes() {
        // A representative `export_tilemap` result from the Lua handler.
        let payload = json!({
            "layer": "Ground",
            "tileWidth": 16, "tileHeight": 16,
            "columns": 2, "rows": 2,
            "tileCount": 4,
            "imageColumns": 2,
            "grid": [[0, 1], [2, 3]],
        });
        let e = build_tilemap_export(
            &payload,
            &None,
            "level.png".to_string(),
            crate::tileset_export::Layout::Blob47,
        )
        .expect("well-formed payload builds an export");
        assert_eq!(e.layer_name, "Ground");
        assert_eq!(e.tile_width, 16);
        assert_eq!((e.columns, e.rows, e.tile_count), (2, 2, 4));
        assert_eq!(e.grid, vec![vec![0_i64, 1], vec![2, 3]]);
        // The boundary output must serialize for every engine target.
        for t in [
            crate::tileset_export::Target::Json,
            crate::tileset_export::Target::Tiled,
            crate::tileset_export::Target::Godot,
        ] {
            assert!(crate::tileset_export::serialize(&e, t).is_ok(), "target {t:?}");
        }
    }

    #[test]
    fn build_tilemap_export_falls_back_to_param_layer_and_rejects_missing_dims() {
        // Missing tileWidth/tileHeight is a loud error (malformed plugin payload).
        let bad = json!({ "columns": 2, "rows": 0, "grid": [] });
        assert!(build_tilemap_export(
            &bad,
            &Some("Walls".to_string()),
            "x.png".to_string(),
            crate::tileset_export::Layout::None,
        )
        .is_err());
        // With dims present and no `layer` in the payload, the param layer is used.
        let ok = json!({ "tileWidth": 8, "tileHeight": 8 });
        let e = build_tilemap_export(
            &ok,
            &Some("Walls".to_string()),
            "x.png".to_string(),
            crate::tileset_export::Layout::None,
        )
        .unwrap();
        assert_eq!(e.layer_name, "Walls");
    }

    #[test]
    fn validates_app_command_identifier() {
        assert!(validate_identifier("name", "SaveFile").is_ok());
        assert!(validate_identifier("name", "_Custom1").is_ok());
        assert!(validate_identifier("name", "Save File").is_err());
        assert!(validate_identifier("name", "1SaveFile").is_err());
    }

    fn disconnected_bridge() -> LiveBridge {
        LiveBridge {
            port: DEFAULT_PORT,
            control_port: DEFAULT_PORT + 1,
            sender: Arc::new(RwLock::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            last_hello: Arc::new(RwLock::new(None)),
            plugin_connected: Arc::new(AtomicBool::new(false)),
            next_id: Arc::new(AtomicU64::new(1)),
            command_lock: Arc::new(Mutex::new(())),
        }
    }

    #[tokio::test]
    async fn preflight_blocks_when_disconnected() {
        let bridge = disconnected_bridge();
        let parsed: Value = serde_json::from_str(&bridge.preflight().await).unwrap();
        assert_eq!(parsed["ready"], json!(false));
        assert_eq!(parsed["connected"], json!(false));
        assert!(parsed["directive"]
            .as_str()
            .unwrap()
            .starts_with("BLOCKED"));
        assert!(!parsed["remediation"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn live_command_refuses_loudly_when_disconnected() {
        let bridge = disconnected_bridge();
        let err = bridge
            .command("draw_pixels", None, None)
            .await
            .expect_err("must refuse when disconnected");
        let parsed: Value = serde_json::from_str(&err).unwrap();
        assert_eq!(parsed["code"], json!("live_not_connected"));
        assert_eq!(parsed["details"]["doNotFallBackToBatch"], json!(true));
    }

    // --- Tool JSON-Schema contract tests (checklist 2.3, 9.5) ---
    // Guards against the boolean-schema bug class: a `serde_json::Value` field
    // (e.g. run_app_command `params`) generating a `true`/`false` property schema
    // that strict MCP clients reject during tools/list. Every param type's schema
    // must be an object whose `properties` are object schemas, recursively.

    fn assert_property_schemas(value: &Value, name: &str) {
        let Some(obj) = value.as_object() else {
            panic!("{name}: schema must be an object, got {value}");
        };
        // `properties` values must be object schemas (never booleans). We do NOT
        // inspect `additionalProperties`, where a boolean is valid JSON Schema.
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            for (key, prop) in props {
                assert!(
                    !prop.is_boolean(),
                    "{name}.{key}: property must not be a boolean schema (strict MCP clients reject it)"
                );
                assert!(
                    prop.is_object(),
                    "{name}.{key}: property schema must be an object"
                );
                assert_property_schemas(prop, &format!("{name}.{key}"));
            }
        }
        // Array item schemas must also be objects (not booleans).
        if let Some(items) = obj.get("items") {
            assert!(!items.is_boolean(), "{name}.items: must not be a boolean schema");
        }
    }

    macro_rules! assert_object_schemas {
        ($($t:ty),+ $(,)?) => {
            $(
                let schema = serde_json::to_value(schemars::schema_for!($t))
                    .expect("schema serializes to JSON");
                assert_property_schemas(&schema, stringify!($t));
            )+
        };
    }

    #[test]
    fn all_tool_param_schemas_are_valid_objects() {
        assert_object_schemas!(
            LiveLayerParams,
            LiveEnsureFramesParams,
            LiveClearCelParams,
            LiveDrawPixelsParams,
            LivePixel,
            LiveUseToolParams,
            LivePoint,
            LiveEmptyParams,
            LiveSpriteSelectorParams,
            LiveOpenSpriteParams,
            LiveSaveSpriteAsParams,
            LiveSavePreviewParams,
            LiveAsciiViewParams,
            LiveSaveFilmstripParams,
            LiveCloseSpriteParams,
            LiveResizeCanvasParams,
            LiveRect,
            LiveSize,
            LiveSpritePropertiesParams,
            LiveLayerNameParams,
            LiveRenameLayerParams,
            LiveCreateGroupLayerParams,
            LiveSetLayerVisibilityParams,
            LiveSetLayerPropertiesParams,
            LiveFrameSelectorParams,
            LiveSetFramePropertiesParams,
            LiveNewEmptyFrameParams,
            LiveNewFrameParams,
            LiveListCelsParams,
            LiveNewCelParams,
            LiveSetCelPropertiesParams,
            LiveDeleteCelParams,
            LiveTagNameParams,
            LiveNewTagParams,
            LiveSetTagPropertiesParams,
            LivePointPayload,
            LiveSliceCenter,
            LiveNewSliceParams,
            LiveSetSlicePropertiesParams,
            LiveSetSelectionParams,
            LiveListPaletteParams,
            LiveSetPaletteColorParams,
            LiveResizePaletteParams,
            LiveRunAppCommandParams,
            LiveCreateTilemapLayerParams,
            LiveGetTilesetParams,
            LiveTile,
            LiveStampTilesParams,
            LiveSetTileDataParams,
            LivePackSimilarTilesParams,
            LiveExportTilesetParams,
            LivePaletteSnapParams,
            LiveAdjustPixelsParams,
            LiveSnapColorsParams,
        );
    }

    #[test]
    fn request_timeout_is_env_tunable_with_safe_floor() {
        // Serialized in one test: Rust tests share process env across threads.
        std::env::remove_var("ASEPRITE_MCP_LIVE_TIMEOUT_MS");
        assert_eq!(request_timeout_ms(), DEFAULT_REQUEST_TIMEOUT_MS);

        std::env::set_var("ASEPRITE_MCP_LIVE_TIMEOUT_MS", "90000");
        assert_eq!(request_timeout_ms(), 90_000);

        // Exact floor boundary is accepted.
        std::env::set_var("ASEPRITE_MCP_LIVE_TIMEOUT_MS", "1000");
        assert_eq!(request_timeout_ms(), MIN_REQUEST_TIMEOUT_MS);

        // Garbage and dangerously-small values fall back to the default.
        for bad in ["abc", "", "0", "999", "-5"] {
            std::env::set_var("ASEPRITE_MCP_LIVE_TIMEOUT_MS", bad);
            assert_eq!(request_timeout_ms(), DEFAULT_REQUEST_TIMEOUT_MS, "value: {bad:?}");
        }
        std::env::remove_var("ASEPRITE_MCP_LIVE_TIMEOUT_MS");
    }

    #[test]
    fn run_app_command_params_field_is_object_not_boolean() {
        // The specific regression: `params: Option<Value>` must serialize to an
        // object schema thanks to the `#[schemars(with = ...)]` annotation.
        let schema = serde_json::to_value(schemars::schema_for!(LiveRunAppCommandParams)).unwrap();
        let params = &schema["properties"]["params"];
        assert!(
            params.is_object(),
            "run_app_command `params` must be an object schema, got {params}"
        );
    }
}
