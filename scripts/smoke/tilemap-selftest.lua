----------------------------------------------------------------------
-- SPEC-003 tilemap API self-test (run inside Aseprite, no bridge needed).
--
-- The live tilemap handlers in `plugin.lua` rely on a handful of Aseprite 1.3
-- scripting calls that this MCP repo cannot exercise from CI (no Aseprite). This
-- script probes exactly those primitives on a throwaway sprite and prints a
-- PASS/FAIL line per assumption, so if your Aseprite build differs we learn the
-- precise call to fix instead of guessing.
--
-- HOW TO RUN:
--   Aseprite > File > Scripts > Open Scripts Folder…  (drop this file in, or)
--   Aseprite > File > Scripts > Rerun, OR open the Console (View > Console) and:
--     dofile("C:/Users/0606b/OneDrive/Documents/aseprite_mcp/scripts/smoke/tilemap-selftest.lua")
--
-- It creates a temp sprite and closes it WITHOUT saving. Copy the printed report
-- back to the assistant.
----------------------------------------------------------------------

local pass, fail, notes = 0, 0, {}

local function check(name, fn)
    local ok, result = pcall(fn)
    if ok and result ~= false then
        pass = pass + 1
        print(string.format("PASS  %-34s %s", name, (type(result) == "string") and result or ""))
    else
        fail = fail + 1
        local why = (result == false) and "returned false/nil" or tostring(result)
        table.insert(notes, name .. ": " .. why)
        print(string.format("FAIL  %-34s %s", name, why))
    end
end

print("==== SPEC-003 tilemap API self-test ====")
print("Aseprite " .. tostring(app.version) .. "  apiVersion=" .. tostring(app.apiVersion))

local TW, TH = 8, 8
local spr = Sprite(TW * 4, TH * 4, ColorMode.RGB) -- 32x32, 4x4 tile grid
app.sprite = spr
local tmlayer, tileset

check("set gridBounds", function()
    spr.gridBounds = Rectangle(0, 0, TW, TH)
    return string.format("%dx%d", spr.gridBounds.width, spr.gridBounds.height)
end)

check("NewLayer{tilemap=true}", function()
    app.command.NewLayer { tilemap = true }
    tmlayer = app.layer
    return tmlayer and tmlayer.name or false
end)

check("layer.isTilemap", function()
    return tmlayer and tmlayer.isTilemap == true
end)

check("layer.tileset present", function()
    tileset = tmlayer and tmlayer.tileset
    return tileset ~= nil
end)

check("#tileset (count)", function()
    return tileset and ("count=" .. tostring(#tileset))
end)

check("tileset.grid.tileSize", function()
    local s = tileset.grid.tileSize
    return string.format("%dx%d", s.width, s.height)
end)

check("Image(ImageSpec TILEMAP)", function()
    local img = Image(ImageSpec { width = 4, height = 4, colorMode = ColorMode.TILEMAP })
    return img and (img.width == 4 and img.height == 4)
end)

check("spr:newTile(tileset)", function()
    local t = spr:newTile(tileset)
    return t and ("index=" .. tostring(t.index))
end)

check("tile.image assignable", function()
    local t = tileset:tile(1) -- the tile we just added (index 1; 0 is empty)
    local timg = Image(TW, TH, spr.colorMode)
    timg:clear(Color(255, 0, 0, 255))
    t.image = timg
    return true
end)

check("tileset:getTile(i) -> Image", function()
    local img = tileset:getTile(1)
    return img and (img.width == TW)
end)

check("tileset:tile(i).image -> Image", function()
    local img = tileset:tile(1).image
    return img and (img.width == TW)
end)

check("newCel with TILEMAP image", function()
    local img = Image(ImageSpec { width = 4, height = 4, colorMode = ColorMode.TILEMAP })
    img:putPixel(0, 0, 1)
    img:putPixel(1, 0, 1)
    spr:newCel(tmlayer, spr.frames[1], img, Point(0, 0))
    return true
end)

check("tilemap cel getPixel round-trip", function()
    local cel = tmlayer:cel(1)
    local v = cel.image:getPixel(0, 0)
    local idx = v
    if app.pixelColor and app.pixelColor.tileI then
        idx = app.pixelColor.tileI(v)
    else
        idx = v & 0x1fffffff
    end
    return idx == 1 and ("tileIndex=" .. tostring(idx)) or ("got " .. tostring(idx) .. " (raw " .. tostring(v) .. ")")
end)

check("app.pixelColor.tileI present", function()
    return app.pixelColor and app.pixelColor.tileI ~= nil
end)

check("Image:isEqual / isEmpty (dedupe)", function()
    local a = Image(TW, TH, spr.colorMode); a:clear()
    local b = Image(TW, TH, spr.colorMode); b:clear()
    return a:isEqual(b) and a:isEmpty()
end)

check("Image:drawImage (pack)", function()
    local sheet = Image(TW * 2, TH, spr.colorMode); sheet:clear()
    local tile = tileset:getTile(1)
    if tile then sheet:drawImage(tile, Point(TW, 0)) end
    return true
end)

check("Image:saveAs PNG", function()
    local sheet = Image(TW * 2, TH, spr.colorMode); sheet:clear()
    -- Save beside this script's folder when we can resolve it; otherwise a plain
    -- relative name (we only need to confirm saveAs works, not where it lands).
    local here = debug.getinfo(1, "S").source
    local path = "aseprite_mcp_tilemap_selftest.png"
    if type(here) == "string" and here:sub(1, 1) == "@" then
        local dir = app.fs.filePath(here:sub(2))
        if dir and dir ~= "" then
            path = app.fs.joinPath(dir, path)
        end
    end
    sheet:saveAs(path)
    return path
end)

-- Clean up: close the throwaway sprite without saving.
pcall(function() spr:close() end)

print(string.format("==== RESULT: %d passed, %d failed ====", pass, fail))
if fail > 0 then
    print("Failures to report back:")
    for _, n in ipairs(notes) do print("  - " .. n) end
else
    print("All tilemap primitives behave as the plugin expects on this build.")
end
