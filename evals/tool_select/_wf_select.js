export const meta = {
  name: 'tool-select-measure',
  description: 'Selector agents pick a tool under a FLAT vs GATED surface (tool-surface measurement)',
  phases: [{ title: 'Select', detail: '14 cases x {flat, gated}' }],
}

const FLAT = ["change_color_mode", "execute_cli", "export_sprite", "export_spritesheet", "live_activate_sprite", "live_adjust_pixels", "live_ascii_view", "live_clear_cel", "live_close_sprite", "live_create_autotile_template", "live_create_group_layer", "live_create_tilemap_layer", "live_delete_cel", "live_delete_frame", "live_delete_layer", "live_delete_slice", "live_delete_tag", "live_dither_fill", "live_draw_pixels", "live_ensure_ai_draft_layer", "live_ensure_frames", "live_ensure_layer", "live_export_tileset", "live_extract_style_profile", "live_frame_diff", "live_get_active_site", "live_get_capabilities", "live_get_selection", "live_get_sprite_info", "live_get_tileset", "live_gradient_map", "live_import_animation", "live_import_reference", "live_list_cels", "live_list_frames", "live_list_layers", "live_list_palette", "live_list_slices", "live_list_sprites", "live_list_tags", "live_list_tilesets", "live_new_cel", "live_new_empty_frame", "live_new_frame", "live_new_slice", "live_new_tag", "live_open_sprite", "live_pack_similar_tiles", "live_palette_snap", "live_preflight", "live_rename_layer", "live_resize_canvas", "live_resize_palette", "live_rotate", "live_run_app_command", "live_save_copy_as", "live_save_filmstrip", "live_save_preview", "live_save_sprite", "live_save_sprite_as", "live_set_active_frame", "live_set_active_layer", "live_set_cel_properties", "live_set_frame_properties", "live_set_layer_properties", "live_set_layer_visibility", "live_set_palette_color", "live_set_selection", "live_set_slice_properties", "live_set_sprite_properties", "live_set_tag_properties", "live_set_tile_data", "live_snap_colors", "live_stamp_tiles", "live_status", "live_use_tool", "run_lua_script"]
const CORE = ["live_preflight", "live_status", "live_get_sprite_info", "live_get_capabilities", "live_list_layers", "live_list_frames", "live_list_palette", "live_list_sprites", "live_draw_pixels", "live_use_tool", "live_save_preview", "live_open_sprite", "live_save_sprite", "live_ensure_layer", "live_ensure_frames", "live_set_active_layer", "live_set_active_frame", "live_import_reference"]
const HINTS = {"color": "palette snap/recolour, ordered dithering, gradient-map onto a ramp, palette resize/colours, style-profile extraction, colour-mode", "animation": "frames, animation tags, filmstrip review, frame-diff, import an animation sheet, onion/ascii view", "tilemap": "tilemap layers, dedupe a mockup into a tileset, stamp tiles, autotile templates, tileset get/list/export, per-tile data", "transform": "rotate, resize canvas, cel create/clear/delete/list/properties", "layers": "create group layer, rename/delete layer, layer visibility/properties, ai-draft layer", "slices_sel": "slices new/list/set/delete, selection get/set, active site", "io_escape": "export sprite/spritesheet, save-as / save-copy, run lua / cli escape hatch, sprite properties, app commands, activate/close sprite"}
const CASES = [{"id": "dither", "prompt": "Shade this rectangle (x8 y10 w12 h6) with an ordered Bayer dither between skin-mid and skin-dark at 40%."}, {"id": "gradient", "prompt": "Re-map this rough grey blob so every colour snaps onto my green ramp dark->light."}, {"id": "palette_snap", "prompt": "These pixels drifted off-palette \u2014 snap them back to my locked palette using perceptual (CIELAB) colour distance."}, {"id": "style_profile", "prompt": "Extract a machine-checkable style profile (native grid, ramps with lint, light direction, heads-tall) from the active sprite."}, {"id": "rotate", "prompt": "Rotate the selected cel about 30 degrees onto a new layer without introducing any new colours."}, {"id": "import_anim", "prompt": "I have a 4-frame walk sprite-sheet PNG (1 row). Bring it in as an Aseprite animation on a shared 12-colour palette."}, {"id": "filmstrip", "prompt": "Save one image that shows every animation frame side by side so I can review the walk-cycle timing."}, {"id": "tag", "prompt": "Add a forward-loop animation tag named 'walk' spanning frames 1 to 4."}, {"id": "pack_tiles", "prompt": "Turn my painted level mockup layer into a deduplicated tileset plus a tilemap that reconstructs it."}, {"id": "autotile", "prompt": "I drew the 4 corner quarters (fill/outer/edge/inner). Generate the full 47-tile blob autotile sheet from them."}, {"id": "rename_layer", "prompt": "Rename the layer currently called 'Layer 1' to 'Body'."}, {"id": "export_sheet", "prompt": "Export the sprite as a packed spritesheet PNG plus a JSON data file that includes the frame tags."}, {"id": "draw", "prompt": "Draw this batch of 20 specific coloured pixels onto the Body layer."}, {"id": "preview", "prompt": "Give me a vision-legible upscaled preview of the sprite with a labelled coordinate gutter so I can read off pixel positions."}, {"id": "import_ref", "prompt": "Convert this reference photo into 48x48 palette-locked pixel art on a Reference layer so I can trace over it."}]

const FLAT_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['chosen_tool', 'reasoning'],
  properties: { chosen_tool: { type: 'string' }, reasoning: { type: 'string' } },
}
const GATED_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['open_profile', 'chosen_tool', 'reasoning'],
  properties: {
    open_profile: { type: 'string', enum: ["core", "color", "animation", "tilemap", "transform", "layers", "slices_sel", "io_escape"] },
    chosen_tool: { type: 'string' }, reasoning: { type: 'string' },
  },
}

function flatBrief(c) {
  return `You route a pixel-art editing request to exactly ONE tool of an Aseprite MCP server.\n\nRequest: "${c.prompt}"\n\nALL available tools (flat list) — pick the single best one:\n${FLAT.join(', ')}\n\nReturn chosen_tool (exact tool name from the list) and a one-line reasoning.`
}
function gatedBrief(c) {
  const groups = Object.entries(HINTS).map(([k, v]) => `- ${k}: ${v}`).join('\n')
  return `You route a pixel-art editing request on an Aseprite MCP server. You currently SEE only these CORE tools:\n${CORE.join(', ')}\n\nIf no core tool fits, you may OPEN exactly one tool GROUP — then that group's tools become available. Groups:\n${groups}\n\nRequest: "${c.prompt}"\n\nDecide where to route: set open_profile to "core" if a core tool already fits, else the group you would open. Also name the tool you expect to call (chosen_tool — your best guess; you cannot see a group's tools until you open it). One-line reasoning.`
}

const flat = await parallel(CASES.map((c) => () =>
  agent(flatBrief(c), { label: `flat:${c.id}`, phase: 'Select', schema: FLAT_SCHEMA, agentType: 'general-purpose' })
    .then((r) => ({ id: c.id, ...r }))))

const gated = await parallel(CASES.map((c) => () =>
  agent(gatedBrief(c), { label: `gated:${c.id}`, phase: 'Select', schema: GATED_SCHEMA, agentType: 'general-purpose' })
    .then((r) => ({ id: c.id, ...r }))))

return { flat, gated }
