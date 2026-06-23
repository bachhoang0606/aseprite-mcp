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
pub struct LiveImportReferenceParams {
    /// Path to a **PNG** reference image on disk (convert other formats to PNG first).
    pub filename: String,
    /// Target grid width in pixels. Omit to use the active sprite's width.
    pub width: Option<u32>,
    /// Target grid height in pixels. Omit to use the active sprite's height.
    pub height: Option<u32>,
    /// Downscale method: "dominant" (per-cell majority, edge-preserving — default) or
    /// "average" (per-cell mean of opaque pixels).
    pub method: Option<String>,
    /// Explicit palette to snap to (a list of `#rrggbb`). Omit to snap to the active
    /// sprite palette.
    pub palette: Option<Vec<String>>,
    /// Set `false` to skip palette snapping and keep the downscaled source colours.
    pub snap: Option<bool>,
    /// Auto-extract an N-colour palette from the reference and snap to it — so you can import a
    /// photo / AI render without supplying a palette. 1..=256, mutually exclusive with `palette`
    /// (and with `snap:false`). Sampled from the **raw source** (before any regrid). The extracted
    /// palette is returned in the summary (`auto_palette`) so you can lock it on the sprite (e.g.
    /// via `/pixel-palette`). On RGB sprites the import is on-model immediately.
    pub auto_colors: Option<u32>,
    /// Extraction method for `auto_colors`: "median_cut" (default), "kmeans", or "frequency".
    /// `median_cut`/`kmeans` are **area-weighted** (a large flat background can crowd out small
    /// bright colours); prefer **`frequency`** for art that is already limited-palette or an integer
    /// upscale. Only used when `auto_colors` is set.
    pub palette_method: Option<String>,
    /// De-fake a *scaled* reference. When `true`, auto-detect the source's native pixel
    /// grid (e.g. a 1024×1024 image that is "really" 64×64 upscaled 16×) and recover it to
    /// 1× before snapping, so the import lands on the true pixel grid. If `width`/`height`
    /// are omitted, the target defaults to the detected native resolution (instead of the
    /// active sprite's size). When no upscale is detected (native art / a photo), this is a
    /// no-op and the usual sizing applies. Defaults to `false`.
    pub regrid: Option<bool>,
    /// Target layer for the imported pixels. Defaults to "Reference".
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
    /// Top-left placement of the import on the canvas (default 0,0).
    pub at_x: Option<i32>,
    pub at_y: Option<i32>,
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
    /// Overlay a labelled coordinate gutter (numeric ticks along the top and left)
    /// so the agent can read off and name the exact source (x,y) of a pixel to fix.
    /// Defaults on whenever the tick spacing is legible at the chosen scale; pass
    /// `false` to suppress it. Pass `true` to require it — that errors if the sprite
    /// is too large for a legible gutter (raise `scale` or crop first), whereas the
    /// default quietly falls back to a plain preview with a `gutter_skipped` note.
    pub gutter: Option<bool>,
    /// Source-px between gutter ticks (default 8). Ignored when `gutter` is `false`.
    pub gutter_step: Option<u32>,
    /// Region to preview, so the upscale budget lands on the subject (SPEC-005
    /// Phase 2): `"sprite"` (whole canvas, default), `"cel"` (the active cel's bbox —
    /// a small cel on a big canvas then fills ~1024px instead of a few), or an explicit
    /// `{ "x", "y", "width", "height" }` rectangle in sprite coordinates. The crop
    /// origin is reported back (`crop`) and the gutter labels read absolute sprite
    /// coordinates, so any (x,y) still inverts exactly.
    pub crop: Option<LiveCrop>,
    /// Also return the PNG as an inline image-content block (base64 `image/png`) so a
    /// vision client sees the pixels directly, not just a path (SPEC-005 Phase 3). The
    /// path is always present too. A preview larger than the byte ceiling degrades to
    /// path-only with a note, so a huge sheet can't blow the context budget.
    pub inline: Option<bool>,
    /// Overlay numbered Set-of-Mark badges on the preview and return a `marks`
    /// `[{n, region, bbox}]` map (SPEC-005 Phase 4): `"slices"` (one badge per named
    /// slice — best, authored), `"layers"` (one per visible layer's cel at the active
    /// frame), or `"components"` (one per connected opaque blob). The critic can then say
    /// "region 3 has a stray pixel" and the orchestrator maps `3 → that slice/layer/blob`
    /// — no fragile free-form coordinates.
    pub marks_from: Option<LiveMarksFrom>,
}

/// `crop` selector for `live_save_preview`: a mode string (`"cel"` / `"sprite"`) or
/// an explicit source-space rectangle.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum LiveCrop {
    /// `"cel"` (active cel bbox) or `"sprite"` (whole canvas — the default).
    Mode(String),
    /// Explicit pixel rectangle `{x, y, width, height}` in sprite coordinates.
    Rect(LiveRect),
}

