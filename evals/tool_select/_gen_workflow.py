"""Generate the selector-agent workflow JS (embeds the surface + cases data)."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
surface = json.load(open(os.path.join(HERE, "surface.json"), encoding="utf-8"))
cases = json.load(open(os.path.join(HERE, "cases.json"), encoding="utf-8"))

FLAT = sorted(surface["weights"].keys())
CORE = surface["core"]
HINTS = {
    "color": "palette snap/recolour, ordered dithering, gradient-map onto a ramp, palette resize/colours, style-profile extraction, colour-mode",
    "animation": "frames, animation tags, filmstrip review, frame-diff, import an animation sheet, onion/ascii view",
    "tilemap": "tilemap layers, dedupe a mockup into a tileset, stamp tiles, autotile templates, tileset get/list/export, per-tile data",
    "transform": "rotate, resize canvas, cel create/clear/delete/list/properties",
    "layers": "create group layer, rename/delete layer, layer visibility/properties, ai-draft layer",
    "slices_sel": "slices new/list/set/delete, selection get/set, active site",
    "io_escape": "export sprite/spritesheet, save-as / save-copy, run lua / cli escape hatch, sprite properties, app commands, activate/close sprite",
}
CASES = [{"id": c["id"], "prompt": c["prompt"]} for c in cases["cases"]]
PROFILE_ENUM = ["core"] + list(HINTS.keys())

js = f"""export const meta = {{
  name: 'tool-select-measure',
  description: 'Selector agents pick a tool under a FLAT vs GATED surface (tool-surface measurement)',
  phases: [{{ title: 'Select', detail: '14 cases x {{flat, gated}}' }}],
}}

const FLAT = {json.dumps(FLAT)}
const CORE = {json.dumps(CORE)}
const HINTS = {json.dumps(HINTS)}
const CASES = {json.dumps(CASES)}

const FLAT_SCHEMA = {{
  type: 'object', additionalProperties: false, required: ['chosen_tool', 'reasoning'],
  properties: {{ chosen_tool: {{ type: 'string' }}, reasoning: {{ type: 'string' }} }},
}}
const GATED_SCHEMA = {{
  type: 'object', additionalProperties: false, required: ['open_profile', 'chosen_tool', 'reasoning'],
  properties: {{
    open_profile: {{ type: 'string', enum: {json.dumps(PROFILE_ENUM)} }},
    chosen_tool: {{ type: 'string' }}, reasoning: {{ type: 'string' }},
  }},
}}

function flatBrief(c) {{
  return `You route a pixel-art editing request to exactly ONE tool of an Aseprite MCP server.\\n\\nRequest: "${{c.prompt}}"\\n\\nALL available tools (flat list) — pick the single best one:\\n${{FLAT.join(', ')}}\\n\\nReturn chosen_tool (exact tool name from the list) and a one-line reasoning.`
}}
function gatedBrief(c) {{
  const groups = Object.entries(HINTS).map(([k, v]) => `- ${{k}}: ${{v}}`).join('\\n')
  return `You route a pixel-art editing request on an Aseprite MCP server. You currently SEE only these CORE tools:\\n${{CORE.join(', ')}}\\n\\nIf no core tool fits, you may OPEN exactly one tool GROUP — then that group's tools become available. Groups:\\n${{groups}}\\n\\nRequest: "${{c.prompt}}"\\n\\nDecide where to route: set open_profile to "core" if a core tool already fits, else the group you would open. Also name the tool you expect to call (chosen_tool — your best guess; you cannot see a group's tools until you open it). One-line reasoning.`
}}

const flat = await parallel(CASES.map((c) => () =>
  agent(flatBrief(c), {{ label: `flat:${{c.id}}`, phase: 'Select', schema: FLAT_SCHEMA, agentType: 'general-purpose' }})
    .then((r) => ({{ id: c.id, ...r }}))))

const gated = await parallel(CASES.map((c) => () =>
  agent(gatedBrief(c), {{ label: `gated:${{c.id}}`, phase: 'Select', schema: GATED_SCHEMA, agentType: 'general-purpose' }})
    .then((r) => ({{ id: c.id, ...r }}))))

return {{ flat, gated }}
"""
open(os.path.join(HERE, "_wf_select.js"), "w", encoding="utf-8").write(js)
print("wrote _wf_select.js", len(js), "chars;", len(CASES), "cases x 2 conditions")
