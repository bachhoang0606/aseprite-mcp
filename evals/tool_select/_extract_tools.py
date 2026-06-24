"""Extract registered tool names + description sizes from src/server.rs (one-off helper)."""
import json
import re

src = open("src/server.rs", encoding="utf-8").read()
tools = []
# #[tool( description = "..." ... )]  async fn NAME(
for m in re.finditer(r'#\[tool\((.*?)\)\]\s*async fn (\w+)\s*\(', src, re.S):
    attr, name = m.group(1), m.group(2)
    chunks = re.findall(r'"((?:[^"\\]|\\.)*)"', attr)  # all quoted string literals in the attr
    desc = "".join(chunks)
    tools.append({"name": name, "desc_chars": len(desc)})

tools.sort(key=lambda t: -t["desc_chars"])
print("count", len(tools))
print("total desc chars", sum(t["desc_chars"] for t in tools))
for t in tools[:6]:
    print(" big ", t["name"], t["desc_chars"])
for t in tools[-6:]:
    print(" small", t["name"], t["desc_chars"])
json.dump([t["name"] for t in sorted(tools, key=lambda t: t["name"])],
          open("evals/tool_select/_tool_names.json", "w"), indent=1)
json.dump(tools, open("evals/tool_select/_tools_raw.json", "w"), indent=1)
