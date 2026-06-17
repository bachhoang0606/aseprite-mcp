local output = app.params["output"]
if not output or output == "" then
  error("Missing output path")
end

local spr = app.sprite
if not spr then
  error("No sprite loaded")
end

spr:saveCopyAs(output)
print("exported " .. output)
