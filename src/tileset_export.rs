//! Engine-export serializers for tilemaps / tilesets (SPEC-003 Phase 5).
//!
//! Pure, deterministic, file-format-only: given a normalized [`TilemapExport`]
//! (tile size, packed-tileset geometry, the row-major tile-index grid, and an
//! optional autotile layout) produce the *bytes* of a Tiled `.tsj`, a Godot
//! `.tres`, or a plain JSON map. There is no Aseprite and no I/O in this module —
//! the live tool fetches the data from the plugin, calls one of these to
//! serialize, and writes the file(s). Keeping the format rules here makes every
//! one unit-testable without the editor (mirrors `preview.rs` / `autotile.rs`).
//!
//! LDtk needs no serializer: it reads `.aseprite` directly with hot-reload, so
//! the deliverable there is `live_save_sprite` (documented, not emitted here).

use serde_json::{json, Value};

use crate::autotile;

/// Which autotile layout the tileset was built with. `None` = a hand-packed /
/// deduped tileset (no terrain semantics): engines get a plain tileset, no
/// wangsets/terrains. `Blob47` = the 47-state corner-masked blob set whose tile
/// order is [`autotile::blob47_masks`], so tile index *is* the mask slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    None,
    Blob47,
    Wang16,
}

impl Layout {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "plain" => Ok(Layout::None),
            "blob47" | "blob" => Ok(Layout::Blob47),
            "wang16" | "wang" => Ok(Layout::Wang16),
            other => Err(format!(
                "unknown layout '{other}' (expected none | blob47 | wang16)"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Layout::None => "none",
            Layout::Blob47 => "blob47",
            Layout::Wang16 => "wang16",
        }
    }
}

/// Which engine file to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Tiled,
    Godot,
    Json,
}

impl Target {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tiled" | "tsj" => Ok(Target::Tiled),
            "godot" | "tres" => Ok(Target::Godot),
            "json" => Ok(Target::Json),
            other => Err(format!(
                "unknown export target '{other}' (expected tiled | godot | json)"
            )),
        }
    }

    /// File extension the serialized bytes should be written with.
    pub fn extension(self) -> &'static str {
        match self {
            Target::Tiled => "tsj",
            Target::Godot => "tres",
            Target::Json => "json",
        }
    }
}

/// Normalized export input. `grid` is row-major — `grid[row][col]` is the tile
/// index at that cell (0 = the empty tile) — and is `rows` high by `columns`
/// wide. `image_name` is the packed-tileset PNG the plugin already wrote
/// (referenced by the engine file, ideally a sibling basename). `image_columns`
/// is how many tiles wide that PNG is, so `tile id -> (col,row)` is recoverable.
#[derive(Debug, Clone)]
pub struct TilemapExport {
    pub layer_name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub tile_count: u32,
    pub image_name: String,
    pub image_columns: u32,
    pub grid: Vec<Vec<i64>>,
    pub layout: Layout,
}

impl TilemapExport {
    /// Pixel width of the packed tileset PNG.
    fn image_width(&self) -> u32 {
        self.image_columns.max(1) * self.tile_width
    }

    /// Pixel height of the packed tileset PNG (rows needed to hold every tile).
    fn image_rows(&self) -> u32 {
        let cols = self.image_columns.max(1);
        self.tile_count.div_ceil(cols).max(1)
    }

    fn image_height(&self) -> u32 {
        self.image_rows() * self.tile_height
    }

    /// Validate the grid shape against the declared dims so a malformed plugin
    /// payload fails loudly here rather than producing a corrupt engine file.
    pub fn validate(&self) -> Result<(), String> {
        if self.tile_width == 0 || self.tile_height == 0 {
            return Err("tile_width and tile_height must be non-zero".into());
        }
        if self.grid.len() as u32 != self.rows {
            return Err(format!(
                "grid has {} rows but `rows` is {}",
                self.grid.len(),
                self.rows
            ));
        }
        for (r, row) in self.grid.iter().enumerate() {
            if row.len() as u32 != self.columns {
                return Err(format!(
                    "grid row {r} has {} cells but `columns` is {}",
                    row.len(),
                    self.columns
                ));
            }
        }
        Ok(())
    }
}

