"""Build surface.json: the core set + workflow profiles + per-tool token weights.

This is the 'pruned' surface model the harness measures against the flat 77-tool surface.
Profiles group tools by workflow; `core` is always-on. Weights approximate each tool's
JSON-schema token cost (a few headline tools are large; most are ~150-300).
"""
import json

names = json.load(open("evals/tool_select/_tool_names.json"))

# Explicit profile assignment (every tool lands in exactly one group; core is always-on).
CORE = [
    "live_preflight", "live_status", "live_get_sprite_info", "live_get_capabilities",
    "live_list_layers", "live_list_frames", "live_list_palette", "live_list_sprites",
    "live_draw_pixels", "live_use_tool", "live_save_preview", "live_open_sprite",
    "live_save_sprite", "live_ensure_layer", "live_ensure_frames",
    "live_set_active_layer", "live_set_active_frame", "live_import_reference",
]
PROFILES = {
    "color": [
        "live_palette_snap", "live_adjust_pixels", "live_snap_colors", "live_dither_fill",
        "live_gradient_map", "live_set_palette_color", "live_resize_palette",
        "live_extract_style_profile", "change_color_mode",
    ],
    "animation": [
        "live_save_filmstrip", "live_frame_diff", "live_set_frame_properties",
        "live_new_empty_frame", "live_new_frame", "live_delete_frame", "live_new_tag",
        "live_set_tag_properties", "live_delete_tag", "live_list_tags",
        "live_import_animation", "live_ascii_view",
    ],
    "tilemap": [
        "live_create_tilemap_layer", "live_pack_similar_tiles", "live_stamp_tiles",
        "live_get_tileset", "live_list_tilesets", "live_export_tileset",
        "live_set_tile_data", "live_create_autotile_template",
    ],
    "transform": [
        "live_rotate", "live_resize_canvas", "live_clear_cel", "live_new_cel",
        "live_delete_cel", "live_set_cel_properties", "live_list_cels",
    ],
    "layers": [
        "live_create_group_layer", "live_rename_layer", "live_delete_layer",
        "live_set_layer_properties", "live_set_layer_visibility", "live_ensure_ai_draft_layer",
    ],
    "slices_sel": [
        "live_new_slice", "live_list_slices", "live_set_slice_properties",
        "live_delete_slice", "live_get_selection", "live_set_selection", "live_get_active_site",
    ],
    "io_escape": [
        "export_sprite", "export_spritesheet", "run_lua_script", "execute_cli",
        "live_save_copy_as", "live_save_sprite_as", "live_run_app_command",
        "live_activate_sprite", "live_close_sprite", "live_set_sprite_properties",
    ],
}

# Headline tools whose schemas are large (approx token weight); everything else defaults.
WEIGHT = {
    "live_save_preview": 2500, "live_import_reference": 1000, "live_import_animation": 600,
    "export_spritesheet": 620, "live_save_filmstrip": 420, "live_adjust_pixels": 610,
    "live_stamp_tiles": 520, "live_set_tile_data": 450, "live_get_tileset": 460,
    "live_new_slice": 530, "live_set_slice_properties": 535, "live_set_sprite_properties": 466,
    "live_use_tool": 505, "live_draw_pixels": 405, "live_create_autotile_template": 520,
    "live_rotate": 470, "live_oscillate": 400, "live_export_tileset": 550,
    "live_palette_snap": 348, "live_set_cel_properties": 280, "live_set_selection": 240,
}
DEFAULT_WEIGHT = 200
NAME_LISTING_PER_TOOL = 18  # cheap "name + 1-line" cost when only the NAME is shown (deferred clients)

assigned = set(CORE) | {t for ts in PROFILES.values() for t in ts}
missing = [n for n in names if n not in assigned]
assert not missing, f"unassigned tools: {missing}"
extra = [t for t in assigned if t not in names]
assert not extra, f"profile names not in server: {extra}"

surface = {
    "core": CORE,
    "profiles": PROFILES,
    "weights": {n: WEIGHT.get(n, DEFAULT_WEIGHT) for n in names},
    "name_listing_per_tool": NAME_LISTING_PER_TOOL,
    "tool_to_profile": {**{t: "core" for t in CORE},
                        **{t: p for p, ts in PROFILES.items() for t in ts}},
    "n_tools": len(names),
}
json.dump(surface, open("evals/tool_select/surface.json", "w"), indent=2)
print(f"surface.json: {len(names)} tools, core={len(CORE)}, profiles={len(PROFILES)}")
print("profile sizes:", {p: len(ts) for p, ts in PROFILES.items()})
