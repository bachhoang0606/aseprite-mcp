"""Pull the full tool-level #[tool(description=...)] text for the candidate tools."""
import json, re, os
src = open("src/server.rs", encoding="utf-8").read()
TARGETS = ["live_save_preview","live_import_reference","live_import_animation",
           "live_rotate","live_dither_fill","live_create_autotile_template"]
full = {}
# Anchor on `description = "..." )] async fn NAME` so inner ')' in the text don't truncate.
for m in re.finditer(r'description\s*=\s*"((?:[^"\\]|\\.)*)"\s*\)\]\s*async fn (\w+)', src, re.S):
    desc, name = m.group(1), m.group(2)
    if name in TARGETS:
        full[name] = re.sub(r"\s+", " ", desc).strip()
for n in TARGETS:
    print(n, "::", len(full.get(n,"")), "chars")
json.dump(full, open("evals/tool_usage/_full_desc.json", "w", encoding="utf-8"), indent=1, ensure_ascii=False)