/// Serialize `export` for `target`. Pretty-prints JSON-shaped formats so the
/// output is diff-friendly and human-auditable.
pub fn serialize(export: &TilemapExport, target: Target) -> Result<String, String> {
    export.validate()?;
    Ok(match target {
        Target::Json => to_pretty(&export_json(export)),
        Target::Tiled => to_pretty(&export_tiled(export)),
        Target::Godot => export_godot(export),
    })
}

fn to_pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

// ----------------------------------------------------------------------------
// Plain JSON (engine-agnostic; the Gabinou shape — Phaser / custom engines)
// ----------------------------------------------------------------------------

fn export_json(e: &TilemapExport) -> Value {
    json!({
        "tileWidth": e.tile_width,
        "tileHeight": e.tile_height,
        "columns": e.columns,
        "rows": e.rows,
        "tileCount": e.tile_count,
        "layout": e.layout.as_str(),
        "image": e.image_name,
        "imageColumns": e.image_columns,
        "imageWidth": e.image_width(),
        "imageHeight": e.image_height(),
        "layers": [
            { "name": e.layer_name, "data": e.grid }
        ],
    })
}

// ----------------------------------------------------------------------------
// Tiled tileset (.tsj) — https://doc.mapeditor.org/en/stable/reference/json-map-format/
// A blob47 tileset also gets a `wangsets` entry so Tiled's terrain brush can
// autotile out of the box.
// ----------------------------------------------------------------------------

fn export_tiled(e: &TilemapExport) -> Value {
    let mut tileset = json!({
        "type": "tileset",
        "tiledversion": "1.10.2",
        "version": "1.10",
        "name": e.layer_name,
        "image": e.image_name,
        "imagewidth": e.image_width(),
        "imageheight": e.image_height(),
        "tilewidth": e.tile_width,
        "tileheight": e.tile_height,
        "columns": e.image_columns,
        "tilecount": e.tile_count,
        "margin": 0,
        "spacing": 0,
    });

    if e.layout == Layout::Blob47 {
        tileset["wangsets"] = json!([tiled_blob47_wangset(e.tile_count)]);
    }
    tileset
}

/// One Tiled "wangset" describing the blob47 terrain. Each tile id carries a
/// `wangid` — Tiled's 8-slot `[top, topright, right, bottomright, bottom,
/// bottomleft, left, topleft]` array — derived deterministically from the tile's
/// blob mask (tile id == mask slot in [`autotile::blob47_masks`]). Slot value 1
/// = "this terrain", 0 = "outside". Tiled uses this to pick the matching tile
/// for any neighbour configuration the user paints.
fn tiled_blob47_wangset(tile_count: u32) -> Value {
    let masks = autotile::blob47_masks();
    let wangtiles: Vec<Value> = masks
        .iter()
        .enumerate()
        .filter(|(i, _)| (*i as u32) < tile_count)
        .map(|(i, &mask)| {
            let id = blob47_wangid(mask);
            json!({
                "tileid": i,
                "wangid": id.to_vec(),
            })
        })
        .collect();

    json!({
        "name": "terrain",
        "type": "mixed",
        "tile": -1,
        "colors": [
            {
                "name": "terrain",
                "color": "#ff0000",
                "tile": -1,
                "probability": 1.0,
            }
        ],
        "wangtiles": wangtiles,
    })
}

/// Map a blob47 8-neighbour mask to a Tiled wangid (8 slots, terrain index 1
/// where the edge/corner is filled). Order matches Tiled:
/// `[top, topright, right, bottomright, bottom, bottomleft, left, topleft]`.
pub fn blob47_wangid(mask: u8) -> [u8; 8] {
    use autotile::{E, N, NE, NW, S, SE, SW, W};
    let f = |bit: u8| if mask & bit != 0 { 1u8 } else { 0u8 };
    [
        f(N),
        f(NE),
        f(E),
        f(SE),
        f(S),
        f(SW),
        f(W),
        f(NW),
    ]
}

