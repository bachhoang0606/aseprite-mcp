local output = app.params["output"]
if not output or output == "" then
  error("Missing output path")
end

local spr = Sprite(32, 32, ColorMode.RGB)
spr.filename = output
spr:saveAs(output)
print("created " .. output)
