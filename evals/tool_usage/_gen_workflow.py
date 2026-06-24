"""Generate the usage-correctness workflow JS (full vs trimmed tool descriptions)."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
desc = json.load(open(os.path.join(HERE, "descriptions.json"), encoding="utf-8"))
cases = json.load(open(os.path.join(HERE, "cases.json"), encoding="utf-8"))

# Param signatures (names + types/enums) — CONSTANT across conditions; the tool-level
# description is the only thing that varies. (No param-level docs -> conservative/worst-case.)
SIGS = {
    "live_save_preview": "filename:str(req), scale:int?, gutter:bool?, gutter_step:int?, "
        "crop:('sprite'|'cel'|{x,y,width,height})?, inline:bool?, marks_from:('slices'|'layers'|'components')?",
    "live_import_animation": "filename:str?, sheet:{cols:int,rows:int}?, frames:[str]?, width:int?, height:int?, "
        "method:('dominant'|'average')?, palette:[str]?, snap:bool?, auto_colors:int?, "
        "palette_method:('median_cut'|'kmeans'|'frequency')?, regrid:bool?, layer:str?, start_frame:int?, "
        "tag:str?, fps:float?, at_x:int?, at_y:int?",
    "live_import_reference": "filename:str(req), width:int?, height:int?, method:('dominant'|'average')?, "
        "palette:[str]?, snap:bool?, auto_colors:int?, palette_method:('median_cut'|'kmeans'|'frequency')?, "
        "regrid:bool?, layer:str?, frame:int?, at_x:int?, at_y:int?",
    "live_rotate": "angle:float(req,deg +cw), at_x:int?, at_y:int?, frame:int?, layer:str?, "
        "rect:{x,y,width,height}?, selection_only:bool?",
    "live_dither_fill": "rect:{x,y,width,height}(req), color_a:str(req), color_b:str(req), level:float?, "
        "matrix:('bayer4'|'bayer2'|'checker')?, layer:str?, frame:int?",
    "live_create_autotile_template": "tile_size:int(req,even), layout:('blob47'|'wang16')?, source_x:int?, "
        "source_y:int?, at_x:int?, at_y:int?, layer:str?, frame:int?",
}
CASES = [{"id": c["id"], "tool": c["tool"], "type": c["type"], "prompt": c["prompt"]} for c in cases["cases"]]
DFULL = {t: d["full"] for t, d in desc.items()}
DTRIM = {t: d["trimmed"] for t, d in desc.items()}

js = f"""export const meta = {{
  name: 'tool-usage-measure',
  description: 'Usage-correctness: emit a tool call under FULL vs TRIMMED tool description',
  phases: [{{ title: 'Use', detail: '8 cases x {{full, trimmed}}' }}],
}}

const SIGS = {json.dumps(SIGS)}
const DFULL = {json.dumps(DFULL)}
const DTRIM = {json.dumps(DTRIM)}
const CASES = {json.dumps(CASES)}

const SCHEMA = {{
  type: 'object', additionalProperties: false, required: ['params', 'reasoning'],
  properties: {{ params: {{ type: 'object' }}, reasoning: {{ type: 'string' }} }},
}}

function brief(c, description) {{
  if (c.type === 'compute') {{
    return `You are using the MCP tool \\`${{c.tool}}\\`. Its description:\\n"${{description}}"\\n\\nTask: ${{c.prompt}}\\n\\nReturn your answer in \\`params\\` as integer fields source_x and source_y. Show the arithmetic in reasoning.`
  }}
  return `You are calling the MCP tool \\`${{c.tool}}\\`. Parameters (name:type):\\n${{SIGS[c.tool]}}\\n\\nTool description:\\n"${{description}}"\\n\\nTask: ${{c.prompt}}\\n\\nReturn the exact argument object you would call the tool with in \\`params\\` (only the params you need, correct names + values). One-line reasoning.`
}}

const full = await parallel(CASES.map((c) => () =>
  agent(brief(c, DFULL[c.tool]), {{ label: `full:${{c.id}}`, phase: 'Use', schema: SCHEMA, agentType: 'general-purpose' }})
    .then((r) => ({{ id: c.id, ...r }}))))

const trimmed = await parallel(CASES.map((c) => () =>
  agent(brief(c, DTRIM[c.tool]), {{ label: `trim:${{c.id}}`, phase: 'Use', schema: SCHEMA, agentType: 'general-purpose' }})
    .then((r) => ({{ id: c.id, ...r }}))))

return {{ full, trimmed }}
"""
open(os.path.join(HERE, "_wf_usage.js"), "w", encoding="utf-8").write(js)
print("wrote _wf_usage.js;", len(CASES), "cases x 2 conditions")