/// Region source for Set-of-Mark numbered badges (SPEC-005 Phase 4). All three reuse
/// existing read-only plugin enumeration (`list_slices` / `list_cels`) or a pure-Rust
/// connected-component pass — no new plugin command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LiveMarksFrom {
    /// One badge per named slice (authored regions — most meaningful).
    Slices,
    /// One badge per visible layer's cel bbox at the active frame.
    Layers,
    /// One badge per 4-connected opaque blob (pure-Rust, no plugin read).
    Components,
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
pub struct LiveFrameDiffParams {
    /// 1-based source frame number.
    pub from_frame: u32,
    /// 1-based target frame number.
    pub to_frame: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveExtractStyleProfileParams {
    /// Palette size to extract (clamped 2..64). Defaults to 12.
    pub colors: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveDitherFillParams {
    /// Rectangle to fill, in sprite pixels.
    pub rect: LiveRect,
    /// Two `#rrggbb` colours to dither between (usually two adjacent ramp steps).
    pub color_a: String,
    pub color_b: String,
    /// Fraction of `color_b`, 0..1. Default 0.5.
    pub level: Option<f64>,
    /// Dither matrix: "bayer4" (default), "bayer2", or "checker".
    pub matrix: Option<String>,
    /// Target layer. Defaults to "AI Draft".
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveRotateParams {
    /// Rotation in degrees, **positive = clockwise**. Right angles (90/180/270) are
    /// exact; any other angle uses the RotSprite scale-rotate-downscale pipeline.
    pub angle: f64,
    /// Source rectangle to rotate, in sprite pixels. Omit to rotate the whole canvas
    /// (or the active selection's bounds when `selection_only` is true).
    pub rect: Option<LiveRect>,
    /// Use the active selection's bounding box as the source rect. Ignored if `rect` is
    /// given. Errors loudly if there is no selection.
    pub selection_only: Option<bool>,
    /// Layer to draw the rotated copy onto. Defaults to "Rotated" (a NEW layer — the
    /// source is left untouched; rotation reads the flattened render).
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
    /// Top-left placement of the rotated result on the canvas. Omit to centre the
    /// rotated bounding box on the source rectangle's centre (rotate "in place").
    pub at_x: Option<i32>,
    pub at_y: Option<i32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LiveCreateAutotileTemplateParams {
    /// Full tile edge in pixels — must be EVEN and in 4..=64 (each corner quarter is tile_size/2).
    pub tile_size: u32,
    /// Top-left of the four-quarter source strip the agent drew, laid left-to-right as
    /// `[fill | outer | edge | inner]` (each quarter `tile_size/2` square). Canonical orientation:
    /// `outer` = a CONVEX corner rounded at its TOP-LEFT, `edge` = a boundary along its TOP,
    /// `inner` = a CONCAVE notch at its TOP-LEFT — the compositor rotates these into all 4 quadrants
    /// of every tile. Defaults to 0,0.
    pub source_x: Option<u32>,
    pub source_y: Option<u32>,
    /// Top-left placement of the generated 47-tile sheet. Defaults to just right of the source strip.
    pub at_x: Option<i32>,
    pub at_y: Option<i32>,
    /// Autotile layout — only `"blob47"` is supported (wang16 is a follow-up). Defaults to `"blob47"`.
    pub layout: Option<String>,
    /// Target layer for the generated sheet. Defaults to "Autotile Template".
    pub layer: Option<String>,
    /// Target frame, 1-based. Omit to use the active frame.
    pub frame: Option<u32>,
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, schemars::JsonSchema)]
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
pub struct LiveGradientMapParams {
    /// Ramp to map onto, an ordered dark→light list of `#rrggbb` (e.g. a StyleProfile ramp).
    pub ramp: Vec<String>,
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
        // Resolve (and validate) the crop selector up front — an explicit bad rect or
        // an unknown mode fails fast, before the bridge round-trip.
        let plan = resolve_crop_plan(&params.crop)?;

        // 1) Render the active frame to a single-frame PNG in the system temp dir
        // (NOT the user's project tree), so a hard crash between save and cleanup
        // cannot leak a file into their repo; the id keeps concurrent previews
        // distinct. The plugin's `save_preview` renders the active frame into a
        // standalone Image and Image:saveAs's it — this is modal-free even on a
        // multi-frame sprite (ADR-0004), unlike `save_copy_as`/saveCopyAs which
        // pops Aseprite's "format doesn't support multiple frames" dialog. The same
        // response also carries the active cel's bounds (for crop="cel").
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_preview_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let temp_str = temp.to_string_lossy().to_string();
        let resp = self
            .command("save_preview", None, Some(json!({ "filename": temp_str })))
            .await?;

        // 2) Resolve the crop rect (crop="cel" needs the plugin's reported bounds), then
        //    upscale in-process to a buffer; clean up the temp regardless of outcome.
        let crop = match plan {
            CropPlan::Whole => None,
            CropPlan::Explicit(c) => Some(c),
            CropPlan::Cel => Some(cel_crop_from_response(&resp)?),
        };

        // 2b) Gather Set-of-Mark regions (SPEC-005 Phase 4). Slices/layers need read-only
        //     bridge enumeration (done here, before the temp is consumed); components are
        //     computed from the rendered buffer inside finish_preview. The active frame for
        //     the layer cels comes from the save_preview response.
        let marks = match params.marks_from {
            None => MarksInput::None,
            Some(LiveMarksFrom::Components) => MarksInput::Components,
            Some(LiveMarksFrom::Slices) => {
                MarksInput::Regions(parse_slice_regions(&self.list_slices().await?))
            }
            Some(LiveMarksFrom::Layers) => {
                let frame = active_frame_from_response(&resp);
                let cels = self
                    .list_cels(LiveListCelsParams { layer: None, frame: Some(frame) })
                    .await?;
                let layers = self.list_layers().await?;
                MarksInput::Regions(parse_layer_regions(&cels, &layers))
            }
        };

        let rendered = crate::preview::render_preview_buffer(&temp, params.scale, crop);
        let _ = std::fs::remove_file(&temp);
        let (buffer, info) = rendered.map_err(|e| live_error("preview_render_failed", &e, None))?;

        // 3) Optionally composite a labelled coordinate gutter + numbered marks, then
        //    write the PNG. Pure (buffer-in / path-out) so it is all unit-tested below.
        finish_preview(
            buffer,
            info,
            &params.filename,
            params.gutter,
            params.gutter_step,
            marks,
        )
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

    /// Render frame `frame` to a temp 1x PNG (sets it active) and return the image.
    /// Caller is responsible for restoring the original active frame.
    async fn render_frame(&self, frame: u32) -> Result<image::RgbaImage, String> {
        self.command("set_active_frame", None, Some(json!({ "frame": frame })))
            .await?;
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_framediff_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        let res = self
            .command("save_preview", None, Some(json!({ "filename": temp.to_string_lossy() })))
            .await
            .and_then(|_| {
                image::open(&temp)
                    .map(|im| im.to_rgba8())
                    .map_err(|e| live_error("frame_diff_decode_failed", &format!("{e}"), None))
            });
        let _ = std::fs::remove_file(&temp);
        res
    }

    /// SPEC-008: derive a machine-checkable `StyleProfile` from the active sprite — the
    /// native grid (de-fakes a scaled reference), palette, ramps + ramp-lint scores,
    /// light direction, head proportion, and outline policy (research §G). Renders a
    /// modal-free 1× copy and analyses it in pure Rust (`crate::style_profile`, the same
    /// algorithms the eval gates test); the open document is untouched.
    pub async fn extract_style_profile(
        &self,
        params: LiveExtractStyleProfileParams,
    ) -> Result<String, String> {
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_style_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        self.command("save_preview", None, Some(json!({ "filename": temp.to_string_lossy() })))
            .await?;
        let decoded = image::open(&temp)
            .map(|im| im.to_rgba8())
            .map_err(|e| live_error("style_profile_decode_failed", &format!("{e}"), None));
        let _ = std::fs::remove_file(&temp);
        let img = decoded?;
        let colors = params.colors.unwrap_or(12).clamp(2, 64);
        let profile = crate::style_profile::derive(&img, colors);
        serde_json::to_string(&profile)
            .map_err(|e| live_error("style_profile_serialize_failed", &format!("{e}"), None))
    }

    /// SPEC-009: ordered (Bayer) dither-fill a rectangle between two palette colours — a
    /// palette-legal-by-construction shading op (only the two inputs appear). Computes the
    /// pattern in pure Rust (`crate::dither`) and draws it via the existing `draw_pixels`
    /// path; no new plugin command.
    pub async fn dither_fill(&self, params: LiveDitherFillParams) -> Result<String, String> {
        let r = &params.rect;
        if r.width == 0 || r.height == 0 {
            return Err(live_error("empty_rect", "rect width and height must be > 0", None));
        }
        if (r.width as u64) * (r.height as u64) > MAX_DITHER_AREA {
            return Err(live_error(
                "rect_too_large",
                &format!("{}x{} exceeds the {MAX_DITHER_AREA}px dither cap (split it)", r.width, r.height),
                None,
            ));
        }
        let a = crate::color_ops::Rgba::from_hex(&params.color_a)
            .map_err(|e| live_error("invalid_color", &e, None))?;
        let b = crate::color_ops::Rgba::from_hex(&params.color_b)
            .map_err(|e| live_error("invalid_color", &e, None))?;
        let level = params.level.unwrap_or(0.5);
        if !(0.0..=1.0).contains(&level) {
            return Err(live_error("invalid_level", "level must be in [0, 1]", None));
        }
        let matrix = match &params.matrix {
            Some(s) => crate::dither::Matrix::parse(s).map_err(|e| live_error("invalid_matrix", &e, None))?,
            None => crate::dither::Matrix::Bayer4,
        };
        let pixels: Vec<LivePixel> = crate::dither::dither_region(r.x, r.y, r.width, r.height, a, b, level, matrix)
            .into_iter()
            .map(|(x, y, c)| LivePixel { x, y, color: c.to_hex() })
            .collect();
        let count = pixels.len();
        self.draw_pixels(LiveDrawPixelsParams { pixels, layer: params.layer.clone(), frame: params.frame })
            .await?;
        Ok(json!({
            "pixels": count,
            "color_a": params.color_a,
            "color_b": params.color_b,
            "level": level,
            "matrix": params.matrix.unwrap_or_else(|| "bayer4".into()),
        })
        .to_string())
    }

    /// SPEC-009: artifact-free **RotSprite** rotation — rotate a region of the rendered
    /// sprite by any angle and stamp the clean result onto a target layer. The pure-Rust
    /// core (`crate::rotate`) scales ×8 (Scale2×), rotates nearest-neighbour, then
    /// downscales by per-block mode, so the output introduces **no new colours**
    /// (palette-legal by construction); right angles are exact. Reads the flattened render
    /// via the modal-free `save_preview` and draws via the existing `draw_pixels` path —
    /// no new plugin command. The rotated copy lands on a NEW layer; the source is left as-is.
    pub async fn rotate(&self, params: LiveRotateParams) -> Result<String, String> {
        if !params.angle.is_finite() {
            return Err(live_error("invalid_angle", "angle must be a finite number of degrees", None));
        }

        // Render the active frame to a temp 1x PNG and decode it (same path as extract_style_profile).
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_rotate_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        self.command("save_preview", None, Some(json!({ "filename": temp.to_string_lossy() })))
            .await?;
        let decoded = image::open(&temp)
            .map(|im| im.to_rgba8())
            .map_err(|e| live_error("rotate_decode_failed", &format!("{e}"), None));
        let _ = std::fs::remove_file(&temp);
        let img = decoded?;
        let (iw, ih) = (img.width(), img.height());
        if iw == 0 || ih == 0 {
            return Err(live_error("empty_sprite", "the active sprite has a zero dimension", None));
        }

        // Resolve the source rect: explicit rect, else selection bbox, else the whole canvas.
        let rect = match params.rect {
            Some(r) => r,
            None if params.selection_only.unwrap_or(false) => self.selection_bounds().await?,
            None => LiveRect { x: 0, y: 0, width: iw, height: ih },
        };

        // Clamp to the canvas; reject an empty intersection.
        if rect.x >= iw as i32 || rect.y >= ih as i32 {
            return Err(live_error("rect_off_canvas", "the source rect lies outside the canvas", None));
        }
        let rx = rect.x.max(0) as u32;
        let ry = rect.y.max(0) as u32;
        let rw = rect.width.min(iw - rx);
        let rh = rect.height.min(ih - ry);
        if rw == 0 || rh == 0 {
            return Err(live_error("empty_rect", "the source rect has zero overlap with the canvas", None));
        }
        if (rw as u64) * (rh as u64) > MAX_ROTATE_AREA {
            return Err(live_error(
                "rect_too_large",
                &format!(
                    "{rw}x{rh} exceeds the {MAX_ROTATE_AREA}px rotate cap (the ×8 RotSprite buffer would be huge) — crop or scale down first"
                ),
                None,
            ));
        }

        // Crop -> pure RotSprite core -> rotated raster.
        let src = region_to_raster(&img, rx, ry, rw, rh);
        let rotated = crate::rotate::rotsprite(&src, params.angle);

        // Placement: centre the rotated bbox on the source rect's centre unless overridden.
        let cx = rect.x as f64 + rw as f64 / 2.0;
        let cy = rect.y as f64 + rh as f64 / 2.0;
        let at_x = params.at_x.unwrap_or((cx - rotated.width as f64 / 2.0).round() as i32);
        let at_y = params.at_y.unwrap_or((cy - rotated.height as f64 / 2.0).round() as i32);

        let pixels = raster_to_pixels(&rotated, at_x, at_y);
        if pixels.is_empty() {
            return Err(live_error(
                "empty_result",
                "the rotated region is fully transparent — nothing to draw",
                None,
            ));
        }
        let drawn = pixels.len();
        let distinct = rotated.distinct_colors();
        let (ow, oh) = (rotated.width, rotated.height);

        let layer = params.layer.unwrap_or_else(|| "Rotated".to_string());
        self.draw_pixels(LiveDrawPixelsParams {
            pixels,
            layer: Some(layer.clone()),
            frame: params.frame,
        })
        .await?;

        Ok(json!({
            "changed": true,
            "layer": layer,
            "angle": params.angle,
            "source": { "x": rect.x, "y": rect.y, "width": rw, "height": rh },
            "output": { "width": ow, "height": oh, "at_x": at_x, "at_y": at_y },
            "pixels_drawn": drawn,
            "distinct_colors": distinct,
            "note": "rotated copy drawn on a new layer; the source render is unchanged",
        })
        .to_string())
    }

    /// SPEC-003 Phase 3: compose a full **blob-47** autotile sheet from the four corner quarters
    /// the agent drew (`[fill | outer | edge | inner]`), drawn as a near-square grid on a new layer.
    /// "Draw ~4 quarters → get 47 tiles." Reads the render, composes with the pure `autotile`
    /// compositor (palette-legal by construction — only the source colours), draws via `draw_pixels`.
    pub async fn create_autotile_template(
        &self,
        params: LiveCreateAutotileTemplateParams,
    ) -> Result<String, String> {
        let layout = params.layout.as_deref().unwrap_or("blob47");
        if layout != "blob47" {
            return Err(live_error(
                "unsupported_layout",
                &format!("layout '{layout}' not supported yet (only 'blob47'; wang16 is a follow-up)"),
                None,
            ));
        }
        let ts = params.tile_size;
        if !(4..=MAX_AUTOTILE_TILE).contains(&ts) || ts % 2 != 0 {
            return Err(live_error(
                "invalid_tile_size",
                &format!("tile_size must be an EVEN number in 4..={MAX_AUTOTILE_TILE} (got {ts})"),
                None,
            ));
        }
        let q = ts / 2;

        // Render + decode the active frame (same modal-free path as live_rotate).
        let temp = std::env::temp_dir().join(format!(
            "aseprite_mcp_autotile_{}.png",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        ));
        self.command("save_preview", None, Some(json!({ "filename": temp.to_string_lossy() })))
            .await?;
        let decoded = image::open(&temp)
            .map(|im| im.to_rgba8())
            .map_err(|e| live_error("autotile_decode_failed", &format!("{e}"), None));
        let _ = std::fs::remove_file(&temp);
        let img = decoded?;

        // Slice the four source quarters the agent drew.
        let (sx, sy) = (params.source_x.unwrap_or(0), params.source_y.unwrap_or(0));
        let pieces = crate::autotile::slice_corner_pieces(&img, sx, sy, q).ok_or_else(|| {
            live_error(
                "source_off_canvas",
                &format!(
                    "the 4-quarter source strip ({}x{} at {sx},{sy}) runs off the {}x{} canvas — draw \
                     [fill|outer|edge|inner] quarters of {q}px each there first",
                    4 * q, q, img.width(), img.height()
                ),
                None,
            )
        })?;

        // Compose the 47 tiles and lay them out as a near-square sheet.
        let tiles = crate::autotile::assemble_blob47(&pieces);
        let (cols, rows) = crate::autotile::sheet_dims(tiles.len());
        let at_x = params.at_x.unwrap_or((sx + 4 * q + 2) as i32);
        let at_y = params.at_y.unwrap_or(sy as i32);

        let mut pixels = Vec::new();
        for (i, tile) in tiles.iter().enumerate() {
            let (col, row) = (i as u32 % cols, i as u32 / cols);
            pixels.extend(grid_to_pixels(tile, at_x + (col * ts) as i32, at_y + (row * ts) as i32));
        }
        if pixels.is_empty() {
            return Err(live_error(
                "empty_result",
                "the composed tiles are fully transparent — draw opaque corner quarters first",
                None,
            ));
        }
        let drawn = pixels.len();

        let layer = params.layer.unwrap_or_else(|| "Autotile Template".to_string());
        self.draw_pixels(LiveDrawPixelsParams {
            pixels,
            layer: Some(layer.clone()),
            frame: params.frame,
        })
        .await?;

        Ok(json!({
            "changed": true,
            "layer": layer,
            "layout": "blob47",
            "tile_size": ts,
            "tiles": tiles.len(),
            "sheet": { "cols": cols, "rows": rows, "at_x": at_x, "at_y": at_y, "tile_size": ts },
            "source": { "x": sx, "y": sy, "quarter": q },
            "pixels_drawn": drawn,
            "note": format!(
                "47 blob tiles composed in canonical mask order; run live_pack_similar_tiles \
                 grid_size={ts} over this layer to build the tileset (tile index = blob47 order, \
                 matching autotile::blob47_tile_index)"
            ),
        })
        .to_string())
    }

    /// Resolve the active selection's bounding box as a `LiveRect`, erroring loudly when
    /// there is no usable selection (for `live_rotate`'s `selection_only`).
    async fn selection_bounds(&self) -> Result<LiveRect, String> {
        let raw = self.get_selection().await?;
        let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let sel = v.get("selection");
        let is_empty = sel
            .and_then(|s| s.get("isEmpty"))
            .and_then(|b| b.as_bool())
            .unwrap_or(true);
        let bounds = sel.and_then(|s| s.get("bounds"));
        if let (false, Some(b)) = (is_empty, bounds) {
            let g = |k: &str| b.get(k).and_then(|v| v.as_i64());
            if let (Some(x), Some(y), Some(w), Some(h)) = (g("x"), g("y"), g("width"), g("height")) {
                if w > 0 && h > 0 {
                    return Ok(LiveRect { x: x as i32, y: y as i32, width: w as u32, height: h as u32 });
                }
            }
        }
        Err(live_error(
            "no_selection",
            "selection_only is set but there is no active selection — make a selection or pass `rect`",
            None,
        ))
    }

    /// Diff two animation frames as a TEXT grid (`.`=unchanged, `-`=erased, else the
    /// new colour's glyph). Lets the agent see EXACTLY where two frames differ at the
    /// pixel level — verify an edit, or inspect motion between frames (Path 1 / §A).
    /// Restores the user's active frame.
    pub async fn frame_diff(&self, params: LiveFrameDiffParams) -> Result<String, String> {
        let site: Value = serde_json::from_str(&self.command("get_active_site", None, None).await?)
            .unwrap_or_else(|_| json!({}));
        let frames = site
            .get("sprite")
            .and_then(|s| s.get("frames"))
            .and_then(|f| f.as_u64())
            .unwrap_or(0) as u32;
        if frames == 0 {
            return Err(live_error("no_sprite", "No active sprite to diff", None));
        }
        let orig = site.get("frame").and_then(|f| f.as_u64()).unwrap_or(1) as u32;
        for (label, f) in [("from_frame", params.from_frame), ("to_frame", params.to_frame)] {
            if f < 1 || f > frames {
                return Err(live_error(
                    "invalid_frame",
                    &format!("{label} {f} out of range 1..={frames}"),
                    None,
                ));
            }
        }
        if params.from_frame == params.to_frame {
            return Err(live_error(
                "same_frame",
                "from_frame and to_frame must differ",
                None,
            ));
        }

        let a = self.render_frame(params.from_frame).await;
        let b = match &a {
            Ok(_) => self.render_frame(params.to_frame).await,
            Err(_) => Err(String::new()),
        };
        // Restore the user's active frame regardless of outcome.
        let _ = self
            .command("set_active_frame", None, Some(json!({ "frame": orig })))
            .await;
        let (a, b) = (a?, b?);

        let grid = crate::ascii_view::diff_to_ascii(&a, &b)
            .map_err(|e| live_error("frame_diff_failed", &e, None))?;
        Ok(format!(
            "frame {} → {}\n{}",
            params.from_frame, params.to_frame, grid
        ))
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

    /// Import a reference PNG as palette-locked pixel art on a layer (SPEC-006). Decodes
    /// the reference, content-aware downscales it to the target grid + snaps to a palette
    /// (pure `reference` core), then draws the result onto `layer` via the `draw_pixels`
    /// path — no new plugin command. The live document is the only thing mutated.
    pub async fn import_reference(
        &self,
        params: LiveImportReferenceParams,
    ) -> Result<String, String> {
        validate_non_empty("filename", &params.filename)?;
        let method = crate::reference::Method::parse(params.method.as_deref().unwrap_or("dominant"))
            .map_err(|e| live_error("invalid_method", &e, None))?;

        // Guard the source size BEFORE the full decode (a huge PNG would OOM at decode).
        let decode_err = |e: &dyn std::fmt::Display| {
            live_error(
                "reference_decode_failed",
                &format!("failed to read PNG '{}': {e} (convert other formats to PNG first)", params.filename),
                None,
            )
        };
        let dims = image::ImageReader::open(&params.filename)
            .map_err(|e| decode_err(&e))?
            .into_dimensions()
            .map_err(|e| decode_err(&e))?;
        let cap = crate::reference::MAX_SOURCE_EDGE;
        if dims.0 > cap || dims.1 > cap {
            return Err(live_error(
                "reference_too_large",
                &format!("reference {}x{} exceeds the {cap}px decode cap — downscale it first", dims.0, dims.1),
                None,
            ));
        }

        // Decode the reference (PNG only — `image` is built png-only).
        let img = image::open(&params.filename)
            .map_err(|e| {
                live_error(
                    "reference_decode_failed",
                    &format!("failed to read PNG '{}': {e} (convert other formats to PNG first)", params.filename),
                    None,
                )
            })?
            .to_rgba8();
        let (sw, sh) = (img.width(), img.height());
        if sw == 0 || sh == 0 {
            return Err(live_error(
                "invalid_reference",
                &format!("reference has a zero dimension ({sw}x{sh})"),
                None,
            ));
        }

        // Optional de-fake: detect the native pixel grid of a *scaled* reference so we can
        // recover it to 1× (block-uniformity / GCD — `style_profile::detect_grid`). Only a
        // *real* upscale (scale > 1 and a plausible, non-degenerate native — `is_real_upscale`)
        // steers the target; native art / photos / flat swatches fall through unchanged.
        let regrid = params.regrid.unwrap_or(false);
        let detected = if regrid { Some(crate::style_profile::detect_grid(&img)) } else { None };
        let honored = detected
            .as_ref()
            .is_some_and(|g| crate::reference::is_real_upscale(g.scale, g.native[0], g.native[1]));
        // The native grid we'll actually use to steer the import (None unless honoured).
        let native: Option<(u32, u32)> =
            if honored { detected.as_ref().map(|g| (g.native[0], g.native[1])) } else { None };

        // Resolve the target grid: explicit dims win; else the honoured native resolution;
        // else the active sprite's size. Only fetch sprite info when a dim is still missing
        // AND the native grid doesn't already cover it — so a scaled-with-no-dims import (the
        // headline case) doesn't make a failure-prone bridge round-trip.
        let need_sprite = (params.width.is_none() || params.height.is_none()) && native.is_none();
        let sprite_dims = if need_sprite {
            let info: Value =
                serde_json::from_str(&self.get_sprite_info().await?).unwrap_or(Value::Null);
            let sprite_w = info.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let sprite_h = info.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            (sprite_w, sprite_h)
        } else {
            (0, 0) // unused — explicit dims or a detected native grid cover the target
        };
        // A detected native larger than the import cap, with no explicit dims to fit it,
        // is a dead end under the generic cap error — say so specifically and actionably.
        if let Some((nw, nh)) = native {
            let cap = crate::reference::MAX_TARGET_EDGE;
            if (params.width.is_none() || params.height.is_none()) && (nw > cap || nh > cap) {
                return Err(live_error(
                    "native_exceeds_cap",
                    &format!(
                        "regrid detected a native resolution of {nw}x{nh}, larger than the {cap}px \
                         import cap — pass an explicit width/height ≤{cap} to fit (regrid still \
                         recovers the clean 1× pixels first)"
                    ),
                    None,
                ));
            }
        }
        let (tw, th) = resolve_import_target((params.width, params.height), native, sprite_dims);
        validate_target_dims(tw, th)?;

        // Resolve the palette: auto-extracted from the source, an explicit list, the active
        // palette, or none (snap off). `auto_colors` and `palette` are mutually exclusive.
        let snap = params.snap.unwrap_or(true);
        if !snap && params.auto_colors.is_some() {
            return Err(live_error(
                "snap_conflict",
                "auto_colors extracts a palette to snap to, but snap is false — drop one",
                None,
            ));
        }
        let mut auto_palette: Option<Vec<Rgba>> = None;
        let palette: Option<Vec<Rgba>> = if !snap {
            None
        } else if let Some(n) = params.auto_colors {
            if params.palette.is_some() {
                return Err(live_error(
                    "palette_conflict",
                    "pass either `palette` or `auto_colors`, not both",
                    None,
                ));
            }
            let n = validate_auto_colors(n)?;
            let pm = crate::palette_extract::Method::parse(
                params.palette_method.as_deref().unwrap_or("median_cut"),
            )
            .map_err(|e| live_error("invalid_palette_method", &e, None))?;
            let pal = crate::palette_extract::extract_from_image(&img, pm, n);
            if pal.is_empty() {
                return Err(live_error(
                    "empty_auto_palette",
                    "could not extract a palette — the reference is fully transparent",
                    None,
                ));
            }
            auto_palette = Some(pal.clone());
            Some(pal)
        } else if let Some(hexes) = &params.palette {
            Some(parse_hex_palette(hexes)?)
        } else {
            Some(self.fetch_active_palette().await?)
        };

        // Pure core: downscale + snap. A honoured upscale routes through the regrid two-pass
        // (recover the exact native, then fit) so the final downscale starts from clean 1×
        // pixels rather than the scaled blur; otherwise a plain content-aware shrink.
        let grid = match native {
            Some(nat) => crate::reference::regrid_then_fit(&img, nat, (tw, th), palette.as_deref(), method),
            None => crate::reference::downscale_to_grid(&img, tw, th, palette.as_deref(), method),
        };
        let (ax, ay) = (params.at_x.unwrap_or(0), params.at_y.unwrap_or(0));
        let pixels = grid_to_pixels(&grid, ax, ay);
        if pixels.is_empty() {
            return Err(live_error(
                "empty_result",
                "the downscaled reference is fully transparent — nothing to draw",
                None,
            ));
        }
        let drawn = pixels.len();
        let distinct = crate::reference::distinct_colors(&grid);

        // Draw onto the target layer via the existing path.
        let layer = params.layer.unwrap_or_else(|| "Reference".to_string());
        self.draw_pixels(LiveDrawPixelsParams {
            pixels,
            layer: Some(layer.clone()),
            frame: params.frame,
        })
        .await?;

        Ok(json!({
            "changed": true,
            "layer": layer,
            "source": { "width": sw, "height": sh },
            "target": { "width": tw, "height": th },
            "factor": (sw.max(sh) as f64 / tw.max(th).max(1) as f64),
            "method": if method == crate::reference::Method::Average { "average" } else { "dominant" },
            "palette_size": palette.as_ref().map(|p| p.len()),
            "pixels_drawn": drawn,
            "distinct_colors": distinct,
            // What the de-fake pass found/did, so the agent can see whether a scaled
            // reference was recovered (loud) — null when regrid wasn't requested. `applied`
            // is true only for a real upscale that was honoured; a degenerate/flat detection
            // (e.g. native 1×1) reports its raw scale but applied:false (it was a no-op).
            "regrid": detected.as_ref().map(|g| json!({
                "detected_scale": g.scale,
                "native": { "width": g.native[0], "height": g.native[1] },
                "applied": honored,
            })),
            // The auto-extracted palette (hex, luma-sorted) when `auto_colors` was used — so the
            // agent can lock it on the sprite; null otherwise. `count` may be < `requested` after
            // dedup (e.g. a source with fewer distinct colours than asked).
            "auto_palette": auto_palette.as_ref().map(|p| json!({
                "method": params.palette_method.as_deref().unwrap_or("median_cut"),
                "requested": params.auto_colors,
                "count": p.len(),
                "colors": p.iter().map(|c| c.to_hex()).collect::<Vec<_>>(),
            })),
        })
        .to_string())
    }

    /// Fetch the active sprite's full palette as `color_ops::Rgba` (for import snapping).
    async fn fetch_active_palette(&self) -> Result<Vec<Rgba>, String> {
        let resp = self
            .list_palette(LiveListPaletteParams { palette: None, from: Some(0), limit: Some(256) })
            .await?;
        let palette = parse_palette_colors(&resp);
        if palette.is_empty() {
            return Err(live_error(
                "no_palette",
                "the active sprite has no palette to snap to — pass `palette` or set snap:false",
                None,
            ));
        }
        Ok(palette)
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

    /// SPEC-009: re-shade a region onto a target ramp — each unique colour maps to the ramp
    /// step matching its luma (dark→light). Palette-legal by construction (only ramp colours
    /// are emitted); reuses the SPEC-004 `get_region_colors` → `apply_color_map` path.
    pub async fn gradient_map(&self, params: LiveGradientMapParams) -> Result<String, String> {
        let ramp: Vec<Rgba> = params
            .ramp
            .iter()
            .map(|h| Rgba::from_hex(h))
            .collect::<Result<_, _>>()
            .map_err(|e| live_error("invalid_color", &e, None))?;
        if ramp.len() < 2 {
            return Err(live_error("invalid_ramp", "ramp needs >= 2 colours (dark->light)", None));
        }
        let (colors, _palette, image_layer) = self
            .fetch_region_colors(params.layer.clone(), params.frame, params.selection_only)
            .await?;
        if !image_layer {
            return Err(live_error(
                "not_an_image_layer",
                "colour ops need a regular image layer (not a group or tilemap)",
                None,
            ));
        }
        let map: Vec<(Rgba, Rgba)> = colors
            .iter()
            .map(|&c| (c, color_ops::gradient_map(c, &ramp)))
            .filter(|(f, t)| f != t)
            .collect();
        if map.is_empty() {
            return Ok(json!({
                "changed": false,
                "mappedColors": 0,
                "uniqueColors": colors.len(),
            })
            .to_string());
        }
        let res = self
            .apply_color_map(params.layer, params.frame, params.selection_only, &map)
            .await?;
        Ok(json!({
            "changed": true,
            "mappedColors": map.len(),
            "uniqueColors": colors.len(),
            "rampSteps": ramp.len(),
            "pixels": res.get("pixels").cloned().unwrap_or(Value::Null),
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

/// Choose the `import_reference` target grid. Explicit `width`/`height` (both set) win.
/// Otherwise, when regrid honoured a real upscale, the import defaults to the recovered
/// **native** resolution; if there is no honoured native (native art / a photo / regrid off)
/// it falls back to the active sprite's size. A single missing dim is filled from the native
/// grid (if any) or the sprite. Pure → unit-testable.
fn resolve_import_target(
    explicit: (Option<u32>, Option<u32>),
    native: Option<(u32, u32)>, // honoured native grid, else None
    sprite: (u32, u32),
) -> (u32, u32) {
    if let (Some(w), Some(h)) = explicit {
        return (w, h);
    }
    if let Some((nw, nh)) = native {
        // A real upscale: land on the native grid, but still honour a single explicit dim.
        return (explicit.0.unwrap_or(nw), explicit.1.unwrap_or(nh));
    }
    (explicit.0.unwrap_or(sprite.0), explicit.1.unwrap_or(sprite.1))
}

/// Validate `auto_colors` (1..=256) for the import auto-palette. Pure → unit-testable.
fn validate_auto_colors(n: u32) -> Result<usize, String> {
    if (1..=256).contains(&n) {
        Ok(n as usize)
    } else {
        Err(live_error(
            "invalid_auto_colors",
            &format!("auto_colors must be in 1..=256 (got {n})"),
            None,
        ))
    }
}

/// Reject a non-positive or oversized `import_reference` target — bounds the one
/// `draw_pixels` batch the import produces. Pure → unit-testable.
fn validate_target_dims(tw: u32, th: u32) -> Result<(), String> {
    if tw == 0 || th == 0 {
        return Err(live_error(
            "invalid_target",
            &format!("target size must be positive (got {tw}x{th}); no active sprite to size from?"),
            None,
        ));
    }
    let cap = crate::reference::MAX_TARGET_EDGE;
    if tw > cap || th > cap {
        return Err(live_error(
            "target_too_large",
            &format!("target {tw}x{th} exceeds the {cap}px import cap — choose a smaller width/height"),
            None,
        ));
    }
    Ok(())
}

/// Parse an explicit `#rrggbb`(`aa`) palette list into `color_ops::Rgba`. Pure.
fn parse_hex_palette(hexes: &[String]) -> Result<Vec<Rgba>, String> {
    if hexes.is_empty() {
        return Err(live_error("invalid_palette", "palette is empty", None));
    }
    let mut out = Vec::with_capacity(hexes.len());
    for h in hexes {
        out.push(Rgba::from_hex(h).map_err(|e| {
            live_error("invalid_color", &format!("invalid palette colour '{h}': {e}"), None)
        })?);
    }
    Ok(out)
}

/// Parse a `list_palette` response (`{colors:[{color:{red,green,blue,alpha}}]}`) into
/// opaque `color_ops::Rgba`. Pure.
fn parse_palette_colors(resp: &str) -> Vec<Rgba> {
    let v: Value = serde_json::from_str(resp).unwrap_or(Value::Null);
    let mut out = Vec::new();
    if let Some(colors) = v.get("colors").and_then(|c| c.as_array()) {
        for c in colors {
            let col = c.get("color");
            let f = |k: &str| col.and_then(|x| x.get(k)).and_then(|n| n.as_u64()).unwrap_or(0) as u8;
            out.push(Rgba::rgb(f("red"), f("green"), f("blue")));
        }
    }
    out
}

/// Convert the downscaled grid to a `draw_pixels` batch (skips fully-transparent cells),
/// offset by `(ax, ay)`. Pure → unit-tested.
fn grid_to_pixels(grid: &image::RgbaImage, ax: i32, ay: i32) -> Vec<LivePixel> {
    let mut pixels = Vec::new();
    for oy in 0..grid.height() {
        for ox in 0..grid.width() {
            let p = grid.get_pixel(ox, oy).0;
            if p[3] != 0 {
                pixels.push(LivePixel {
                    x: ax + ox as i32,
                    y: ay + oy as i32,
                    color: format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2]),
                });
            }
        }
    }
    pixels
}

/// Crop a `rw × rh` region at `(rx, ry)` from a decoded render into a pure-Rust
/// `rotate::Raster` (SPEC-009 rotation). Caller guarantees the rect is in-bounds.
fn region_to_raster(img: &image::RgbaImage, rx: u32, ry: u32, rw: u32, rh: u32) -> crate::rotate::Raster {
    let mut r = crate::rotate::Raster::new(rw, rh);
    for y in 0..rh {
        for x in 0..rw {
            let p = img.get_pixel(rx + x, ry + y).0;
            r.set(x, y, Rgba::rgba(p[0], p[1], p[2], p[3]));
        }
    }
    r
}

/// Convert a rotated `rotate::Raster` to a `draw_pixels` batch (skips transparent cells),
/// offset by `(ax, ay)`. Emits `#rrggbbaa` when needed so alpha is preserved exactly.
fn raster_to_pixels(r: &crate::rotate::Raster, ax: i32, ay: i32) -> Vec<LivePixel> {
    let mut pixels = Vec::new();
    for y in 0..r.height {
        for x in 0..r.width {
            let c = r.get(x, y);
            if !c.is_transparent() {
                pixels.push(LivePixel { x: ax + x as i32, y: ay + y as i32, color: c.to_hex() });
            }
        }
    }
    pixels
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

/// What region a preview should cover, after validating the `crop` selector but before
/// the bridge round-trip. `Cel` is resolved later from the plugin's reported bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropPlan {
    Whole,
    Cel,
    Explicit(crate::preview::Crop),
}

/// Validate the `crop` selector and turn it into a [`CropPlan`] (SPEC-005 Phase 2).
/// Pure — unit-tested without the bridge. `"sprite"`/absent = whole canvas; `"cel"` =
/// defer to the active cel's bounds; an explicit rect is validated here.
fn resolve_crop_plan(crop: &Option<LiveCrop>) -> Result<CropPlan, String> {
    match crop {
        None => Ok(CropPlan::Whole),
        Some(LiveCrop::Mode(m)) => match m.as_str() {
            "sprite" => Ok(CropPlan::Whole),
            "cel" => Ok(CropPlan::Cel),
            other => Err(live_error(
                "invalid_crop",
                &format!(
                    "crop must be \"cel\", \"sprite\", or {{x,y,width,height}} (got \"{other}\")"
                ),
                None,
            )),
        },
        Some(LiveCrop::Rect(r)) => Ok(CropPlan::Explicit(rect_to_crop(r)?)),
    }
}

/// Validate an explicit crop rectangle and convert it to a [`crate::preview::Crop`]
/// (non-negative origin, positive size). Far-edge clamping happens in `preview`.
fn rect_to_crop(r: &LiveRect) -> Result<crate::preview::Crop, String> {
    if r.x < 0 || r.y < 0 {
        return Err(live_error(
            "invalid_crop",
            &format!("crop origin ({},{}) must be non-negative", r.x, r.y),
            None,
        ));
    }
    if r.width == 0 || r.height == 0 {
        return Err(live_error(
            "invalid_crop",
            "crop width and height must be positive",
            None,
        ));
    }
    Ok(crate::preview::Crop {
        x: r.x as u32,
        y: r.y as u32,
        width: r.width,
        height: r.height,
    })
}

/// Build a crop from the active cel's bounds in a `save_preview` response (SPEC-005
/// Phase 2). A negative cel origin is clamped to the canvas (reducing the size). Absent
/// bounds — an empty active layer/frame, or a plugin that predates the report — is a
/// loud degrade (ADR-0005), never a silent guess. Pure → unit-tested.
fn cel_crop_from_response(resp: &str) -> Result<crate::preview::Crop, String> {
    let v: Value = serde_json::from_str(resp).unwrap_or(Value::Null);
    let cel = match v.get("cel") {
        Some(c) if c.is_object() => c,
        _ => {
            return Err(live_error(
                "cel_bounds_unavailable",
                "crop=\"cel\" needs the active cel's bounds, but none were reported — the \
                 active layer/frame may be empty, or the connected Aseprite plugin predates \
                 cel-bounds reporting. Use crop:\"sprite\", an explicit {x,y,width,height}, or \
                 select a non-empty cel.",
                None,
            ));
        }
    };
    let field = |k: &str| cel.get(k).and_then(|n| n.as_i64()).unwrap_or(0);
    let (gx, gy, cw, ch) = (field("x"), field("y"), field("width"), field("height"));
    // Clamp a negative origin to the canvas, shrinking the size to the visible part.
    let x0 = gx.max(0);
    let y0 = gy.max(0);
    let w = gx + cw - x0;
    let h = gy + ch - y0;
    if w <= 0 || h <= 0 {
        return Err(live_error(
            "cel_bounds_unavailable",
            "the active cel has no visible area on the canvas to crop to",
            None,
        ));
    }
    Ok(crate::preview::Crop {
        x: x0 as u32,
        y: y0 as u32,
        width: w as u32,
        height: h as u32,
    })
}

/// Upper bound on rendered Set-of-Mark badges, so a noisy sprite (hundreds of tiny
/// connected components) can't bury the art in overlapping badge boxes. Past this the
/// largest regions are kept and `marks_truncated` reports the true total.
const MAX_MARKS: usize = 64;

/// Upper bound on a single `dither_fill` region (px), so one call can't explode the
/// `draw_pixels` batch; split a larger fill.
const MAX_DITHER_AREA: u64 = 256 * 256;

/// Upper bound on a single `rotate` source region (px). RotSprite scales the region ×8
/// (×64 area) before rotating, so the cap is tighter than the dither one to bound that
/// intermediate buffer; crop or scale down a larger subject first.
const MAX_ROTATE_AREA: u64 = 200 * 200;
/// Largest tile edge `live_create_autotile_template` allows (47 tiles × this² bounds the one
/// `draw_pixels` batch the sheet produces).
const MAX_AUTOTILE_TILE: u32 = 64;

/// Set-of-Mark region source, resolved by `save_preview` (SPEC-005 Phase 4): explicit
/// source-space `Regions` (slices / layer cels read from the bridge) or `Components`
/// (computed from the rendered buffer here). `None` = no marks.
enum MarksInput {
    None,
    Regions(Vec<crate::marks::Region>),
    Components,
}

/// Composite the optional coordinate gutter (SPEC-005 Phase 1) and optional Set-of-Mark
/// numbered badges (Phase 4) onto the upscaled preview `buffer`, write the PNG to `dst`,
/// and build the result JSON. Pure (buffer-in / path-out, no bridge) so the gutter policy
/// (default-on-when-legible, loud `gutter:true` refusal, graceful `gutter_skipped`
/// degrade) and the mark placement are unit-tested without Aseprite.
///
/// The sidecar reports `gutter.left_w`/`top_h` + `scale` so any (x,y) the agent reads off
/// the labelled preview inverts exactly, plus `marks: [{n, region, bbox}]` mapping each
/// badge to its source-space region.
fn finish_preview(
    buffer: image::RgbaImage,
    info: crate::preview::PreviewInfo,
    dst: &str,
    gutter: Option<bool>,
    gutter_step: Option<u32>,
    marks: MarksInput,
) -> Result<String, String> {
    let step = gutter_step.unwrap_or(crate::gutter::DEFAULT_GUTTER_STEP);
    let explicit = gutter == Some(true);
    let mut gutter_json = Value::Null;
    let mut gutter_skipped: Option<String> = None;

    // Palette steers both the gutter labels and the badge text off the sprite's colours.
    let palette = crate::gutter::sprite_palette(&buffer, info.scale);

    // Resolve mark regions to source space BEFORE the gutter consumes `buffer`. Components
    // run the connected-component pass at SOURCE resolution (reconstructed from the bare
    // upscaled buffer by sampling one px per block — exact, and avoids a CC over the up-to-
    // 67M-px upscaled buffer), then offset by the crop origin so bboxes are full-sprite.
    let regions: Vec<crate::marks::Region> = match &marks {
        MarksInput::None => Vec::new(),
        MarksInput::Regions(r) => r.clone(),
        MarksInput::Components => {
            let source = downsample_by_scale(&buffer, info.scale);
            crate::marks::connected_components(&source)
                .into_iter()
                .enumerate()
                .map(|(i, r)| crate::marks::Region {
                    name: format!("component-{}", i + 1),
                    bbox: crate::marks::MarkRect {
                        x: info.crop_x + r.x,
                        y: info.crop_y + r.y,
                        width: r.width,
                        height: r.height,
                    },
                })
                .collect()
        }
    };

    let mut final_buffer = if gutter == Some(false) {
        buffer
    } else {
        // Labels read ABSOLUTE sprite coords (offset by the crop origin), so an agent
        // reads the real (x,y) off the gutter even on a cropped preview.
        match crate::gutter::render_with_gutter_at(
            &buffer,
            info.scale,
            step,
            info.crop_x,
            info.crop_y,
            &palette,
        ) {
            Ok((composited, gi)) => {
                gutter_json = json!({
                    "left_w": gi.left_w,
                    "top_h": gi.top_h,
                    "step": gi.step,
                    "image": { "width": gi.out_width, "height": gi.out_height },
                });
                composited
            }
            // An explicit request that can't be drawn legibly is a loud refusal with
            // guidance (raise scale / crop); a default one degrades to a plain preview.
            Err(e) if explicit => {
                return Err(live_error("gutter_unreadable", &e, None));
            }
            Err(e) => {
                gutter_skipped = Some(e);
                buffer
            }
        }
    };

    // Keep only regions whose centre falls inside the previewed crop window, THEN number
    // them — so every emitted mark has a visible badge and numbering is contiguous (a
    // slice outside a cel crop gets neither a badge nor an orphan number). Components are
    // already in-window by construction.
    let mut in_window: Vec<crate::marks::Region> = regions
        .into_iter()
        .filter(|r| {
            let (scx, scy) = r.bbox.center();
            scx >= info.crop_x
                && scy >= info.crop_y
                && scx < info.crop_x + info.source_width
                && scy < info.crop_y + info.source_height
        })
        .collect();
    // Cap the number of badges so a noisy sprite (hundreds of tiny components) can't bury
    // the art in overlapping boxes; keep the largest regions and report how many were cut.
    let total_regions = in_window.len();
    let marks_truncated = total_regions > MAX_MARKS;
    if marks_truncated {
        in_window.sort_by_key(|r| std::cmp::Reverse((r.bbox.width as u64) * (r.bbox.height as u64)));
        in_window.truncate(MAX_MARKS);
    }
    let placed = crate::marks::assign_marks(&in_window);

    // Draw a badge at each region centroid (in final-image space, offset by the gutter
    // band when present). The returned `[{n, region, bbox}]` lets the orchestrator map any
    // mark the critic names back to its source region.
    let band = gutter_json
        .get("left_w")
        .and_then(|v| v.as_u64())
        .map(|lw| (lw as u32, gutter_json["top_h"].as_u64().unwrap_or(0) as u32))
        .unwrap_or((0, 0));
    if !placed.is_empty() {
        let fg = crate::gutter::pick_label_color(&palette);
        let fg_px = image::Rgba([fg.r, fg.g, fg.b, 255]);
        let fs = crate::marks::badge_font_scale(info.scale);
        for m in &placed {
            let (scx, scy) = m.bbox.center();
            let px = band.0 + (scx - info.crop_x) * info.scale + info.scale / 2;
            let py = band.1 + (scy - info.crop_y) * info.scale + info.scale / 2;
            crate::marks::draw_badge(&mut final_buffer, px, py, m.n, fs, fg_px);
        }
    }

    final_buffer
        .save_with_format(std::path::Path::new(dst), image::ImageFormat::Png)
        .map_err(|e| {
            live_error(
                "preview_render_failed",
                &format!("failed to write preview {dst}: {e}"),
                None,
            )
        })?;

    // `gutter_applied` is the machine-readable signal an orchestrator checks before
    // inverting a coordinate: true -> source = crop.{x,y} + (preview - {left_w,top_h}) /
    // scale; false (suppressed/degraded) -> the bare art, source = crop.{x,y} + preview /
    // scale. `crop` is the previewed region's origin (0,0 uncropped); `source` is its size.
    let mut out = json!({
        "changed": true,
        "filename": dst,
        "scale": info.scale,
        "source": { "width": info.source_width, "height": info.source_height },
        "crop": { "x": info.crop_x, "y": info.crop_y },
        "preview": { "width": info.preview_width, "height": info.preview_height },
        "gutter_applied": !gutter_json.is_null(),
    });
    if !gutter_json.is_null() {
        out["gutter"] = gutter_json;
    }
    if let Some(reason) = gutter_skipped {
        out["gutter_skipped"] = json!(reason);
    }
    // `marks` is present (possibly `[]`) whenever marks were requested, so an orchestrator
    // can tell "requested, none found" from "not requested". When more regions than
    // `MAX_MARKS` were found, only the largest are badged and `marks_truncated` reports the
    // true total.
    if !matches!(marks, MarksInput::None) {
        out["marks"] = Value::Array(
            placed
                .iter()
                .map(|m| {
                    json!({
                        "n": m.n,
                        "region": m.region,
                        "bbox": { "x": m.bbox.x, "y": m.bbox.y, "width": m.bbox.width, "height": m.bbox.height },
                    })
                })
                .collect(),
        );
        if marks_truncated {
            out["marks_truncated"] = json!(total_regions);
        }
    }
    Ok(out.to_string())
}

/// Reconstruct the source-resolution image from a nearest-neighbor `scale`× upscaled
/// `buf` by sampling the top-left pixel of each block (exact: `buf[x,y] = src[x/scale,
/// y/scale]`). Lets the connected-component pass run cheaply at source resolution instead
/// of over the upscaled buffer, and yields source-space bboxes directly.
fn downsample_by_scale(buf: &image::RgbaImage, scale: u32) -> image::RgbaImage {
    let scale = scale.max(1);
    let (sw, sh) = (buf.width() / scale, buf.height() / scale);
    let mut out = image::RgbaImage::new(sw.max(1), sh.max(1));
    for sy in 0..sh {
        for sx in 0..sw {
            out.put_pixel(sx, sy, *buf.get_pixel(sx * scale, sy * scale));
        }
    }
    out
}

/// Active frame number from a `save_preview` response (the plugin reports `frame`);
/// defaults to 1 if absent. Used to pick the layer cels for `marks_from="layers"`.
fn active_frame_from_response(resp: &str) -> u32 {
    serde_json::from_str::<Value>(resp)
        .ok()
        .and_then(|v| v.get("frame").and_then(|f| f.as_u64()))
        .unwrap_or(1) as u32
}

/// Parse `list_slices` output into source-space mark regions (SPEC-005 Phase 4). Each
/// named slice with a bounds rect becomes one region; slices without bounds are skipped.
/// Pure → unit-tested.
fn parse_slice_regions(resp: &str) -> Vec<crate::marks::Region> {
    let v: Value = serde_json::from_str(resp).unwrap_or(Value::Null);
    let mut out = Vec::new();
    if let Some(slices) = v.get("slices").and_then(|s| s.as_array()) {
        for s in slices {
            if let Some(bbox) = rect_from_json(s.get("bounds")) {
                let name = s
                    .get("name")
                    .and_then(|n| n.as_str())
                    .filter(|n| !n.is_empty())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("slice-{}", out.len() + 1));
                out.push(crate::marks::Region { name, bbox });
            }
        }
    }
    out
}

/// Parse `list_cels` (active frame) + `list_layers` (visibility) into one region per
/// VISIBLE layer's cel bbox (SPEC-005 Phase 4). A cel's bbox is its position + image size;
/// cels on hidden layers (or with no image) are skipped. Pure → unit-tested.
fn parse_layer_regions(cels_resp: &str, layers_resp: &str) -> Vec<crate::marks::Region> {
    // `None` = visibility couldn't be determined (don't filter); `Some(set)` = the
    // effectively-visible layer names (an empty set genuinely means "nothing visible").
    let visible = visible_layer_names(layers_resp);
    let v: Value = serde_json::from_str(cels_resp).unwrap_or(Value::Null);
    let mut out = Vec::new();
    // Disambiguate duplicate layer names so the mark→name map stays human-usable
    // (Aseprite allows two layers to share a name).
    let mut name_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    if let Some(cels) = v.get("cels").and_then(|c| c.as_array()) {
        for c in cels {
            let layer = c.get("layer").and_then(|n| n.as_str()).unwrap_or("");
            if let Some(set) = &visible {
                if !set.contains(layer) {
                    continue;
                }
            }
            let (x, y) = (
                c.get("x").and_then(|n| n.as_i64()).unwrap_or(0),
                c.get("y").and_then(|n| n.as_i64()).unwrap_or(0),
            );
            let img = c.get("image");
            let (w, h) = (
                img.and_then(|i| i.get("width")).and_then(|n| n.as_i64()).unwrap_or(0),
                img.and_then(|i| i.get("height")).and_then(|n| n.as_i64()).unwrap_or(0),
            );
            if w <= 0 || h <= 0 {
                continue;
            }
            let x0 = x.max(0);
            let y0 = y.max(0);
            let w = x + w - x0;
            let h = y + h - y0;
            if w <= 0 || h <= 0 {
                continue;
            }
            let base = if layer.is_empty() {
                format!("layer-{}", out.len() + 1)
            } else {
                layer.to_string()
            };
            let count = name_counts.entry(base.clone()).or_insert(0);
            *count += 1;
            let name = if *count > 1 { format!("{base} #{count}") } else { base };
            out.push(crate::marks::Region {
                name,
                bbox: crate::marks::MarkRect {
                    x: x0 as u32,
                    y: y0 as u32,
                    width: w as u32,
                    height: h as u32,
                },
            });
        }
    }
    out
}

/// Effectively-visible layer names from `list_layers` output. Returns `None` when the
/// `layers` key is absent or unparseable (caller then declines to filter), else `Some`
/// of the names whose own `isVisible` AND every ancestor group's are true — matching what
/// the preview composite (`img:drawSprite`) actually renders (a layer inside a hidden
/// group is not drawn, even if its own flag is visible).
fn visible_layer_names(resp: &str) -> Option<std::collections::HashSet<String>> {
    fn walk(node: &Value, ancestors_visible: bool, out: &mut std::collections::HashSet<String>) {
        if let Some(arr) = node.as_array() {
            for l in arr {
                walk(l, ancestors_visible, out);
            }
            return;
        }
        // A layer/group is effectively visible only if it AND all ancestors are visible
        // (a missing flag defaults to visible — the plugin always reports it).
        let self_visible = ancestors_visible
            && node.get("isVisible").and_then(|v| v.as_bool()).unwrap_or(true);
        if self_visible {
            if let Some(name) = node.get("name").and_then(|n| n.as_str()) {
                out.insert(name.to_string());
            }
        }
        // Descend into a group only when the group is itself effectively visible.
        if let Some(children) = node.get("layers") {
            walk(children, self_visible, out);
        }
    }
    let v: Value = serde_json::from_str(resp).ok()?;
    let layers = v.get("layers")?;
    let mut out = std::collections::HashSet::new();
    walk(layers, true, &mut out);
    Some(out)
}

/// Parse a `{x,y,width,height}` rect (the plugin's `rectangle_info` shape) into a
/// source-space [`crate::marks::MarkRect`]; `None` if missing or degenerate.
fn rect_from_json(v: Option<&Value>) -> Option<crate::marks::MarkRect> {
    let r = v?;
    let f = |k: &str| r.get(k).and_then(|n| n.as_i64());
    let (x, y, w, h) = (f("x")?, f("y")?, f("width")?, f("height")?);
    if w <= 0 || h <= 0 {
        return None;
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let w = x + w - x0;
    let h = y + h - y0;
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(crate::marks::MarkRect {
        x: x0 as u32,
        y: y0 as u32,
        width: w as u32,
        height: h as u32,
    })
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

    // ---- import_reference helpers (SPEC-006) ------------------------------------

    #[test]
    fn validate_target_dims_bounds_the_import() {
        assert!(validate_target_dims(64, 48).is_ok());
        assert!(validate_target_dims(0, 32).unwrap_err().contains("invalid_target"));
        assert!(validate_target_dims(32, 0).unwrap_err().contains("invalid_target"));
        let cap = crate::reference::MAX_TARGET_EDGE;
        assert!(validate_target_dims(cap, cap).is_ok());
        assert!(validate_target_dims(cap + 1, 32).unwrap_err().contains("target_too_large"));
    }

    #[test]
    fn validate_auto_colors_bounds() {
        assert_eq!(validate_auto_colors(16).unwrap(), 16);
        assert_eq!(validate_auto_colors(1).unwrap(), 1);
        assert_eq!(validate_auto_colors(256).unwrap(), 256);
        assert!(validate_auto_colors(0).unwrap_err().contains("invalid_auto_colors"));
        assert!(validate_auto_colors(257).unwrap_err().contains("invalid_auto_colors"));
    }

    #[test]
    fn resolve_import_target_precedence() {
        // Explicit dims always win, even over an honoured native grid.
        assert_eq!(
            resolve_import_target((Some(48), Some(32)), Some((64, 64)), (16, 16)),
            (48, 32)
        );
        // An honoured native + no dims -> land on the native grid, NOT the active sprite size.
        assert_eq!(resolve_import_target((None, None), Some((64, 64)), (16, 16)), (64, 64));
        // No honoured native (native art / a photo / degenerate / regrid off) -> sprite size.
        assert_eq!(resolve_import_target((None, None), None, (16, 16)), (16, 16));
        assert_eq!(resolve_import_target((None, None), None, (24, 24)), (24, 24));
        // A single explicit dim is honoured; the other comes from the native grid (if any)
        // or the sprite (otherwise).
        assert_eq!(
            resolve_import_target((Some(100), None), Some((64, 64)), (16, 16)),
            (100, 64)
        );
        assert_eq!(resolve_import_target((None, Some(40)), None, (16, 24)), (16, 40));
    }

    #[test]
    fn parse_hex_palette_parses_and_rejects() {
        let pal = parse_hex_palette(&["#ff0000".into(), "#00ff00".into()]).unwrap();
        assert_eq!(pal, vec![Rgba::rgb(255, 0, 0), Rgba::rgb(0, 255, 0)]);
        assert!(parse_hex_palette(&[]).unwrap_err().contains("invalid_palette"));
        assert!(parse_hex_palette(&["nothex".into()]).unwrap_err().contains("invalid_color"));
    }

    #[test]
    fn parse_palette_colors_reads_list_palette_response() {
        let resp = json!({
            "colors": [
                { "index": 0, "color": { "red": 255, "green": 0, "blue": 0, "alpha": 255 } },
                { "index": 1, "color": { "red": 16, "green": 32, "blue": 48, "alpha": 255 } },
            ]
        }).to_string();
        assert_eq!(parse_palette_colors(&resp), vec![Rgba::rgb(255, 0, 0), Rgba::rgb(16, 32, 48)]);
        // No colors array -> empty (caller turns that into a loud no_palette error).
        assert!(parse_palette_colors("{}").is_empty());
    }

    #[test]
    fn grid_to_pixels_skips_transparent_and_offsets() {
        let mut grid = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 0]));
        grid.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        grid.put_pixel(1, 1, image::Rgba([0, 128, 255, 255]));
        let pixels = grid_to_pixels(&grid, 10, 20);
        assert_eq!(pixels.len(), 2, "transparent cells skipped");
        assert_eq!((pixels[0].x, pixels[0].y), (10, 20));
        assert_eq!(pixels[0].color, "#ff0000");
        assert_eq!((pixels[1].x, pixels[1].y), (11, 21));
        assert_eq!(pixels[1].color, "#0080ff");
        // A fully-transparent grid yields nothing (caller errors empty_result).
        let blank = image::RgbaImage::from_pixel(3, 3, image::Rgba([0, 0, 0, 0]));
        assert!(grid_to_pixels(&blank, 0, 0).is_empty());
    }

    // ---- finish_preview (SPEC-005 Phase 1 gutter wiring) -------------------------

    fn preview_buffer(src_w: u32, src_h: u32, scale: u32) -> (image::RgbaImage, crate::preview::PreviewInfo) {
        let (pw, ph) = (src_w * scale, src_h * scale);
        // A non-trivial colour so sprite_palette has something to steer off.
        let buf = image::RgbaImage::from_pixel(pw, ph, image::Rgba([40, 90, 160, 255]));
        let info = crate::preview::PreviewInfo {
            source_width: src_w,
            source_height: src_h,
            scale,
            preview_width: pw,
            preview_height: ph,
            crop_x: 0,
            crop_y: 0,
        };
        (buf, info)
    }

    fn unique_preview_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "aseprite_mcp_finish_preview_{tag}_{}.png",
            std::process::id()
        ))
    }

    #[test]
    fn finish_preview_default_draws_a_legible_gutter() {
        // 4x4 source at 16x = 64x64 preview; step 8 -> 128px tick spacing (legible).
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = unique_preview_path("legible");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), None, None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(v["scale"], json!(16));
        assert_eq!(v["preview"]["width"], json!(64));
        assert_eq!(v["gutter_applied"], json!(true));
        assert!(v.get("gutter_skipped").is_none(), "should not skip: {out}");
        let g = &v["gutter"];
        assert!(g["left_w"].as_u64().unwrap() > 0);
        assert!(g["top_h"].as_u64().unwrap() > 0);
        assert_eq!(g["step"], json!(8));

        // The written PNG is the gutter'd (larger) image, matching the reported dims.
        let png = image::open(&dst).unwrap();
        assert_eq!(png.width(), g["image"]["width"].as_u64().unwrap() as u32);
        assert_eq!(png.height(), g["image"]["height"].as_u64().unwrap() as u32);
        assert!(png.width() > 64 && png.height() > 64, "gutter must grow the image");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_default_degrades_when_gutter_is_unreadable() {
        // 16x16 source at 1x; step 8 -> 8px spacing < 24 floor -> skip, keep preview.
        let (buf, info) = preview_buffer(16, 16, 1);
        let dst = unique_preview_path("degrade");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), None, None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(v["gutter_applied"], json!(false));
        assert!(v.get("gutter").is_none(), "no gutter expected: {out}");
        assert!(v["gutter_skipped"].is_string(), "expected a skip note: {out}");

        // The written PNG is the bare upscaled preview (no gutter growth).
        let png = image::open(&dst).unwrap();
        assert_eq!((png.width(), png.height()), (16, 16));
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_explicit_gutter_refuses_loudly_when_unreadable() {
        // Same too-dense case, but gutter:true -> hard error, no file policy change.
        let (buf, info) = preview_buffer(16, 16, 1);
        let dst = unique_preview_path("refuse");
        let err = finish_preview(buf, info, &dst.to_string_lossy(), Some(true), None, MarksInput::None).unwrap_err();
        assert!(err.contains("gutter_unreadable"), "got: {err}");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_gutter_false_writes_bare_preview() {
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = unique_preview_path("off");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["gutter_applied"], json!(false));
        assert!(v.get("gutter").is_none());
        assert!(v.get("gutter_skipped").is_none());

        let png = image::open(&dst).unwrap();
        assert_eq!((png.width(), png.height()), (64, 64));
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_explicit_gutter_true_succeeds_when_legible() {
        // The success twin of the refusal test: gutter:true on a legible scale draws.
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = unique_preview_path("explicit_ok");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(true), None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["gutter_applied"], json!(true));
        assert!(v["gutter"]["left_w"].as_u64().unwrap() > 0);
        let png = image::open(&dst).unwrap();
        assert!(png.width() > 64 && png.height() > 64);
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_transparent_sprite_still_draws_a_gutter() {
        // Empty palette (fully transparent art) must not break pick_label_color — it
        // falls back to a default candidate — and the gutter still composites + writes.
        let buf = image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 0]));
        let info = crate::preview::PreviewInfo {
            source_width: 4,
            source_height: 4,
            scale: 16,
            preview_width: 64,
            preview_height: 64,
            crop_x: 0,
            crop_y: 0,
        };
        let dst = unique_preview_path("transparent");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), None, None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["gutter_applied"], json!(true), "got: {out}");
        assert!(image::open(&dst).is_ok());
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_reports_write_failure_loudly() {
        // An unwritable dst (parent dir does not exist) surfaces preview_render_failed
        // rather than panicking. Deterministic + cross-platform: the dir is never created.
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = std::env::temp_dir()
            .join("aseprite_mcp_finish_preview_nodir_xyz")
            .join("out.png");
        let err = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::None).unwrap_err();
        assert!(err.contains("preview_render_failed"), "got: {err}");
    }

    // ---- crop resolution (SPEC-005 Phase 2) -------------------------------------

    #[test]
    fn resolve_crop_plan_handles_modes_and_rects() {
        use crate::preview::Crop;
        assert_eq!(resolve_crop_plan(&None).unwrap(), CropPlan::Whole);
        assert_eq!(
            resolve_crop_plan(&Some(LiveCrop::Mode("sprite".into()))).unwrap(),
            CropPlan::Whole
        );
        assert_eq!(
            resolve_crop_plan(&Some(LiveCrop::Mode("cel".into()))).unwrap(),
            CropPlan::Cel
        );
        assert_eq!(
            resolve_crop_plan(&Some(LiveCrop::Rect(LiveRect { x: 4, y: 6, width: 8, height: 9 }))).unwrap(),
            CropPlan::Explicit(Crop { x: 4, y: 6, width: 8, height: 9 })
        );
        // Unknown mode + negative origin + zero size all rejected with invalid_crop.
        assert!(resolve_crop_plan(&Some(LiveCrop::Mode("frame".into()))).unwrap_err().contains("invalid_crop"));
        assert!(resolve_crop_plan(&Some(LiveCrop::Rect(LiveRect { x: -1, y: 0, width: 4, height: 4 }))).unwrap_err().contains("invalid_crop"));
        assert!(resolve_crop_plan(&Some(LiveCrop::Rect(LiveRect { x: 0, y: 0, width: 0, height: 4 }))).unwrap_err().contains("invalid_crop"));
    }

    #[test]
    fn cel_crop_from_response_parses_clamps_and_degrades() {
        use crate::preview::Crop;
        // Normal bounds map straight through.
        let r = json!({ "changed": true, "cel": { "x": 12, "y": 8, "width": 16, "height": 20 } }).to_string();
        assert_eq!(cel_crop_from_response(&r).unwrap(), Crop { x: 12, y: 8, width: 16, height: 20 });
        // A negative origin clamps to the canvas, shrinking the size to the visible part.
        let r = json!({ "cel": { "x": -5, "y": 0, "width": 20, "height": 10 } }).to_string();
        assert_eq!(cel_crop_from_response(&r).unwrap(), Crop { x: 0, y: 0, width: 15, height: 10 });
        // No cel reported (old plugin or empty layer) -> loud degrade, not a guess.
        let r = json!({ "changed": true, "width": 64, "height": 64 }).to_string();
        assert!(cel_crop_from_response(&r).unwrap_err().contains("cel_bounds_unavailable"));
        // A cel entirely off-canvas has no visible area.
        let r = json!({ "cel": { "x": -30, "y": 0, "width": 10, "height": 10 } }).to_string();
        assert!(cel_crop_from_response(&r).unwrap_err().contains("cel_bounds_unavailable"));
    }

    // ---- Set-of-Mark region parsing (SPEC-005 Phase 4) --------------------------

    #[test]
    fn parse_slice_regions_maps_named_bounds() {
        let resp = json!({
            "slices": [
                { "name": "weapon", "bounds": { "x": 20, "y": 4, "width": 6, "height": 14 } },
                { "name": "head", "bounds": { "x": 0, "y": 0, "width": 8, "height": 8 } },
                { "name": "nobnds" }, // no bounds -> skipped
            ]
        }).to_string();
        let regions = parse_slice_regions(&resp);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0], crate::marks::Region {
            name: "weapon".into(),
            bbox: crate::marks::MarkRect { x: 20, y: 4, width: 6, height: 14 },
        });
        assert_eq!(regions[1].name, "head");
        // No slices array -> empty (not an error).
        assert!(parse_slice_regions("{}").is_empty());
    }

    #[test]
    fn parse_slice_regions_fallback_names_and_skips_zero_bounds() {
        let resp = json!({
            "slices": [
                { "name": "", "bounds": { "x": 1, "y": 2, "width": 3, "height": 4 } }, // empty name -> slice-1
                { "name": "z", "bounds": { "x": 0, "y": 0, "width": 0, "height": 5 } }, // zero width -> skipped
            ]
        }).to_string();
        let regions = parse_slice_regions(&resp);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "slice-1");
        assert_eq!(regions[0].bbox, crate::marks::MarkRect { x: 1, y: 2, width: 3, height: 4 });
    }

    #[test]
    fn parse_layer_regions_honours_group_effective_visibility() {
        // ArmL is visible by its own flag but sits inside a HIDDEN group -> the preview
        // doesn't render it, so it must NOT be marked.
        let layers = json!({
            "layers": [
                { "name": "Body", "isVisible": true },
                { "name": "Grp", "isVisible": false, "layers": [
                    { "name": "ArmL", "isVisible": true }
                ]},
            ]
        }).to_string();
        let cels = json!({
            "cels": [
                { "layer": "Body", "x": 0, "y": 0, "image": { "width": 8, "height": 8 } },
                { "layer": "ArmL", "x": 4, "y": 4, "image": { "width": 4, "height": 4 } },
            ]
        }).to_string();
        let regions = parse_layer_regions(&cels, &layers);
        assert_eq!(regions.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["Body"]);
    }

    #[test]
    fn parse_layer_regions_disambiguates_duplicate_names() {
        // Two visible layers both named "Body" -> two regions "Body" and "Body #2".
        let layers = json!({
            "layers": [
                { "name": "Body", "isVisible": true },
                { "name": "Body", "isVisible": true },
            ]
        }).to_string();
        let cels = json!({
            "cels": [
                { "layer": "Body", "x": 0, "y": 0, "image": { "width": 8, "height": 8 } },
                { "layer": "Body", "x": 2, "y": 2, "image": { "width": 4, "height": 4 } },
            ]
        }).to_string();
        let regions = parse_layer_regions(&cels, &layers);
        assert_eq!(regions.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["Body", "Body #2"]);
    }

    #[test]
    fn parse_layer_regions_all_hidden_emits_no_marks_but_unparseable_falls_back() {
        // Every layer hidden -> visible set parsed-but-empty -> NO marks (not "don't filter").
        let layers = json!({ "layers": [ { "name": "A", "isVisible": false } ] }).to_string();
        let cels = json!({ "cels": [ { "layer": "A", "x": 0, "y": 0, "image": { "width": 8, "height": 8 } } ] }).to_string();
        assert!(parse_layer_regions(&cels, &layers).is_empty());
        // No parseable layers info -> can't determine visibility -> don't filter (fallback).
        assert_eq!(parse_layer_regions(&cels, "not json").len(), 1);
        assert_eq!(parse_layer_regions(&cels, "{}").len(), 1);
    }

    #[test]
    fn parse_layer_regions_uses_visible_cels_at_frame() {
        let layers = json!({
            "layers": [
                { "name": "Body", "isVisible": true },
                { "name": "Hidden", "isVisible": false },
                { "name": "Group", "isVisible": true, "layers": [
                    { "name": "ArmL", "isVisible": true }
                ]},
            ]
        }).to_string();
        let cels = json!({
            "cels": [
                { "layer": "Body", "frame": 2, "x": 4, "y": 6, "image": { "width": 16, "height": 20 } },
                { "layer": "Hidden", "frame": 2, "x": 0, "y": 0, "image": { "width": 8, "height": 8 } },
                { "layer": "ArmL", "frame": 2, "x": 10, "y": 2, "image": { "width": 6, "height": 12 } },
                { "layer": "Body", "frame": 2, "x": 0, "y": 0, "image": { "width": 0, "height": 0 } }, // empty image -> skip
            ]
        }).to_string();
        let regions = parse_layer_regions(&cels, &layers);
        // Body + ArmL are visible; Hidden filtered; empty-image cel skipped.
        let names: Vec<&str> = regions.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Body", "ArmL"]);
        assert_eq!(regions[0].bbox, crate::marks::MarkRect { x: 4, y: 6, width: 16, height: 20 });
        assert_eq!(regions[1].bbox, crate::marks::MarkRect { x: 10, y: 2, width: 6, height: 12 });
    }

    #[test]
    fn active_frame_from_response_reads_or_defaults() {
        assert_eq!(active_frame_from_response(&json!({ "frame": 3 }).to_string()), 3);
        assert_eq!(active_frame_from_response("{}"), 1);
        assert_eq!(active_frame_from_response("not json"), 1);
    }

    #[test]
    fn downsample_by_scale_reconstructs_source_exactly() {
        // A 2×2 source upscaled 4x = 8×8 buffer; sampling per block returns the source.
        let mut src = image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 0]));
        let a = image::Rgba([200, 30, 20, 255]);
        let b = image::Rgba([20, 30, 200, 255]);
        for dy in 0..4 {
            for dx in 0..4 {
                src.put_pixel(dx, dy, a); // source (0,0)
                src.put_pixel(4 + dx, 4 + dy, b); // source (1,1)
            }
        }
        let ds = downsample_by_scale(&src, 4);
        assert_eq!((ds.width(), ds.height()), (2, 2));
        assert_eq!(*ds.get_pixel(0, 0), a);
        assert_eq!(*ds.get_pixel(1, 1), b);
        assert_eq!(ds.get_pixel(1, 0).0[3], 0); // transparent source cell
    }

    #[test]
    fn finish_preview_components_marks_appear_in_json_and_image() {
        // A 4×4 buffer at 8x with two separated opaque blocks -> 2 components -> 2 marks.
        let scale = 8u32;
        let mut buf = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 0]));
        // Block A at source (0,0); block B at source (3,3) (1px each, well separated).
        for dy in 0..scale {
            for dx in 0..scale {
                buf.put_pixel(dx, dy, image::Rgba([200, 30, 20, 255]));
                buf.put_pixel(3 * scale + dx, 3 * scale + dy, image::Rgba([30, 200, 20, 255]));
            }
        }
        let info = crate::preview::PreviewInfo {
            source_width: 4, source_height: 4, scale,
            preview_width: 32, preview_height: 32, crop_x: 0, crop_y: 0,
        };
        let dst = unique_preview_path("marks_components");
        // gutter:false to isolate the mark drawing/JSON from the gutter band.
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::Components).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let marks = v["marks"].as_array().expect("marks array");
        assert_eq!(marks.len(), 2, "got: {out}");
        assert_eq!(marks[0]["n"], json!(1));
        assert_eq!(marks[0]["region"], json!("component-1"));
        // First component bbox is the source (0,0,1,1) block.
        assert_eq!(marks[0]["bbox"]["x"], json!(0));
        assert_eq!(marks[1]["bbox"]["x"], json!(3));
        // The written PNG exists and carries badge pixels (it's no longer all-transparent
        // in the gutter band region — badges drew a backing box).
        assert!(image::open(&dst).is_ok());
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_no_marks_omits_the_field() {
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = unique_preview_path("no_marks");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("marks").is_none());
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_regions_filters_to_crop_window_and_renumbers() {
        // Crop window is the whole 8×8 source at 8x. Two regions inside + one outside;
        // only the in-window ones get marks, numbered 1..2 contiguously.
        let buf = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
        let info = crate::preview::PreviewInfo {
            source_width: 8, source_height: 8, scale: 8,
            preview_width: 64, preview_height: 64, crop_x: 0, crop_y: 0,
        };
        let regions = vec![
            crate::marks::Region { name: "in_a".into(), bbox: crate::marks::MarkRect { x: 1, y: 1, width: 2, height: 2 } },
            crate::marks::Region { name: "outside".into(), bbox: crate::marks::MarkRect { x: 40, y: 40, width: 4, height: 4 } },
            crate::marks::Region { name: "in_b".into(), bbox: crate::marks::MarkRect { x: 5, y: 5, width: 2, height: 2 } },
        ];
        let dst = unique_preview_path("marks_regions");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::Regions(regions)).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let marks = v["marks"].as_array().unwrap();
        assert_eq!(marks.len(), 2, "outside-crop region must be filtered: {out}");
        assert_eq!(marks[0]["region"], json!("in_a"));
        assert_eq!(marks[0]["n"], json!(1));
        assert_eq!(marks[1]["region"], json!("in_b"));
        assert_eq!(marks[1]["n"], json!(2)); // contiguous renumbering, no orphan #2
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_marks_under_nonzero_crop_report_full_sprite_coords() {
        // Source 4×4 cropped from (16,16) of a bigger sprite, at 8x (buffer 32×32). One
        // opaque block at buffer (0,0) = source (16,16); one explicit region at (18,18)
        // (inside the crop window) and one at (4,4) (outside → filtered).
        let scale = 8u32;
        let mut buf = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 0]));
        for dy in 0..scale {
            for dx in 0..scale {
                buf.put_pixel(dx, dy, image::Rgba([200, 30, 20, 255]));
            }
        }
        let info = crate::preview::PreviewInfo {
            source_width: 4, source_height: 4, scale,
            preview_width: 32, preview_height: 32, crop_x: 16, crop_y: 16,
        };
        // Components: the bbox must be in FULL-sprite coords (offset by the crop origin).
        let dst = unique_preview_path("marks_crop_comp");
        let out = finish_preview(buf.clone(), info, &dst.to_string_lossy(), Some(false), None, MarksInput::Components).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let marks = v["marks"].as_array().unwrap();
        assert_eq!(marks.len(), 1, "got: {out}");
        assert_eq!(marks[0]["bbox"]["x"], json!(16));
        assert_eq!(marks[0]["bbox"]["y"], json!(16));
        let _ = std::fs::remove_file(&dst);

        // Regions: in-window vs outside, evaluated in source (full-sprite) coords.
        let regions = vec![
            crate::marks::Region { name: "inside".into(), bbox: crate::marks::MarkRect { x: 18, y: 18, width: 2, height: 2 } },
            crate::marks::Region { name: "outside".into(), bbox: crate::marks::MarkRect { x: 4, y: 4, width: 2, height: 2 } },
        ];
        let dst = unique_preview_path("marks_crop_reg");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::Regions(regions)).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let marks = v["marks"].as_array().unwrap();
        assert_eq!(marks.len(), 1, "only the in-window region survives: {out}");
        assert_eq!(marks[0]["region"], json!("inside"));
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_marks_compose_over_an_applied_gutter_band() {
        // Default gutter on (4×4 @16x is legible) + components: assert the gutter applied
        // AND a badge drew into the ART quadrant (a BAND_BG box at band + centroid offset —
        // BAND_BG there can only come from a badge, since the art fill is a different colour).
        let (buf, info) = preview_buffer(4, 4, 16); // fully opaque -> one component
        let dst = unique_preview_path("marks_gutter");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), None, None, MarksInput::Components).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["gutter_applied"], json!(true), "got: {out}");
        let marks = v["marks"].as_array().unwrap();
        assert_eq!(marks.len(), 1);
        let (lw, th) = (
            v["gutter"]["left_w"].as_u64().unwrap() as u32,
            v["gutter"]["top_h"].as_u64().unwrap() as u32,
        );
        let img = image::open(&dst).unwrap().to_rgba8();
        let band_bg = crate::gutter::BAND_BG;
        // Scan the art quadrant (x>=left_w, y>=top_h) for a badge backing pixel.
        let mut found = false;
        for y in th..img.height() {
            for x in lw..img.width() {
                if *img.get_pixel(x, y) == band_bg {
                    found = true;
                }
            }
        }
        assert!(found, "no badge box found in the art quadrant (band offset not applied?)");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_empty_regions_emit_empty_marks_array() {
        // marks requested but none placed -> `marks: []` present (distinguishes
        // "requested, none" from "not requested").
        let (buf, info) = preview_buffer(4, 4, 16);
        let dst = unique_preview_path("marks_empty");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::Regions(vec![])).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["marks"], json!([]));
        assert!(v.get("marks_truncated").is_none());
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn finish_preview_truncates_excess_marks_keeping_largest() {
        // More than MAX_MARKS regions -> only the largest MAX_MARKS are badged, and
        // marks_truncated reports the true total. A big region must survive.
        let buf = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
        let info = crate::preview::PreviewInfo {
            source_width: 8, source_height: 8, scale: 8,
            preview_width: 64, preview_height: 64, crop_x: 0, crop_y: 0,
        };
        let mut regions: Vec<crate::marks::Region> = (0..(MAX_MARKS + 5))
            .map(|i| crate::marks::Region {
                name: format!("r{i}"),
                bbox: crate::marks::MarkRect { x: 1, y: 1, width: 1, height: 1 },
            })
            .collect();
        regions.push(crate::marks::Region { name: "big".into(), bbox: crate::marks::MarkRect { x: 2, y: 2, width: 4, height: 4 } });
        let total = regions.len();
        let dst = unique_preview_path("marks_trunc");
        let out = finish_preview(buf, info, &dst.to_string_lossy(), Some(false), None, MarksInput::Regions(regions)).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["marks"].as_array().unwrap().len(), MAX_MARKS);
        assert_eq!(v["marks_truncated"], json!(total));
        // The largest region (area 16) is kept, sorted to the front.
        assert_eq!(v["marks"][0]["region"], json!("big"));
        let _ = std::fs::remove_file(&dst);
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
            LiveImportReferenceParams,
            LiveUseToolParams,
            LivePoint,
            LiveEmptyParams,
            LiveSpriteSelectorParams,
            LiveOpenSpriteParams,
            LiveSaveSpriteAsParams,
            LiveSavePreviewParams,
            LiveAsciiViewParams,
            LiveSaveFilmstripParams,
            LiveFrameDiffParams,
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
            LiveCreateAutotileTemplateParams,
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