// ----------------------------------------------------------------------------
// Godot 4 TileSet (.tres) — a TileSetAtlasSource over the packed PNG. Terrain
// peering is Tiled-only for now (documented limitation); this emits a valid,
// importable atlas with every tile declared so a tilemap can reference indices.
// ----------------------------------------------------------------------------

fn export_godot(e: &TilemapExport) -> String {
    // Two sub-resources: the texture (ext) and the atlas source. We use a stable
    // ext_resource id so re-exports diff cleanly.
    let cols = e.image_columns.max(1);
    let mut tiles = String::new();
    // Declare every tile id 0..tile_count at its (col,row) atlas coordinate.
    for i in 0..e.tile_count {
        let col = i % cols;
        let row = i / cols;
        // Godot atlas tile declaration: `<col>:<row>/0 = 0`.
        tiles.push_str(&format!("{col}:{row}/0 = 0\n"));
    }

    format!(
        "[gd_resource type=\"TileSet\" load_steps=3 format=3]\n\n\
         [ext_resource type=\"Texture2D\" path=\"res://{image}\" id=\"1_tiles\"]\n\n\
         [sub_resource type=\"TileSetAtlasSource\" id=\"TileSetAtlasSource_tiles\"]\n\
         texture = ExtResource(\"1_tiles\")\n\
         texture_region_size = Vector2i({tw}, {th})\n\
         {tiles}\n\
         [resource]\n\
         tile_size = Vector2i({tw}, {th})\n\
         sources/0 = SubResource(\"TileSetAtlasSource_tiles\")\n",
        image = e.image_name,
        tw = e.tile_width,
        th = e.tile_height,
        tiles = tiles,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(layout: Layout, tile_count: u32) -> TilemapExport {
        // 2x2 grid of indices over a 4-tile set, packed 2 tiles wide.
        TilemapExport {
            layer_name: "Ground".into(),
            tile_width: 16,
            tile_height: 16,
            columns: 2,
            rows: 2,
            tile_count,
            image_name: "level.png".into(),
            image_columns: 2,
            grid: vec![vec![0, 1], vec![2, 3]],
            layout,
        }
    }

    #[test]
    fn target_and_layout_parse_round_trip() {
        assert_eq!(Target::parse("tiled").unwrap(), Target::Tiled);
        assert_eq!(Target::parse("GODOT").unwrap(), Target::Godot);
        assert_eq!(Target::parse("json").unwrap(), Target::Json);
        assert!(Target::parse("unity").is_err());
        assert_eq!(Layout::parse("").unwrap(), Layout::None);
        assert_eq!(Layout::parse("blob47").unwrap(), Layout::Blob47);
        assert!(Layout::parse("hexagon").is_err());
        assert_eq!(Target::Tiled.extension(), "tsj");
        assert_eq!(Target::Godot.extension(), "tres");
        assert_eq!(Target::Json.extension(), "json");
    }

    #[test]
    fn validate_rejects_ragged_or_mismatched_grids() {
        let mut e = sample(Layout::None, 4);
        assert!(e.validate().is_ok());
        e.rows = 3; // grid still has 2 rows
        assert!(e.validate().is_err());
        let mut e2 = sample(Layout::None, 4);
        e2.grid[0].push(9); // ragged row
        assert!(e2.validate().is_err());
        let mut e3 = sample(Layout::None, 4);
        e3.tile_width = 0;
        assert!(e3.validate().is_err());
    }

    #[test]
    fn json_export_carries_grid_and_geometry() {
        let v = export_json(&sample(Layout::None, 4));
        assert_eq!(v["tileWidth"], 16);
        assert_eq!(v["columns"], 2);
        assert_eq!(v["rows"], 2);
        assert_eq!(v["tileCount"], 4);
        assert_eq!(v["image"], "level.png");
        assert_eq!(v["imageWidth"], 32); // 2 cols * 16
        assert_eq!(v["imageHeight"], 32); // ceil(4/2)=2 rows * 16
        assert_eq!(v["layers"][0]["name"], "Ground");
        assert_eq!(v["layers"][0]["data"], json!([[0, 1], [2, 3]]));
    }

    #[test]
    fn json_image_height_grows_with_tile_count() {
        // 47 tiles packed 8 wide -> ceil(47/8)=6 rows.
        let mut e = sample(Layout::None, 47);
        e.image_columns = 8;
        assert_eq!(export_json(&e)["imageHeight"], 6 * 16);
    }

    #[test]
    fn tiled_export_is_a_tileset_without_wangsets_when_plain() {
        let v = export_tiled(&sample(Layout::None, 4));
        assert_eq!(v["type"], "tileset");
        assert_eq!(v["tilewidth"], 16);
        assert_eq!(v["tilecount"], 4);
        assert_eq!(v["columns"], 2);
        assert!(v.get("wangsets").is_none(), "plain set has no wangsets");
    }

    #[test]
    fn tiled_blob47_export_has_one_wangset_with_a_wangtile_per_tile() {
        let count = autotile::blob47_masks().len() as u32; // 47
        let mut e = sample(Layout::Blob47, count);
        e.image_columns = 8;
        let v = export_tiled(&e);
        let wangsets = v["wangsets"].as_array().unwrap();
        assert_eq!(wangsets.len(), 1);
        let wangtiles = wangsets[0]["wangtiles"].as_array().unwrap();
        assert_eq!(wangtiles.len(), 47);
        // Every wangtile has an 8-slot wangid and a tileid inside the set.
        for wt in wangtiles {
            assert_eq!(wt["wangid"].as_array().unwrap().len(), 8);
            assert!((wt["tileid"].as_u64().unwrap() as usize) < 47);
        }
    }

    #[test]
    fn wangid_reflects_edges_and_corners() {
        use autotile::{E, N, NE, S};
        // Empty mask -> all zero.
        assert_eq!(blob47_wangid(0), [0, 0, 0, 0, 0, 0, 0, 0]);
        // N|E|NE -> top, topright, right set; others clear.
        let m = N | E | NE;
        // order: [top, topright, right, bottomright, bottom, bottomleft, left, topleft]
        assert_eq!(blob47_wangid(m), [1, 1, 1, 0, 0, 0, 0, 0]);
        // A lone S edge sets only the bottom slot.
        assert_eq!(blob47_wangid(S), [0, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn godot_export_is_a_valid_tileset_resource_with_every_tile() {
        let e = sample(Layout::None, 4);
        let tres = export_godot(&e);
        assert!(tres.starts_with("[gd_resource type=\"TileSet\""));
        assert!(tres.contains("texture_region_size = Vector2i(16, 16)"));
        assert!(tres.contains("tile_size = Vector2i(16, 16)"));
        assert!(tres.contains("res://level.png"));
        // 4 tiles packed 2 wide -> coords (0,0)(1,0)(0,1)(1,1).
        assert!(tres.contains("0:0/0 = 0"));
        assert!(tres.contains("1:0/0 = 0"));
        assert!(tres.contains("0:1/0 = 0"));
        assert!(tres.contains("1:1/0 = 0"));
    }

    #[test]
    fn serialize_dispatches_and_pretty_prints_json_targets() {
        let e = sample(Layout::Blob47, 47);
        let json_out = serialize(&e, Target::Json).unwrap();
        assert!(json_out.contains('\n'), "JSON is pretty-printed");
        assert!(serde_json::from_str::<Value>(&json_out).is_ok());
        let tsj = serialize(&e, Target::Tiled).unwrap();
        assert!(serde_json::from_str::<Value>(&tsj).is_ok());
        let tres = serialize(&e, Target::Godot).unwrap();
        assert!(tres.contains("gd_resource"));
    }
}
