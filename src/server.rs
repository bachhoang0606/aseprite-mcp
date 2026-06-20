use rmcp::handler::server::tool::Parameters;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext},
    model::*,
    service::RequestContext,
    tool, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};

use crate::aseprite::{AsepriteRunner, ScriptOutput};
use crate::live::LiveBridge;
use crate::tools;

// ============================================================================
// AsepriteServer
// ============================================================================

/// The MCP server: owns the offline CLI runner, the live WebSocket bridge, and the
/// generated tool router.
#[derive(Debug, Clone)]
pub struct AsepriteServer {
    runner: Arc<AsepriteRunner>,
    live: Arc<LiveBridge>,
    /// Base directory for generated files (`ASEPRITE_OUTPUT_DIR`); relative output
    /// paths resolve against it when set.
    output_dir: Option<PathBuf>,
    tool_router: ToolRouter<Self>,
}

/// Read `ASEPRITE_OUTPUT_DIR`, creating the directory if it does not exist yet.
/// Returns `None` when the variable is unset.
fn resolve_output_dir() -> Option<PathBuf> {
    let dir = std::env::var("ASEPRITE_OUTPUT_DIR").ok()?;
    let path = PathBuf::from(dir);
    if !path.exists() {
        info!("creating output directory {}", path.display());
        std::fs::create_dir_all(&path).ok();
    }
    info!("output directory: {}", path.display());
    Some(path)
}

