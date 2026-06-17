# Live Command Matrix

This matrix is the Level 4 source of truth for the Rust MCP live tool surface and the Lua plugin protocol commands.

Protocol: `aseprite-live-edit` v1. The wire version stays **1** across plugin
builds; new command families are gated by `get_capabilities.features` (e.g.
`"tilemap"`, SPEC-003) and an old plugin rejects an unknown command loudly with
`unsupported_command` rather than a breaking version mismatch (ADR-0005).

| Area | Lua command | Rust MCP tool | Params | Result | Smoke case |
| --- | --- | --- | --- | --- | --- |
| Capabilities | `get_capabilities` | `live_get_capabilities` | none | protocol/plugin/Aseprite versions, command list | status/capabilities |
| Session | `list_sprites` | `live_list_sprites` | none | open sprites | status/capabilities |
| Session | `open_sprite` | `live_open_sprite` | `filename` | sprite info | temp sprite open |
| Session | `activate_sprite` | `live_activate_sprite` | `filename` or `index` | sprite info | temp sprite activate |
| Session | `get_active_site` | `live_get_active_site` | none | active sprite/layer/frame/cel | status/capabilities |
| Sprite | `get_sprite_info` | `live_get_sprite_info` | none | active sprite info | status/capabilities |
| Sprite | `set_sprite_properties` | `live_set_sprite_properties` | metadata/grid/pixel ratio/color | sprite info | metadata roundtrip |
| Sprite | `save_sprite` | `live_save_sprite` | none | filename | save temp sprite |
| Sprite | `save_sprite_as` | `live_save_sprite_as` | `filename` | filename | save temp copy |
| Sprite | `save_copy_as` | `live_save_copy_as` | `filename` | filename | save temp copy |
| Sprite | `save_copy_as` (+ Rust upscale) | `live_save_preview` | `filename`, `scale?` | source/scale/preview sizes | vision-legible preview (nearest-neighbor ~1024px) |
| Sprite | `save_preview` (+ Rust pixels→text) | `live_ascii_view` | none | text grid + legend | token-space readback (1 glyph/pixel); ≤64×64 |
| Sprite | per-frame `save_preview` (+ Rust compose) | `live_save_filmstrip` | `filename`, `scale?` | frames/cols/rows + strip size | all frames in one grid for animation review |
| Sprite | two-frame `save_preview` (+ Rust diff) | `live_frame_diff` | `from_frame`, `to_frame` | diff text grid (changed-cell count + legend) | pixel-level diff of two frames; ≤64×64 |
| Sprite | `close_sprite` | `live_close_sprite` | `filename` or `index`, `save?` | closed sprite | close temp sprite |
| Sprite | `resize_canvas` | `live_resize_canvas` | `width`, `height` | sprite info | canvas temp sprite |
| Layer | `list_layers` | `live_list_layers` | none | layer list | layer flow |
| Layer | `ensure_layer` | `live_ensure_layer` | `name` | layer | layer flow |
| Layer | `set_active_layer` | `live_set_active_layer` | `name` | layer info | layer flow |
| Layer | `rename_layer` | `live_rename_layer` | `name`, `new_name` | layer info | layer flow |
| Layer | `create_group_layer` | `live_create_group_layer` | `name`, `parent?` | group info | group layer flow |
| Layer | `set_layer_visibility` | `live_set_layer_visibility` | `name`, `visible` | visibility | layer flow |
| Layer | `set_layer_properties` | `live_set_layer_properties` | visible/editable/opacity/blend/stack/parent | layer info | layer flow |
| Layer | `delete_layer` | `live_delete_layer` | `name` | changed flag | layer cleanup |
| Frame | `ensure_frames` | `live_ensure_frames` | `count`, `duration?` | frame count | frame flow |
| Frame | `list_frames` | `live_list_frames` | none | frame list | frame flow |
| Frame | `set_active_frame` | `live_set_active_frame` | `frame?` | frame info | frame flow |
| Frame | `set_frame_properties` | `live_set_frame_properties` | `frame`, `duration?` | frame info | frame flow |
| Frame | `new_empty_frame` | `live_new_empty_frame` | `index?`, `duration?` | frame info | frame flow |
| Frame | `new_frame` | `live_new_frame` | `frame?`, `source_frame?`, `duration?` | frame info | frame copy flow |
| Frame | `delete_frame` | `live_delete_frame` | `frame?` | frame count | frame cleanup |
| Cel | `list_cels` | `live_list_cels` | `layer?`, `frame?` | cel list | cel flow |
| Cel | `new_cel` | `live_new_cel` | layer/frame/position/opacity/replace | cel info | cel flow |
| Cel | `set_cel_properties` | `live_set_cel_properties` | layer/frame/position/opacity/zIndex/data | cel info | cel flow |
| Cel | `delete_cel` | `live_delete_cel` | layer/frame | changed flag | cel cleanup |
| Cel | `clear_cel` | `live_clear_cel` | target layer/frame | changed flag | draw flow |
| Drawing | `draw_pixels` | `live_draw_pixels` | pixels, target layer/frame | changed count | draw flow |
| Drawing | `use_tool` | `live_use_tool` | tool/points/color/brush/target | changed count | draw tool flow |
| Tags | `list_tags` | `live_list_tags` | none | tag list | tag flow |
| Tags | `new_tag` | `live_new_tag` | name/frame range/repeats/color/data | tag info | tag flow |
| Tags | `set_tag_properties` | `live_set_tag_properties` | name/new name/repeats/color/data | tag info | tag flow |
| Tags | `delete_tag` | `live_delete_tag` | name | changed flag | tag cleanup |
| Slices | `list_slices` | `live_list_slices` | none | slice list | slice flow |
| Slices | `new_slice` | `live_new_slice` | name/bounds/center/pivot/color/data | slice info | slice flow |
| Slices | `set_slice_properties` | `live_set_slice_properties` | name/new name/bounds/center/pivot/color/data | slice info | slice flow |
| Slices | `delete_slice` | `live_delete_slice` | name | changed flag | slice cleanup |
| Selection | `get_selection` | `live_get_selection` | none | selection info | selection flow |
| Selection | `set_selection` | `live_set_selection` | mode/bounds | selection info | selection flow |
| Palette | `list_palette` | `live_list_palette` | palette/from/limit | colors | palette read |
| Palette | `set_palette_color` | `live_set_palette_color` | palette/index/color | color info | palette temp sprite |
| Palette | `resize_palette` | `live_resize_palette` | palette/count | palette size | palette temp sprite |
| Advanced | `run_app_command` | `live_run_app_command` | command name/params | sprite info | `DeselectMask` |
| Tilemap (SPEC-003) | `create_tilemap_layer` | `live_create_tilemap_layer` | `name`, `tile_width?`, `tile_height?` | layer + tileset info | tilemap-selftest |
| Tilemap (SPEC-003) | `list_tilesets` | `live_list_tilesets` | none | tilesets (index/name/count/grid) | tilemap-selftest |
| Tilemap (SPEC-003) | `get_tileset` | `live_get_tileset` | `index?` or `layer?`, `filename?`, `scale?` | tileset + per-tile data (+ upscaled packed PNG) | tilemap-selftest |
| Tilemap (SPEC-003) | `stamp_tiles` | `live_stamp_tiles` | `tiles[{x,y,tile_index}]`, `layer`, `frame?` | placed/skipped counts | tilemap-selftest |
| Tilemap (SPEC-003) | `set_tile_data` | `live_set_tile_data` | `tile_index`, `tileset_index?`/`layer?`, `data?` | tile data | tilemap-selftest |
| Tilemap (SPEC-003) | `pack_similar_tiles` | `live_pack_similar_tiles` | `tile_width`, `tile_height?`, `layer?`, `tilemap_layer?` | dedupe stats (cells→unique) | tilemap-selftest |
| Tilemap (SPEC-003) | `export_tilemap` (+ Rust serialize) | `live_export_tileset` | `target`, `path`, `layer?`, `frame?`, `layout?`, `image_columns?` | engine file + packed PNG | tilemap-selftest |
| Colour ops (SPEC-004) | `get_region_colors` + `apply_color_map` | `live_palette_snap` | `layer?`, `frame?`, `selection_only?` | snapped colours/pixels + mapping | colour-ops E2E |
| Colour ops (SPEC-004) | `get_region_colors` + `apply_color_map` | `live_adjust_pixels` | `op`, `amount?`, `hue?`, `clamp_to_palette?`, `layer?`, `frame?`, `selection_only?` | changed colours/pixels | colour-ops E2E |
| Colour ops (SPEC-004) | `get_region_colors` (palette) | `live_snap_colors` | `colors[]` | input→snapped hex (no edit) | colour-ops E2E |