#[tool_router]
impl AsepriteServer {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            runner: Arc::new(AsepriteRunner::new()?),
            live: LiveBridge::start_from_env(),
            output_dir: resolve_output_dir(),
            tool_router: Self::tool_router(),
        })
    }

    // ========================================================================
    // Live Aseprite Tools (WebSocket plugin)
    // ========================================================================

    #[tool(description = "Return the connection status for the live Aseprite WebSocket plugin.")]
    async fn live_status(&self) -> Result<String, String> {
        Ok(self.live.status_json().await)
    }

    #[tool(
        description = "PREFLIGHT GUARD — call this BEFORE any drawing or sprite-editing workflow. \
        Returns ready=true only when the live Aseprite plugin is connected. If ready=false, STOP \
        and report to the user: do NOT fall back to batch/file drawing tools (they edit files on \
        disk and will not appear in the open Aseprite window). The response includes a directive \
        and remediation steps."
    )]
    async fn live_preflight(&self) -> Result<String, String> {
        Ok(self.live.preflight().await)
    }

    #[tool(
        description = "Return live protocol/plugin capabilities reported by the connected Aseprite plugin."
    )]
    async fn live_get_capabilities(&self) -> Result<String, String> {
        self.live.get_capabilities().await
    }

    #[tool(description = "List sprites currently open in the running Aseprite UI session.")]
    async fn live_list_sprites(&self) -> Result<String, String> {
        self.live.list_sprites().await
    }

    #[tool(
        description = "Open a sprite file in the running Aseprite UI session via the live plugin."
    )]
    async fn live_open_sprite(
        &self,
        params: Parameters<crate::live::LiveOpenSpriteParams>,
    ) -> Result<String, String> {
        self.live.open_sprite(params.0).await
    }

    #[tool(
        description = "Activate an already-open sprite by filename or 1-based index in the running Aseprite UI session."
    )]
    async fn live_activate_sprite(
        &self,
        params: Parameters<crate::live::LiveSpriteSelectorParams>,
    ) -> Result<String, String> {
        self.live.activate_sprite(params.0).await
    }

    #[tool(
        description = "Get active sprite/layer/frame/cel site information from the running Aseprite UI session."
    )]
    async fn live_get_active_site(&self) -> Result<String, String> {
        self.live.get_active_site().await
    }

    #[tool(
        description = "Get information about the currently active sprite in a running Aseprite instance via the live plugin."
    )]
    async fn live_get_sprite_info(&self) -> Result<String, String> {
        self.live.get_sprite_info().await
    }

    #[tool(
        description = "Legacy live helper: create or reuse a layer in the active Aseprite sprite. Prefer live_ensure_layer for new workflows."
    )]
    async fn live_ensure_ai_draft_layer(
        &self,
        params: Parameters<crate::live::LiveLayerParams>,
    ) -> Result<String, String> {
        self.live.ensure_ai_draft_layer(params.0.layer).await
    }

    #[tool(
        description = "Set metadata/properties on the active live sprite: data, tab color, grid bounds, pixel ratio, or transparent palette index."
    )]
    async fn live_set_sprite_properties(
        &self,
        params: Parameters<crate::live::LiveSpritePropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_sprite_properties(params.0).await
    }

    #[tool(description = "Save the active sprite in the running Aseprite UI session.")]
    async fn live_save_sprite(&self) -> Result<String, String> {
        self.live.save_sprite().await
    }

    #[tool(
        description = "Save the active live sprite to a new filename and keep editing that file."
    )]
    async fn live_save_sprite_as(
        &self,
        params: Parameters<crate::live::LiveSaveSpriteAsParams>,
    ) -> Result<String, String> {
        self.live.save_sprite_as(params.0).await
    }

    #[tool(
        description = "Save a copy of the active live sprite to a filename without changing the active file."
    )]
    async fn live_save_copy_as(
        &self,
        params: Parameters<crate::live::LiveSaveSpriteAsParams>,
    ) -> Result<String, String> {
        self.live.save_copy_as(params.0).await
    }

    #[tool(
        description = "Save a vision-legible PNG preview of the active live sprite, nearest-neighbor \
            upscaled so the sprite's long edge lands near ~1024px. Raw 1x previews of small sprites \
            are below the resolution a vision model can read reliably, so use THIS (not \
            live_save_copy_as) whenever an agent needs to SEE its own work. Pass an integer `scale` \
            to override, or omit it for an automatic factor (capped at 16x). By default the preview \
            gets a labelled coordinate gutter (numeric ticks along the top + left) so you can name \
            the exact source (x,y) of a pixel to fix. Check `gutter_applied` in the result: when \
            true, invert with source_x = (preview_x - gutter.left_w) / scale and source_y = \
            (preview_y - gutter.top_h) / scale; when false (suppressed, or auto-degraded on a sprite \
            too large for a legible gutter — see `gutter_skipped`) the file is the bare upscaled art, \
            so source_x = preview_x / scale and source_y = preview_y / scale. Set `gutter:false` to \
            suppress it, `gutter:true` to require it (errors if illegible), or tune `gutter_step` \
            (default 8 source-px). Returns source size, chosen scale, preview size, and (when drawn) \
            the gutter band extents — so preview pixels map back to sprite coordinates exactly."
    )]
    async fn live_save_preview(
        &self,
        params: Parameters<crate::live::LiveSavePreviewParams>,
    ) -> Result<String, String> {
        self.live.save_preview(params.0).await
    }

    #[tool(
        description = "Read the active live sprite's frame back as a TEXT grid: one glyph per pixel \
            ('.' = transparent), with row/column rulers and a colour legend mapping each glyph to its \
            hex colour. An LLM reads this token-space grid far more reliably than it reads a small \
            sprite image, so use it to VERIFY exact pixel values/positions, count cells, or check work \
            on non-vision clients — complementary to live_save_preview (which is for the human/vision \
            view). Active frame, all visible layers; refuses sprites larger than 64x64 (crop first)."
    )]
    async fn live_ascii_view(
        &self,
        params: Parameters<crate::live::LiveAsciiViewParams>,
    ) -> Result<String, String> {
        self.live.ascii_view(params.0).await
    }

    #[tool(
        description = "Save a vision-legible PNG that composites EVERY animation frame into one \
            near-square grid (row-major, gray gaps between cells), nearest-neighbor upscaled. The \
            Claude API only reads the first frame of an animated GIF, so this strip is the way to \
            REVIEW an animation (walk/attack cycle) in a single image — check timing and cross-frame \
            proportion drift. The user's active frame is restored. Optional integer `scale`; returns \
            frames/cols/rows + the upscaled strip size."
    )]
    async fn live_save_filmstrip(
        &self,
        params: Parameters<crate::live::LiveSaveFilmstripParams>,
    ) -> Result<String, String> {
        self.live.save_filmstrip(params.0).await
    }

    #[tool(
        description = "Diff two animation frames as a TEXT grid: '.' = unchanged, '-' = erased \
            (became transparent), otherwise the glyph of the NEW colour at that cell (with a \
            glyph→#rrggbb legend), plus a changed-cell count. Lets the agent see EXACTLY where two \
            frames differ at the pixel level — verify what an edit changed, or inspect motion between \
            frames. Pass 1-based `from_frame` and `to_frame`; the user's active frame is restored."
    )]
    async fn live_frame_diff(
        &self,
        params: Parameters<crate::live::LiveFrameDiffParams>,
    ) -> Result<String, String> {
        self.live.frame_diff(params.0).await
    }

    #[tool(
        description = "Close a sprite in the running Aseprite UI session by filename or 1-based index."
    )]
    async fn live_close_sprite(
        &self,
        params: Parameters<crate::live::LiveCloseSpriteParams>,
    ) -> Result<String, String> {
        self.live.close_sprite(params.0).await
    }

    #[tool(
        description = "Change the active live sprite canvas size using Aseprite CanvasSize without reopening the file."
    )]
    async fn live_resize_canvas(
        &self,
        params: Parameters<crate::live::LiveResizeCanvasParams>,
    ) -> Result<String, String> {
        self.live.resize_canvas(params.0).await
    }

    #[tool(
        description = "List layers in the active live sprite including visibility, opacity, stack index, and hierarchy metadata."
    )]
    async fn live_list_layers(&self) -> Result<String, String> {
        self.live.list_layers().await
    }

    #[tool(description = "Create or reuse a named layer in the active live sprite.")]
    async fn live_ensure_layer(
        &self,
        params: Parameters<crate::live::LiveLayerNameParams>,
    ) -> Result<String, String> {
        self.live.ensure_layer(params.0).await
    }

    #[tool(description = "Set the active layer in the running Aseprite UI session.")]
    async fn live_set_active_layer(
        &self,
        params: Parameters<crate::live::LiveLayerNameParams>,
    ) -> Result<String, String> {
        self.live.set_active_layer(params.0).await
    }

    #[tool(description = "Rename a layer in the active live sprite.")]
    async fn live_rename_layer(
        &self,
        params: Parameters<crate::live::LiveRenameLayerParams>,
    ) -> Result<String, String> {
        self.live.rename_layer(params.0).await
    }

    #[tool(
        description = "Create a group layer in the active live sprite, optionally under a parent group."
    )]
    async fn live_create_group_layer(
        &self,
        params: Parameters<crate::live::LiveCreateGroupLayerParams>,
    ) -> Result<String, String> {
        self.live.create_group_layer(params.0).await
    }

    #[tool(description = "Set layer visibility in the active live sprite.")]
    async fn live_set_layer_visibility(
        &self,
        params: Parameters<crate::live::LiveSetLayerVisibilityParams>,
    ) -> Result<String, String> {
        self.live.set_layer_visibility(params.0).await
    }

    #[tool(
        description = "Set layer properties in the active live sprite: visible, editable, opacity, blend mode, stack index, or parent."
    )]
    async fn live_set_layer_properties(
        &self,
        params: Parameters<crate::live::LiveSetLayerPropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_layer_properties(params.0).await
    }

    #[tool(description = "Delete a layer from the active live sprite by name.")]
    async fn live_delete_layer(
        &self,
        params: Parameters<crate::live::LiveLayerNameParams>,
    ) -> Result<String, String> {
        self.live.delete_layer(params.0).await
    }

    #[tool(
        description = "Ensure the active Aseprite sprite has at least the requested number of animation frames via the live plugin."
    )]
    async fn live_ensure_frames(
        &self,
        params: Parameters<crate::live::LiveEnsureFramesParams>,
    ) -> Result<String, String> {
        self.live.ensure_frames(params.0).await
    }

    #[tool(
        description = "List animation frames in the active live sprite with frame numbers and durations."
    )]
    async fn live_list_frames(&self) -> Result<String, String> {
        self.live.list_frames().await
    }

    #[tool(description = "Set the active frame in the running Aseprite UI session.")]
    async fn live_set_active_frame(
        &self,
        params: Parameters<crate::live::LiveFrameSelectorParams>,
    ) -> Result<String, String> {
        self.live.set_active_frame(params.0).await
    }

    #[tool(description = "Set live frame properties such as duration in seconds.")]
    async fn live_set_frame_properties(
        &self,
        params: Parameters<crate::live::LiveSetFramePropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_frame_properties(params.0).await
    }

    #[tool(description = "Create an empty frame in the active live sprite.")]
    async fn live_new_empty_frame(
        &self,
        params: Parameters<crate::live::LiveNewEmptyFrameParams>,
    ) -> Result<String, String> {
        self.live.new_empty_frame(params.0).await
    }

    #[tool(description = "Create a new frame by copying an existing live frame.")]
    async fn live_new_frame(
        &self,
        params: Parameters<crate::live::LiveNewFrameParams>,
    ) -> Result<String, String> {
        self.live.new_frame(params.0).await
    }

    #[tool(description = "Delete a frame from the active live sprite.")]
    async fn live_delete_frame(
        &self,
        params: Parameters<crate::live::LiveFrameSelectorParams>,
    ) -> Result<String, String> {
        self.live.delete_frame(params.0).await
    }

    #[tool(
        description = "List cels in the active live sprite, optionally filtered by layer and frame."
    )]
    async fn live_list_cels(
        &self,
        params: Parameters<crate::live::LiveListCelsParams>,
    ) -> Result<String, String> {
        self.live.list_cels(params.0).await
    }

    #[tool(description = "Create an empty cel in the active live sprite at a layer and frame.")]
    async fn live_new_cel(
        &self,
        params: Parameters<crate::live::LiveNewCelParams>,
    ) -> Result<String, String> {
        self.live.new_cel(params.0).await
    }

    #[tool(description = "Set live cel properties such as position, opacity, z-index, or data.")]
    async fn live_set_cel_properties(
        &self,
        params: Parameters<crate::live::LiveSetCelPropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_cel_properties(params.0).await
    }

    #[tool(description = "Delete a cel in the active live sprite at a layer and frame.")]
    async fn live_delete_cel(
        &self,
        params: Parameters<crate::live::LiveDeleteCelParams>,
    ) -> Result<String, String> {
        self.live.delete_cel(params.0).await
    }

    #[tool(
        description = "Clear a layer/frame cel in the active Aseprite sprite via the live plugin. Defaults to the AI Draft layer and active frame."
    )]
    async fn live_clear_cel(
        &self,
        params: Parameters<crate::live::LiveClearCelParams>,
    ) -> Result<String, String> {
        self.live.clear_cel(params.0).await
    }

    #[tool(
        description = "Draw a batch of pixels into the active Aseprite sprite via the live plugin. Defaults to the AI Draft layer and active frame."
    )]
    async fn live_draw_pixels(
        &self,
        params: Parameters<crate::live::LiveDrawPixelsParams>,
    ) -> Result<String, String> {
        self.live.draw_pixels(params.0).await
    }

    #[tool(
        description = "Use an Aseprite drawing tool in the active sprite via the live plugin. Defaults to the AI Draft layer and active frame."
    )]
    async fn live_use_tool(
        &self,
        params: Parameters<crate::live::LiveUseToolParams>,
    ) -> Result<String, String> {
        self.live.use_tool(params.0).await
    }

    #[tool(description = "List animation tags in the active live sprite.")]
    async fn live_list_tags(&self) -> Result<String, String> {
        self.live.list_tags().await
    }

    #[tool(description = "Create an animation tag in the active live sprite.")]
    async fn live_new_tag(
        &self,
        params: Parameters<crate::live::LiveNewTagParams>,
    ) -> Result<String, String> {
        self.live.new_tag(params.0).await
    }

    #[tool(
        description = "Set live animation tag properties such as name, repeats, color, or data."
    )]
    async fn live_set_tag_properties(
        &self,
        params: Parameters<crate::live::LiveSetTagPropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_tag_properties(params.0).await
    }

    #[tool(description = "Delete an animation tag from the active live sprite.")]
    async fn live_delete_tag(
        &self,
        params: Parameters<crate::live::LiveTagNameParams>,
    ) -> Result<String, String> {
        self.live.delete_tag(params.0).await
    }

    #[tool(description = "List slices in the active live sprite.")]
    async fn live_list_slices(&self) -> Result<String, String> {
        self.live.list_slices().await
    }

    #[tool(
        description = "Create a slice in the active live sprite with optional bounds, center, pivot, color, and data."
    )]
    async fn live_new_slice(
        &self,
        params: Parameters<crate::live::LiveNewSliceParams>,
    ) -> Result<String, String> {
        self.live.new_slice(params.0).await
    }

    #[tool(
        description = "Set live slice properties such as name, bounds, center, pivot, color, or data."
    )]
    async fn live_set_slice_properties(
        &self,
        params: Parameters<crate::live::LiveSetSlicePropertiesParams>,
    ) -> Result<String, String> {
        self.live.set_slice_properties(params.0).await
    }

    #[tool(description = "Delete a slice from the active live sprite.")]
    async fn live_delete_slice(
        &self,
        params: Parameters<crate::live::LiveTagNameParams>,
    ) -> Result<String, String> {
        self.live.delete_slice(params.0).await
    }

    #[tool(description = "Get the current selection in the active live sprite.")]
    async fn live_get_selection(&self) -> Result<String, String> {
        self.live.get_selection().await
    }

    #[tool(
        description = "Set the current live selection using replace, add, subtract, intersect, select_all, or deselect mode."
    )]
    async fn live_set_selection(
        &self,
        params: Parameters<crate::live::LiveSetSelectionParams>,
    ) -> Result<String, String> {
        self.live.set_selection(params.0).await
    }

    #[tool(description = "List palette colors from the active live sprite.")]
    async fn live_list_palette(
        &self,
        params: Parameters<crate::live::LiveListPaletteParams>,
    ) -> Result<String, String> {
        self.live.list_palette(params.0).await
    }

    #[tool(description = "Set one palette color in the active live sprite.")]
    async fn live_set_palette_color(
        &self,
        params: Parameters<crate::live::LiveSetPaletteColorParams>,
    ) -> Result<String, String> {
        self.live.set_palette_color(params.0).await
    }

    #[tool(description = "Resize a palette in the active live sprite.")]
    async fn live_resize_palette(
        &self,
        params: Parameters<crate::live::LiveResizePaletteParams>,
    ) -> Result<String, String> {
        self.live.resize_palette(params.0).await
    }

    #[tool(
        description = "Privileged: run a whitelisted-by-identifier Aseprite app.command in the active live UI session."
    )]
    async fn live_run_app_command(
        &self,
        params: Parameters<crate::live::LiveRunAppCommandParams>,
    ) -> Result<String, String> {
        self.live.run_app_command(params.0).await
    }

    // ========================================================================
    // Tilemap Tools (SPEC-003) — needs a plugin advertising the 'tilemap'
    // capability (check live_get_capabilities); older plugins reject these
    // commands loudly rather than silently no-op.
    // ========================================================================

    #[tool(
        description = "SPEC-003: Create a new tilemap layer (and its empty tileset) in the active live sprite with the given tile size. Requires a plugin that advertises the 'tilemap' capability (live_get_capabilities)."
    )]
    async fn live_create_tilemap_layer(
        &self,
        params: Parameters<crate::live::LiveCreateTilemapLayerParams>,
    ) -> Result<String, String> {
        self.live.create_tilemap_layer(params.0).await
    }

    #[tool(
        description = "SPEC-003: List the tilesets in the active live sprite with 1-based index, name, tile count, grid tile-size, and base index."
    )]
    async fn live_list_tilesets(&self) -> Result<String, String> {
        self.live.list_tilesets().await
    }

    #[tool(
        description = "SPEC-003: Get a tileset's metadata, and (when `filename` is set) a vision-legible packed PNG of its tiles, nearest-neighbor upscaled like live_save_preview so the agent can SEE them. Select the tileset by 1-based `index` or by tilemap `layer`."
    )]
    async fn live_get_tileset(
        &self,
        params: Parameters<crate::live::LiveGetTilesetParams>,
    ) -> Result<String, String> {
        self.live.get_tileset(params.0).await
    }

    #[tool(
        description = "SPEC-003: Stamp a batch of tiles into a tilemap layer — the tile-grid analogue of live_draw_pixels (each {x, y, tile_index}). x/y are tile-grid cells (columns/rows), NOT pixels."
    )]
    async fn live_stamp_tiles(
        &self,
        params: Parameters<crate::live::LiveStampTilesParams>,
    ) -> Result<String, String> {
        self.live.stamp_tiles(params.0).await
    }

    #[tool(
        description = "SPEC-003: Set the user-data string on a tile (e.g. a terrain/collision tag). It is stored on the tile in the .aseprite file and read back by live_get_tileset; NOTE: live_export_tileset does NOT yet emit per-tile data — Tiled wangsets come from the blob47 layout, not from this field. Select the tileset by 1-based `tileset_index` or by tilemap `layer`."
    )]
    async fn live_set_tile_data(
        &self,
        params: Parameters<crate::live::LiveSetTileDataParams>,
    ) -> Result<String, String> {
        self.live.set_tile_data(params.0).await
    }

    #[tool(
        description = "SPEC-003: Turn a painted mockup layer into a deduplicated tileset + a tilemap layer that reconstructs it pixel-for-pixel (port of Aseprite's Pack Similar Tiles). Returns efficiency stats (cells -> unique tiles)."
    )]
    async fn live_pack_similar_tiles(
        &self,
        params: Parameters<crate::live::LivePackSimilarTilesParams>,
    ) -> Result<String, String> {
        self.live.pack_similar_tiles(params.0).await
    }

    #[tool(
        description = "SPEC-003: Export the active tilemap to an engine file — 'tiled' (.tsj, with a blob47 wangset when layout=blob47), 'godot' (.tres TileSet), or 'json' — plus a sibling packed tileset PNG. Exports the WHOLE canvas. (LDtk needs no exporter: it reads .aseprite directly with hot-reload — use live_save_sprite.)"
    )]
    async fn live_export_tileset(
        &self,
        params: Parameters<crate::live::LiveExportTilesetParams>,
    ) -> Result<String, String> {
        self.live.export_tileset(params.0).await
    }

    // ========================================================================
    // Constrained / semantic colour ops (SPEC-004) — needs a plugin advertising
    // the 'color_ops' capability (live_get_capabilities). RGB sprites only.
    // ========================================================================

    #[tool(
        description = "SPEC-004: Snap every off-palette colour in a layer/selection to its perceptually-nearest palette colour (real CIELAB ΔE, not RGBA euclidean) — makes the region palette-legal by construction. RGB sprites only; pass selection_only to limit to the active selection."
    )]
    async fn live_palette_snap(
        &self,
        params: Parameters<crate::live::LivePaletteSnapParams>,
    ) -> Result<String, String> {
        self.live.palette_snap(params.0).await
    }

    #[tool(
        description = "SPEC-004: Shade pixels by INTENT in a layer/selection. op = darken | lighten (value shift + rule-correct hue-shift: shadows cool toward blue, highlights warm toward orange) | hue_shift (amount = degrees) | colorize (hue = target degrees) | snap. clamp_to_palette (default true) keeps the result palette-legal. RGB sprites only."
    )]
    async fn live_adjust_pixels(
        &self,
        params: Parameters<crate::live::LiveAdjustPixelsParams>,
    ) -> Result<String, String> {
        self.live.adjust_pixels(params.0).await
    }

    #[tool(
        description = "SPEC-004: Snap a list of hex colours to the active sprite's palette (real CIELAB nearest) WITHOUT editing the sprite — so a stroke's colours can be made legal BEFORE live_draw_pixels. Returns each input + its snapped hex."
    )]
    async fn live_snap_colors(
        &self,
        params: Parameters<crate::live::LiveSnapColorsParams>,
    ) -> Result<String, String> {
        self.live.snap_colors(params.0).await
    }

    // ========================================================================
    // Export Tools
    // ========================================================================

    #[tool(
        description = "Export a sprite FILE to another image format (png/gif/jpg/bmp/webp/…), with an optional integer scale and layer/tag filter. Offline, via the Aseprite CLI."
    )]
    async fn export_sprite(
        &self,
        params: Parameters<tools::export::ExportSpriteParams>,
    ) -> Result<String, String> {
        tools::export::export_sprite(self, params.0).await
    }

    #[tool(
        description = "Pack a sprite FILE's frames into a spritesheet image (+ optional JSON metadata: frame tags, layers, slices). Layouts: horizontal, vertical, rows, columns, packed. Offline, via the Aseprite CLI."
    )]
    async fn export_spritesheet(
        &self,
        params: Parameters<tools::export::ExportSpritesheetParams>,
    ) -> Result<String, String> {
        tools::export::export_spritesheet(self, params.0).await
    }

    // ========================================================================
    // Script & Command Execution
    // ========================================================================

    #[tool(
        description = "Run arbitrary Lua in Aseprite's scripting environment (full API access; print() returns data; optionally open a file first). SECURITY: this is code execution on the host and is OFF by default — set ASEPRITE_MCP_ALLOW_LUA=1 in the server environment to enable (see SECURITY.md / ADR-0003)."
    )]
    async fn run_lua_script(
        &self,
        params: Parameters<tools::scripting::RunLuaScriptParams>,
    ) -> Result<String, String> {
        tools::scripting::run_lua_script(self, params.0).await
    }

    #[tool(
        description = "Run Aseprite in batch mode with raw CLI arguments — for format conversions and exports best expressed on the command line. SECURITY: the CLI can run --script (arbitrary code), so this is OFF by default — set ASEPRITE_MCP_ALLOW_LUA=1 to enable (see SECURITY.md / ADR-0003)."
    )]
    async fn execute_cli(
        &self,
        params: Parameters<tools::scripting::ExecuteCliParams>,
    ) -> Result<String, String> {
        tools::scripting::execute_cli(self, params.0).await
    }

    #[tool(
        description = "Change a sprite FILE's colour mode to 'rgb', 'grayscale', or 'indexed' offline (via the Aseprite CLI), saving in place. Runs a fixed, safe operation (not gated). For the live editor, edit the open sprite instead."
    )]
    async fn change_color_mode(
        &self,
        params: Parameters<tools::scripting::ChangeColorModeParams>,
    ) -> Result<String, String> {
        tools::scripting::change_color_mode(self, params.0).await
    }
}

// ============================================================================
// Public Helper Methods — used by tool modules
// ============================================================================

impl AsepriteServer {
    /// Run a Lua script with no sprite pre-loaded.
    pub async fn execute_script(&self, script: &str) -> Result<String, String> {
        self.finish_script(self.runner.run_script(script).await, None)
    }

    /// Run a Lua script with `file_path` opened first.
    pub async fn execute_script_on_file(
        &self,
        file_path: &str,
        script: &str,
    ) -> Result<String, String> {
        self.finish_script(
            self.runner.run_script_on_file(file_path, script).await,
            Some(file_path),
        )
    }

    /// Shared tail for a script run: log failures and collapse to an Ok/Err string.
    fn finish_script(
        &self,
        outcome: anyhow::Result<ScriptOutput>,
        on_file: Option<&str>,
    ) -> Result<String, String> {
        let suffix = on_file.map(|f| format!(" on {f}")).unwrap_or_default();
        match outcome {
            Ok(o) if o.success => Ok(o.result_text()),
            Ok(o) => {
                error!("Aseprite script error{suffix}: {}", o.stderr);
                Err(o.result_text())
            }
            Err(e) => {
                error!("Aseprite script failed{suffix}: {e}");
                Err(format!("failed to execute script: {e}"))
            }
        }
    }

    /// Resolve a tool's output path: a *relative* path joins the configured
    /// `ASEPRITE_OUTPUT_DIR` (when set); absolute paths (or no configured dir) pass
    /// through unchanged.
    pub fn resolve_output_path(&self, path: &str) -> String {
        match &self.output_dir {
            Some(base) if Path::new(path).is_relative() => {
                base.join(path).to_string_lossy().into_owned()
            }
            _ => path.to_string(),
        }
    }

    /// Run Aseprite with raw CLI args (batch mode). Exposed for tool modules.
    pub async fn run_cli(&self, args: &[String]) -> anyhow::Result<ScriptOutput> {
        self.runner.run_cli(args).await
    }
}

// ============================================================================
// ServerHandler Implementation
// ============================================================================

impl ServerHandler for AsepriteServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            server_info: Implementation {
                name: "aseprite-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Live-first Aseprite MCP server. Before any drawing or sprite-editing workflow, \
                 call live_preflight (or live_status) and only use the live_* tools once connected \
                 is true; if it is not, stop and tell the user the live Aseprite session is not \
                 connected — do not silently write to disk instead, since file edits will not show \
                 up in the open editor. The few offline tools (export_sprite, export_spritesheet, \
                 change_color_mode, and the gated run_lua_script / execute_cli escape hatch) are \
                 for deliberate file-level operations only. Paths may be absolute or relative to \
                 the working directory; colours are hex '#rrggbb' or '#rrggbbaa'."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ctx = ToolCallContext::new(self, request, context);
        async move { self.tool_router.call(ctx).await }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
        }))
    }
}
