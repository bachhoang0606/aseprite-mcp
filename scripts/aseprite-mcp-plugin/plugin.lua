----------------------------------------------------------------------
-- Aseprite MCP — live-edit WebSocket bridge plugin.
--
-- Connects a running Aseprite session to the MCP server and serves a
-- small, whitelisted live-edit command protocol over WebSocket.
----------------------------------------------------------------------

local PROTOCOL = "aseprite-live-edit"
local VERSION = 1
local PLUGIN_VERSION = "0.3.1"
-- Optional capability flags advertised via get_capabilities. The wire protocol
-- VERSION stays 1 (existing commands keep working across plugin builds); new
-- command families like tilemaps are gated by feature flags + the loud
-- "unsupported_command" reply older plugins give, not by a breaking version bump.
local FEATURES = { "tilemap", "color_ops" }

local CONFIG = {
    host = "127.0.0.1",
    port = 9876,
    reconnect_min = 1,
    reconnect_max = 30,
    reconnect_tick = 2,
    -- Max reconnect-timer ticks to wait for a connect attempt to reach OPEN
    -- before forcing a reset. 3 ticks * reconnect_tick(2s) ~= 6s. Guards
    -- against a `connecting` flag that gets stuck true when neither OPEN nor
    -- CLOSE ever fires (stalled handshake / server restarted mid-connect).
    connect_max_ticks = 3,
    -- Consecutive ping failures tolerated before declaring the socket dead.
    -- A connected session works in the background (aseprite#3009), but this
    -- Timer-driven bookkeeping itself may pause while the window is unfocused,
    -- so a single failed ping does NOT mean the peer is gone. Tolerating
    -- several misses (8 * reconnect_tick(2s) ~= 16s) avoids tearing down a
    -- healthy connection. The server has no idle timeout, so a quiet
    -- connection is safe; only a genuinely dead one should be closed.
    ping_max_misses = 8,
    debug = false,
}

local ws = nil
local connected = false
local connecting = false
local connecting_ticks = 0
local ping_misses = 0
local should_connect = false
local reconnect_timer = nil
local sprite_info = nil

local function log(msg)
    if CONFIG.debug then
        print("[MCP Live] " .. msg)
    end
end

local function mark_disconnected(reason)
    connected = false
    connecting = false
    connecting_ticks = 0
    ping_misses = 0
    -- Close the underlying socket before dropping the reference, otherwise the
    -- orphaned WebSocket keeps auto-reconnecting in the background and races the
    -- next freshly-created socket over the shared connection state.
    if ws then
        pcall(function() ws:close() end)
    end
    ws = nil
    if reason then
        log("Disconnected: " .. tostring(reason))
    end
end

local function safe_send_text(text)
    if not ws or not connected then
        mark_disconnected("send skipped because socket is not connected")
        return false
    end

    local ok, err = pcall(function()
        ws:sendText(text)
    end)
    if not ok then
        mark_disconnected(err)
        return false
    end
    return true
end

local function ok_response(id, result)
    return {
        protocol = PROTOCOL,
        version = VERSION,
        id = id,
        ok = true,
        result = result or {},
    }
end

local function error_response(id, code, message, details)
    return {
        protocol = PROTOCOL,
        version = VERSION,
        id = id,
        ok = false,
        error = {
            code = code,
            message = message,
            details = details,
        },
    }
end

local function parse_hex_color(value)
    if type(value) ~= "string" then
        return nil, "Color must be a string"
    end

    local hex = value:gsub("^#", "")
    if #hex ~= 6 and #hex ~= 8 then
        return nil, "Color must use #rrggbb or #rrggbbaa"
    end
    if not hex:match("^[0-9a-fA-F]+$") then
        return nil, "Color contains non-hex characters"
    end

    local r = tonumber(hex:sub(1, 2), 16)
    local g = tonumber(hex:sub(3, 4), 16)
    local b = tonumber(hex:sub(5, 6), 16)
    local a = 255
    if #hex == 8 then
        a = tonumber(hex:sub(7, 8), 16)
    end
    return { r = r, g = g, b = b, a = a }, nil
end

local function color_to_pixel(color)
    return app.pixelColor.rgba(color.r, color.g, color.b, color.a)
end

local function safe_get(obj, key)
    local ok, value = pcall(function()
        return obj[key]
    end)
    if ok then
        return value
    end
    return nil
end

local function safe_set(obj, key, value)
    local ok, err = pcall(function()
        obj[key] = value
    end)
    return ok, err
end

local function find_layer(layers, name)
    for _, layer in ipairs(layers) do
        if layer.name == name then
            return layer
        end
        if safe_get(layer, "isGroup") then
            local nested = find_layer(safe_get(layer, "layers") or {}, name)
            if nested then
                return nested
            end
        end
    end
    return nil
end

local function layer_info(layer)
    local blend_mode = safe_get(layer, "blendMode")
    local cels = safe_get(layer, "cels")
    local parent = safe_get(layer, "parent")
    local info = {
        name = safe_get(layer, "name"),
        isVisible = safe_get(layer, "isVisible"),
        isEditable = safe_get(layer, "isEditable"),
        isGroup = safe_get(layer, "isGroup"),
        isImage = safe_get(layer, "isImage"),
        isTilemap = safe_get(layer, "isTilemap"),
        isBackground = safe_get(layer, "isBackground"),
        isTransparent = safe_get(layer, "isTransparent"),
        isContinuous = safe_get(layer, "isContinuous"),
        isCollapsed = safe_get(layer, "isCollapsed"),
        isExpanded = safe_get(layer, "isExpanded"),
        isReference = safe_get(layer, "isReference"),
        stackIndex = safe_get(layer, "stackIndex"),
        opacity = safe_get(layer, "opacity"),
        blendMode = blend_mode and tostring(blend_mode) or nil,
        cels = cels and #cels or 0,
    }
    if parent and safe_get(parent, "name") then
        info.parent = safe_get(parent, "name")
    end
    return info
end

local function collect_layer_infos(layers)
    local result = {}
    for _, layer in ipairs(layers) do
        local info = layer_info(layer)
        if safe_get(layer, "isGroup") then
            info.layers = collect_layer_infos(safe_get(layer, "layers") or {})
        end
        table.insert(result, info)
    end
    return result
end

local function require_layer(spr, name, id)
    if not name or name == "" then
        return nil, error_response(id, "missing_layer", "Layer name is required")
    end

    local layer = find_layer(spr.layers, name)
    if not layer then
        return nil, error_response(id, "layer_not_found", "Layer was not found", { layer = name })
    end
    return layer, nil
end

local function ensure_sprite()
    local spr = app.sprite
    if not spr then
        return nil, error_response(nil, "no_sprite", "No sprite is open in Aseprite")
    end
    return spr, nil
end

local function handle_list_sprites(cmd)
    local sprites = {}
    for index, spr in ipairs(app.sprites) do
        table.insert(sprites, {
            index = index,
            filename = spr.filename,
            width = spr.width,
            height = spr.height,
            frames = #spr.frames,
            layers = #spr.layers,
            isModified = spr.isModified,
            isActive = spr == app.sprite,
        })
    end
    return ok_response(cmd.id, { sprites = sprites })
end

local function handle_new_sprite(cmd)
    local payload = cmd.payload or {}
    local width = payload.width or 24
    local height = payload.height or 24
    local filename = payload.filename

    if type(width) ~= "number" or width < 1 or type(height) ~= "number" or height < 1 then
        return error_response(cmd.id, "invalid_size", "Sprite width and height must be positive numbers")
    end

    local spr = Sprite(width, height, ColorMode.RGB)
    if filename and filename ~= "" then
        spr.filename = filename
    end
    app.sprite = spr

    return ok_response(cmd.id, {
        filename = spr.filename,
        width = spr.width,
        height = spr.height,
        frames = #spr.frames,
    })
end

local function handle_open_sprite(cmd)
    local payload = cmd.payload or {}
    local filename = payload.filename
    if not filename or filename == "" then
        return error_response(cmd.id, "missing_filename", "filename is required")
    end

    local spr = app.open(filename)
    if not spr then
        return error_response(cmd.id, "open_failed", "Aseprite could not open the file", { filename = filename })
    end
    app.sprite = spr
    return ok_response(cmd.id, sprite_info(spr))
end

local function handle_activate_sprite(cmd)
    local payload = cmd.payload or {}
    local filename = payload.filename
    local index = payload.index

    if index then
        if type(index) ~= "number" or index < 1 or index > #app.sprites then
            return error_response(cmd.id, "invalid_index", "index must refer to an open sprite")
        end
        local spr = app.sprites[index]
        app.sprite = spr
        return ok_response(cmd.id, sprite_info(spr))
    end

    if not filename or filename == "" then
        return error_response(cmd.id, "missing_sprite_selector", "filename or index is required")
    end

    for _, spr in ipairs(app.sprites) do
        if spr.filename == filename then
            app.sprite = spr
            return ok_response(cmd.id, sprite_info(spr))
        end
    end

    return error_response(cmd.id, "sprite_not_found", "No open sprite matches filename", { filename = filename })
end

local function ensure_layer(spr, name)
    local layer = find_layer(spr.layers, name)
    if layer then
        return layer
    end

    layer = spr:newLayer()
    layer.name = name
    return layer
end

local function resolve_frame(spr, frame_value)
    if frame_value == nil or frame_value == "active" then
        return app.frame or spr.frames[1], nil
    end
    if type(frame_value) ~= "number" then
        return nil, "Frame must be a 1-based number or 'active'"
    end
    if frame_value < 1 or frame_value > #spr.frames then
        return nil, "Frame out of range"
    end
    return spr.frames[frame_value], nil
end

local function get_target(cmd)
    local target = cmd.target or {}
    return {
        layer = target.layer or "AI Draft",
        frame = target.frame or "active",
    }
end

local function get_or_create_cel(spr, layer, frame)
    local cel = layer:cel(frame)
    if cel then
        return cel
    end

    local image = Image(spr.width, spr.height, spr.colorMode)
    return spr:newCel(layer, frame, image, Point(0, 0))
end

local function frame_info(frame)
    if not frame then
        return nil
    end
    return {
        frameNumber = safe_get(frame, "frameNumber"),
        duration = safe_get(frame, "duration"),
    }
end

local function cel_info(cel)
    if not cel then
        return nil
    end
    local position = safe_get(cel, "position")
    local image = safe_get(cel, "image")
    return {
        layer = safe_get(safe_get(cel, "layer") or {}, "name"),
        frame = safe_get(cel, "frameNumber"),
        x = position and position.x or nil,
        y = position and position.y or nil,
        opacity = safe_get(cel, "opacity"),
        zIndex = safe_get(cel, "zIndex"),
        data = safe_get(cel, "data"),
        image = image and {
            width = safe_get(image, "width"),
            height = safe_get(image, "height"),
            colorMode = tostring(safe_get(image, "colorMode")),
        } or nil,
    }
end

local function rectangle_info(rect)
    if not rect then
        return nil
    end
    return {
        x = safe_get(rect, "x"),
        y = safe_get(rect, "y"),
        width = safe_get(rect, "width"),
        height = safe_get(rect, "height"),
    }
end

local function point_info(point)
    if not point then
        return nil
    end
    return {
        x = safe_get(point, "x"),
        y = safe_get(point, "y"),
    }
end

local function size_info(size)
    if not size then
        return nil
    end
    return {
        width = safe_get(size, "width"),
        height = safe_get(size, "height"),
    }
end

local function color_info(color)
    if not color then
        return nil
    end
    return {
        red = safe_get(color, "red"),
        green = safe_get(color, "green"),
        blue = safe_get(color, "blue"),
        alpha = safe_get(color, "alpha"),
    }
end

local function tag_info(tag)
    if not tag then
        return nil
    end
    local from_frame = safe_get(tag, "fromFrame")
    local to_frame = safe_get(tag, "toFrame")
    return {
        name = safe_get(tag, "name"),
        fromFrame = from_frame and safe_get(from_frame, "frameNumber") or nil,
        toFrame = to_frame and safe_get(to_frame, "frameNumber") or nil,
        frames = safe_get(tag, "frames"),
        aniDir = tostring(safe_get(tag, "aniDir")),
        repeats = safe_get(tag, "repeats"),
        color = color_info(safe_get(tag, "color")),
        data = safe_get(tag, "data"),
    }
end

local function slice_info(slice)
    if not slice then
        return nil
    end
    return {
        name = safe_get(slice, "name"),
        bounds = rectangle_info(safe_get(slice, "bounds")),
        center = rectangle_info(safe_get(slice, "center")),
        pivot = point_info(safe_get(slice, "pivot")),
        color = color_info(safe_get(slice, "color")),
        data = safe_get(slice, "data"),
    }
end

local function selection_info(selection)
    if not selection then
        return nil
    end
    return {
        bounds = rectangle_info(safe_get(selection, "bounds")),
        origin = point_info(safe_get(selection, "origin")),
        isEmpty = safe_get(selection, "isEmpty"),
    }
end

local function palette_color_info(color)
    if not color then
        return nil
    end
    return color_info(color)
end

local function find_tag(spr, name)
    for _, tag in ipairs(safe_get(spr, "tags") or {}) do
        if safe_get(tag, "name") == name then
            return tag
        end
    end
    return nil
end

local function find_slice(spr, name)
    for _, slice in ipairs(safe_get(spr, "slices") or {}) do
        if safe_get(slice, "name") == name then
            return slice
        end
    end
    return nil
end

local function rectangle_from_payload(payload, prefix)
    prefix = prefix or ""
    local x = payload[prefix .. "x"]
    local y = payload[prefix .. "y"]
    local width = payload[prefix .. "width"]
    local height = payload[prefix .. "height"]
    if x == nil and y == nil and width == nil and height == nil then
        return nil, nil
    end
    if type(x) ~= "number" or type(y) ~= "number" or type(width) ~= "number" or type(height) ~= "number" or width < 0 or height < 0 then
        return nil, "Rectangle requires numeric x, y, width, and height"
    end
    return Rectangle(x, y, width, height), nil
end

local function point_from_payload(payload, prefix)
    prefix = prefix or ""
    local x = payload[prefix .. "x"]
    local y = payload[prefix .. "y"]
    if x == nil and y == nil then
        return nil, nil
    end
    if type(x) ~= "number" or type(y) ~= "number" then
        return nil, "Point requires numeric x and y"
    end
    return Point(x, y), nil
end

sprite_info = function(spr)
    local layers = {}
    for _, layer in ipairs(spr.layers) do
        table.insert(layers, {
            name = layer.name,
            isVisible = layer.isVisible,
            isEditable = layer.isEditable,
            opacity = layer.opacity,
        })
    end

    return {
        filename = spr.filename,
        width = spr.width,
        height = spr.height,
        colorMode = tostring(spr.colorMode),
        frames = #spr.frames,
        layers = layers,
        cels = #spr.cels,
        isModified = spr.isModified,
        activeLayer = app.layer and app.layer.name or nil,
        data = safe_get(spr, "data"),
        color = color_info(safe_get(spr, "color")),
        gridBounds = rectangle_info(safe_get(spr, "gridBounds")),
        pixelRatio = size_info(safe_get(spr, "pixelRatio")),
        transparentColor = safe_get(spr, "transparentColor"),
    }
end

local function handle_get_active_site(cmd)
    local spr = app.sprite
    local site = app.site
    return ok_response(cmd.id, {
        hasSprite = spr ~= nil,
        sprite = spr and sprite_info(spr) or nil,
        layer = app.layer and layer_info(app.layer) or nil,
        frame = app.frame and app.frame.frameNumber or nil,
        cel = app.cel and {
            layer = app.cel.layer.name,
            frame = app.cel.frameNumber,
            x = app.cel.position.x,
            y = app.cel.position.y,
            opacity = app.cel.opacity,
        } or nil,
        site = site and {
            sprite = site.sprite and site.sprite.filename or nil,
            layer = site.layer and site.layer.name or nil,
            frame = site.frame and site.frame.frameNumber or nil,
        } or nil,
    })
end

local function handle_get_sprite_info(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end
    return ok_response(cmd.id, sprite_info(spr))
end

local function handle_set_sprite_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    if payload.data ~= nil then
        spr.data = tostring(payload.data)
    end
    if payload.transparentColor ~= nil then
        if type(payload.transparentColor) ~= "number" or payload.transparentColor < 0 then
            return error_response(cmd.id, "invalid_transparent_color", "transparentColor must be a non-negative palette index")
        end
        spr.transparentColor = math.floor(payload.transparentColor)
    end
    if payload.color ~= nil then
        local color, color_err = parse_hex_color(payload.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
        spr.color = Color(color.r, color.g, color.b, color.a)
    end
    if payload.gridBounds ~= nil then
        local grid = payload.gridBounds
        local x = safe_get(grid, "x")
        local y = safe_get(grid, "y")
        local width = safe_get(grid, "width")
        local height = safe_get(grid, "height")
        if type(x) ~= "number" or type(y) ~= "number" or type(width) ~= "number" or type(height) ~= "number" then
            return error_response(cmd.id, "invalid_grid_bounds", "gridBounds requires numeric x, y, width, and height")
        end
        spr.gridBounds = Rectangle(x, y, width, height)
    end
    if payload.pixelRatio ~= nil then
        local ratio = payload.pixelRatio
        local width = safe_get(ratio, "width")
        local height = safe_get(ratio, "height")
        if type(width) ~= "number" or type(height) ~= "number" or width < 1 or height < 1 then
            return error_response(cmd.id, "invalid_pixel_ratio", "pixelRatio requires positive numeric width and height")
        end
        spr.pixelRatio = Size(width, height)
    end

    app.refresh()
    return ok_response(cmd.id, sprite_info(spr))
end

local function handle_save_sprite(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    if not spr.filename or spr.filename == "" then
        return error_response(cmd.id, "missing_filename", "Active sprite does not have a filename")
    end

    app.command.SaveFile()
    return ok_response(cmd.id, {
        changed = true,
        filename = spr.filename,
    })
end

local function handle_save_sprite_as(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local filename = payload.filename
    if not filename or filename == "" then
        return error_response(cmd.id, "missing_filename", "filename is required")
    end

    app.command.SaveFileAs {
        ui = false,
        filename = filename,
    }
    return ok_response(cmd.id, sprite_info(spr))
end

local function handle_save_copy_as(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local filename = payload.filename
    if not filename or filename == "" then
        return error_response(cmd.id, "missing_filename", "filename is required")
    end

    spr:saveCopyAs(filename)
    return ok_response(cmd.id, {
        changed = true,
        filename = filename,
        activeFilename = spr.filename,
    })
end

-- Modal-free preview save (ADR-0004). Renders the ACTIVE frame into a standalone
-- single-frame Image and saves THAT, instead of spr:saveCopyAs on the whole sprite.
-- On a multi-frame sprite, saving the sprite to .png pops Aseprite's "format does
-- not support multiple frames" modal (gated on isUIAvailable(), NOT on ui=false, so
-- saveCopyAs can't avoid it) which blocks the UI thread. Image:saveAs wraps the
-- image in a temporary one-frame sprite, so that branch can never be entered.
local function handle_save_preview(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local filename = payload.filename
    if not filename or filename == "" then
        return error_response(cmd.id, "missing_filename", "filename is required")
    end

    local frame = app.frame or spr.frames[1]
    local frame_number = frame.frameNumber
    -- RGB destination = lossless true-colour composite of all visible layers at the
    -- active frame, regardless of the sprite's colour mode (indexed/gray included).
    local img = Image(spr.width, spr.height, ColorMode.RGB)
    img:drawSprite(spr, frame_number)
    img:saveAs(filename)
    return ok_response(cmd.id, {
        changed = true,
        filename = filename,
        frame = frame_number,
        width = img.width,
        height = img.height,
    })
end

local function handle_close_sprite(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    if spr.isModified and not payload.force then
        return error_response(cmd.id, "sprite_modified", "Active sprite is modified; pass force=true to close without saving")
    end

    local filename = spr.filename
    spr:close()
    return ok_response(cmd.id, {
        changed = true,
        filename = filename,
    })
end

local function handle_resize_canvas(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local width = payload.width
    local height = payload.height

    if type(width) ~= "number" or width < 1 or width ~= math.floor(width) then
        return error_response(cmd.id, "invalid_width", "width must be a positive integer")
    end
    if type(height) ~= "number" or height < 1 or height ~= math.floor(height) then
        return error_response(cmd.id, "invalid_height", "height must be a positive integer")
    end

    local old_width = spr.width
    local old_height = spr.height

    if old_width == width and old_height == height then
        local info = sprite_info(spr)
        info.changed = false
        info.oldWidth = old_width
        info.oldHeight = old_height
        return ok_response(cmd.id, info)
    end

    app.command.CanvasSize {
        ui = false,
        bounds = Rectangle(0, 0, width, height),
        trimOutside = false,
    }

    app.refresh()
    local info = sprite_info(spr)
    info.changed = old_width ~= spr.width or old_height ~= spr.height
    info.oldWidth = old_width
    info.oldHeight = old_height
    return ok_response(cmd.id, info)
end

local function handle_ensure_layer(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name or "AI Draft"
    local layer = ensure_layer(spr, name)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = layer.name,
    })
end

local function handle_list_layers(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end
    return ok_response(cmd.id, { layers = collect_layer_infos(spr.layers) })
end

local function handle_set_layer_visibility(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_layer", "Layer name is required")
    end

    local layer = find_layer(spr.layers, name)
    if not layer then
        return error_response(cmd.id, "layer_not_found", "Layer was not found", { layer = name })
    end

    local ok, set_err = safe_set(layer, "isVisible", payload.visible ~= false)
    if not ok then
        return error_response(cmd.id, "unsupported_layer_property", "Layer visibility could not be changed", { property = "isVisible", reason = tostring(set_err) })
    end
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = safe_get(layer, "name"),
        visible = safe_get(layer, "isVisible"),
    })
end

local function handle_set_active_layer(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local layer, layer_err = require_layer(spr, payload.name, cmd.id)
    if layer_err then
        return layer_err
    end

    app.layer = layer
    return ok_response(cmd.id, { layer = layer_info(layer) })
end

local function handle_rename_layer(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local layer, layer_err = require_layer(spr, payload.name, cmd.id)
    if layer_err then
        return layer_err
    end
    if not payload.newName or payload.newName == "" then
        return error_response(cmd.id, "missing_new_name", "newName is required")
    end

    local old_name = layer.name
    layer.name = payload.newName
    app.refresh()
    return ok_response(cmd.id, {
        changed = old_name ~= layer.name,
        oldName = old_name,
        layer = layer_info(layer),
    })
end

local function handle_create_group_layer(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name or "Group"
    local group = spr:newGroup()
    group.name = name
    if payload.parent then
        local parent, parent_err = require_layer(spr, payload.parent, cmd.id)
        if parent_err then
            return parent_err
        end
        group.parent = parent
    end
    app.refresh()
    return ok_response(cmd.id, { layer = layer_info(group) })
end

local function handle_set_layer_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local layer, layer_err = require_layer(spr, payload.name, cmd.id)
    if layer_err then
        return layer_err
    end

    if payload.visible ~= nil then
        local ok, set_err = safe_set(layer, "isVisible", payload.visible == true)
        if not ok then
            return error_response(cmd.id, "unsupported_layer_property", "Layer visibility could not be changed", { property = "isVisible", reason = tostring(set_err) })
        end
    end
    if payload.editable ~= nil then
        local ok, set_err = safe_set(layer, "isEditable", payload.editable == true)
        if not ok then
            return error_response(cmd.id, "unsupported_layer_property", "Layer editable state could not be changed", { property = "isEditable", reason = tostring(set_err) })
        end
    end
    if payload.opacity ~= nil and safe_get(layer, "opacity") ~= nil then
        if type(payload.opacity) ~= "number" or payload.opacity < 0 or payload.opacity > 255 then
            return error_response(cmd.id, "invalid_opacity", "opacity must be between 0 and 255")
        end
        local ok, set_err = safe_set(layer, "opacity", math.floor(payload.opacity))
        if not ok then
            return error_response(cmd.id, "unsupported_layer_property", "Layer opacity could not be changed", { property = "opacity", reason = tostring(set_err) })
        end
    end
    if payload.blendMode ~= nil and safe_get(layer, "blendMode") ~= nil then
        local ok, set_err = safe_set(layer, "blendMode", payload.blendMode)
        if not ok then
            return error_response(cmd.id, "unsupported_layer_property", "Layer blend mode could not be changed", { property = "blendMode", reason = tostring(set_err) })
        end
    end
    if payload.stackIndex ~= nil then
        if type(payload.stackIndex) ~= "number" or payload.stackIndex < 1 then
            return error_response(cmd.id, "invalid_stack_index", "stackIndex must be a positive integer")
        end
        local ok, set_err = safe_set(layer, "stackIndex", math.floor(payload.stackIndex))
        if not ok then
            return error_response(cmd.id, "unsupported_layer_property", "Layer stack index could not be changed", { property = "stackIndex", reason = tostring(set_err) })
        end
    end
    if payload.parent ~= nil then
        if payload.parent == "" then
            layer.parent = spr
        else
            local parent, parent_err = require_layer(spr, payload.parent, cmd.id)
            if parent_err then
                return parent_err
            end
            layer.parent = parent
        end
    end

    app.refresh()
    return ok_response(cmd.id, { layer = layer_info(layer) })
end

local function handle_delete_layer(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_layer", "Layer name is required")
    end

    local layer = find_layer(spr.layers, name)
    if not layer then
        return ok_response(cmd.id, {
            changed = false,
            layer = name,
        })
    end

    spr:deleteLayer(layer)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = name,
    })
end

local function handle_ensure_frames(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local count = payload.count or 1
    local duration = payload.duration or 0.12
    if type(count) ~= "number" or count < 1 then
        return error_response(cmd.id, "invalid_frame_count", "Frame count must be greater than zero")
    end

    app.transaction("MCP Live Ensure Frames", function()
        while #spr.frames < count do
            spr:newEmptyFrame(#spr.frames + 1)
        end
        for i = 1, count do
            spr.frames[i].duration = duration
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        frames = #spr.frames,
        duration = duration,
    })
end

local function handle_list_frames(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local frames = {}
    for _, frame in ipairs(spr.frames) do
        table.insert(frames, frame_info(frame))
    end
    return ok_response(cmd.id, { frames = frames })
end

local function handle_set_active_frame(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    app.frame = frame
    return ok_response(cmd.id, { frame = frame_info(frame) })
end

local function handle_set_frame_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    if payload.duration ~= nil then
        if type(payload.duration) ~= "number" or payload.duration < 0 then
            return error_response(cmd.id, "invalid_duration", "duration must be a non-negative number")
        end
        local ok, set_err = safe_set(frame, "duration", payload.duration)
        if not ok then
            return error_response(cmd.id, "unsupported_frame_property", "Frame duration could not be changed", { property = "duration", reason = tostring(set_err) })
        end
    end

    app.refresh()
    return ok_response(cmd.id, { frame = frame_info(frame) })
end

local function handle_new_empty_frame(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local index = payload.index or (#spr.frames + 1)
    if type(index) ~= "number" or index < 1 or index > #spr.frames + 1 then
        return error_response(cmd.id, "invalid_frame", "index must be between 1 and frameCount + 1")
    end

    local new_frame
    app.transaction("MCP Live New Empty Frame", function()
        new_frame = spr:newEmptyFrame(math.floor(index))
        if payload.duration ~= nil then
            new_frame.duration = payload.duration
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        frame = frame_info(new_frame),
        frames = #spr.frames,
    })
end

local function handle_new_frame(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local source_frame, frame_err = resolve_frame(spr, payload.sourceFrame or payload.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local new_frame
    app.transaction("MCP Live New Frame", function()
        new_frame = spr:newFrame(source_frame)
        if payload.duration ~= nil then
            new_frame.duration = payload.duration
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        frame = frame_info(new_frame),
        frames = #spr.frames,
    })
end

local function handle_delete_frame(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end
    if #spr.frames <= 1 then
        return error_response(cmd.id, "cannot_delete_last_frame", "Cannot delete the last remaining frame")
    end

    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end
    local frame_number = frame.frameNumber

    app.transaction("MCP Live Delete Frame", function()
        spr:deleteFrame(frame)
    end)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        frame = frame_number,
        frames = #spr.frames,
    })
end

local function handle_list_cels(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local layer_filter = payload.layer
    local frame_filter = payload.frame
    local cels = {}

    for _, cel in ipairs(spr.cels) do
        local info = cel_info(cel)
        local matches_layer = not layer_filter or info.layer == layer_filter
        local matches_frame = not frame_filter or info.frame == frame_filter
        if matches_layer and matches_frame then
            table.insert(cels, info)
        end
    end
    return ok_response(cmd.id, { cels = cels })
end

local function handle_new_cel(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local target = get_target(cmd)
    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame or target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end
    local layer, layer_err = require_layer(spr, payload.layer or target.layer, cmd.id)
    if layer_err then
        return layer_err
    end

    local existing = layer:cel(frame)
    if existing and not payload.replace then
        return ok_response(cmd.id, {
            changed = false,
            cel = cel_info(existing),
        })
    end

    local cel
    app.transaction("MCP Live New Cel", function()
        if existing then
            spr:deleteCel(existing)
        end
        local image = Image(spr.width, spr.height, spr.colorMode)
        cel = spr:newCel(layer, frame, image, Point(payload.x or 0, payload.y or 0))
        if payload.opacity ~= nil then
            cel.opacity = payload.opacity
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        cel = cel_info(cel),
    })
end

local function handle_set_cel_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local target = get_target(cmd)
    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame or target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end
    local layer, layer_err = require_layer(spr, payload.layer or target.layer, cmd.id)
    if layer_err then
        return layer_err
    end

    local cel = layer:cel(frame)
    if not cel then
        return error_response(cmd.id, "cel_not_found", "No cel exists at the requested layer/frame")
    end

    if payload.opacity ~= nil then
        if type(payload.opacity) ~= "number" or payload.opacity < 0 or payload.opacity > 255 then
            return error_response(cmd.id, "invalid_opacity", "opacity must be between 0 and 255")
        end
        local ok, set_err = safe_set(cel, "opacity", math.floor(payload.opacity))
        if not ok then
            return error_response(cmd.id, "unsupported_cel_property", "Cel opacity could not be changed", { property = "opacity", reason = tostring(set_err) })
        end
    end
    if payload.x ~= nil or payload.y ~= nil then
        local position = safe_get(cel, "position") or Point(0, 0)
        cel.position = Point(payload.x or position.x, payload.y or position.y)
    end
    if payload.zIndex ~= nil then
        local ok, set_err = safe_set(cel, "zIndex", payload.zIndex)
        if not ok then
            return error_response(cmd.id, "unsupported_cel_property", "Cel zIndex could not be changed", { property = "zIndex", reason = tostring(set_err) })
        end
    end
    if payload.data ~= nil then
        local ok, set_err = safe_set(cel, "data", tostring(payload.data))
        if not ok then
            return error_response(cmd.id, "unsupported_cel_property", "Cel data could not be changed", { property = "data", reason = tostring(set_err) })
        end
    end

    app.refresh()
    return ok_response(cmd.id, { cel = cel_info(cel) })
end

local function handle_delete_cel(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local target = get_target(cmd)
    local payload = cmd.payload or {}
    local frame, frame_err = resolve_frame(spr, payload.frame or target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end
    local layer, layer_err = require_layer(spr, payload.layer or target.layer, cmd.id)
    if layer_err then
        return layer_err
    end

    local cel = layer:cel(frame)
    if not cel then
        return ok_response(cmd.id, {
            changed = false,
            layer = layer.name,
            frame = frame.frameNumber,
        })
    end

    app.transaction("MCP Live Delete Cel", function()
        spr:deleteCel(cel)
    end)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = layer.name,
        frame = frame.frameNumber,
    })
end

local function handle_list_tags(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local tags = {}
    for _, tag in ipairs(spr.tags) do
        table.insert(tags, tag_info(tag))
    end
    return ok_response(cmd.id, { tags = tags })
end

local function handle_new_tag(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_tag_name", "Tag name is required")
    end
    if find_tag(spr, name) then
        return ok_response(cmd.id, {
            changed = false,
            tag = tag_info(find_tag(spr, name)),
        })
    end

    local from_frame = payload.fromFrame or 1
    local to_frame = payload.toFrame or from_frame
    if type(from_frame) ~= "number" or type(to_frame) ~= "number" or from_frame < 1 or to_frame < from_frame or to_frame > #spr.frames then
        return error_response(cmd.id, "invalid_tag_range", "Tag range must be within the sprite frame count")
    end
    local tag_color = nil
    if payload.color then
        local color, color_err = parse_hex_color(payload.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
        tag_color = color
    end

    local tag
    app.transaction("MCP Live New Tag", function()
        tag = spr:newTag(math.floor(from_frame), math.floor(to_frame))
        tag.name = name
        if payload.repeats ~= nil then tag.repeats = payload.repeats end
        if payload.data ~= nil then tag.data = tostring(payload.data) end
        if tag_color then
            tag.color = Color(tag_color.r, tag_color.g, tag_color.b, tag_color.a)
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        tag = tag_info(tag),
    })
end

local function handle_set_tag_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local tag = find_tag(spr, payload.name)
    if not tag then
        return error_response(cmd.id, "tag_not_found", "Tag was not found", { tag = payload.name })
    end

    if payload.newName ~= nil then
        if payload.newName == "" then
            return error_response(cmd.id, "missing_new_name", "newName cannot be empty")
        end
        tag.name = payload.newName
    end
    if payload.repeats ~= nil then
        tag.repeats = payload.repeats
    end
    if payload.data ~= nil then
        tag.data = tostring(payload.data)
    end
    if payload.color ~= nil then
        local color, color_err = parse_hex_color(payload.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
        tag.color = Color(color.r, color.g, color.b, color.a)
    end

    app.refresh()
    return ok_response(cmd.id, { tag = tag_info(tag) })
end

local function handle_delete_tag(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_tag_name", "Tag name is required")
    end
    local tag = find_tag(spr, name)
    if not tag then
        return ok_response(cmd.id, {
            changed = false,
            tag = name,
        })
    end

    app.transaction("MCP Live Delete Tag", function()
        spr:deleteTag(tag)
    end)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        tag = name,
    })
end

local function handle_list_slices(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local slices = {}
    for _, slice in ipairs(spr.slices) do
        table.insert(slices, slice_info(slice))
    end
    return ok_response(cmd.id, { slices = slices })
end

local function handle_new_slice(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_slice_name", "Slice name is required")
    end
    local existing = find_slice(spr, name)
    if existing and not payload.replace then
        return ok_response(cmd.id, {
            changed = false,
            slice = slice_info(existing),
        })
    end

    local bounds, bounds_err = rectangle_from_payload(payload, "")
    if bounds_err then
        return error_response(cmd.id, "invalid_bounds", bounds_err)
    end
    bounds = bounds or Rectangle(0, 0, spr.width, spr.height)
    local slice_color = nil
    if payload.color ~= nil then
        local color, color_err = parse_hex_color(payload.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
        slice_color = color
    end

    local slice
    app.transaction("MCP Live New Slice", function()
        if existing then
            spr:deleteSlice(existing)
        end
        slice = spr:newSlice(bounds)
        slice.name = name
        if payload.data ~= nil then slice.data = tostring(payload.data) end
        if slice_color then
            slice.color = Color(slice_color.r, slice_color.g, slice_color.b, slice_color.a)
        end
        local center = nil
        if payload.center then
            center = Rectangle(payload.center.x or 0, payload.center.y or 0, payload.center.width or 0, payload.center.height or 0)
        end
        if center then slice.center = center end
        if payload.pivot then
            slice.pivot = Point(payload.pivot.x or 0, payload.pivot.y or 0)
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        slice = slice_info(slice),
    })
end

local function handle_set_slice_properties(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local slice = find_slice(spr, payload.name)
    if not slice then
        return error_response(cmd.id, "slice_not_found", "Slice was not found", { slice = payload.name })
    end

    if payload.newName ~= nil then
        if payload.newName == "" then
            return error_response(cmd.id, "missing_new_name", "newName cannot be empty")
        end
        slice.name = payload.newName
    end
    local bounds, bounds_err = rectangle_from_payload(payload, "")
    if bounds_err then
        return error_response(cmd.id, "invalid_bounds", bounds_err)
    end
    if bounds then
        slice.bounds = bounds
    end
    if payload.center ~= nil then
        if payload.center == false then
            slice.center = nil
        else
            slice.center = Rectangle(payload.center.x or 0, payload.center.y or 0, payload.center.width or 0, payload.center.height or 0)
        end
    end
    if payload.pivot ~= nil then
        if payload.pivot == false then
            slice.pivot = nil
        else
            slice.pivot = Point(payload.pivot.x or 0, payload.pivot.y or 0)
        end
    end
    if payload.data ~= nil then
        slice.data = tostring(payload.data)
    end
    if payload.color ~= nil then
        local color, color_err = parse_hex_color(payload.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
        slice.color = Color(color.r, color.g, color.b, color.a)
    end

    app.refresh()
    return ok_response(cmd.id, { slice = slice_info(slice) })
end

local function handle_delete_slice(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_slice_name", "Slice name is required")
    end
    local slice = find_slice(spr, name)
    if not slice then
        return ok_response(cmd.id, {
            changed = false,
            slice = name,
        })
    end

    app.transaction("MCP Live Delete Slice", function()
        spr:deleteSlice(slice)
    end)
    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        slice = name,
    })
end

local function handle_get_selection(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end
    return ok_response(cmd.id, { selection = selection_info(spr.selection) })
end

local function handle_set_selection(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local mode = payload.mode or "replace"
    local selection = spr.selection

    if mode == "deselect" then
        selection:deselect()
    elseif mode == "select_all" then
        selection:selectAll()
    else
        local rect, rect_err = rectangle_from_payload(payload, "")
        if rect_err then
            return error_response(cmd.id, "invalid_selection_bounds", rect_err)
        end
        if not rect then
            return error_response(cmd.id, "missing_selection_bounds", "Selection rectangle is required for this mode")
        end

        if mode == "replace" then
            selection:select(rect)
        elseif mode == "add" then
            selection:add(rect)
        elseif mode == "subtract" then
            selection:subtract(rect)
        elseif mode == "intersect" then
            selection:intersect(rect)
        else
            return error_response(cmd.id, "invalid_selection_mode", "mode must be replace, add, subtract, intersect, select_all, or deselect")
        end
    end

    app.refresh()
    return ok_response(cmd.id, { selection = selection_info(selection) })
end

local function handle_list_palette(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local palette_index = payload.palette or 1
    local palette = spr.palettes[palette_index]
    if not palette then
        return error_response(cmd.id, "palette_not_found", "Palette was not found", { palette = palette_index })
    end

    local count = #palette
    local from_index = payload.from or 0
    local limit = payload.limit or math.min(count, 32)
    if type(from_index) ~= "number" or from_index < 0 or from_index >= count then
        return error_response(cmd.id, "invalid_palette_index", "from must be a valid 0-based palette index")
    end
    if type(limit) ~= "number" or limit < 1 then
        return error_response(cmd.id, "invalid_limit", "limit must be greater than zero")
    end

    local colors = {}
    local last_index = math.min(count - 1, from_index + limit - 1)
    for index = from_index, last_index do
        table.insert(colors, {
            index = index,
            color = palette_color_info(palette:getColor(index)),
        })
    end
    return ok_response(cmd.id, {
        palette = palette_index,
        count = count,
        colors = colors,
    })
end

local function handle_set_palette_color(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local palette_index = payload.palette or 1
    local palette = spr.palettes[palette_index]
    if not palette then
        return error_response(cmd.id, "palette_not_found", "Palette was not found", { palette = palette_index })
    end
    local index = payload.index
    if type(index) ~= "number" or index < 0 or index >= #palette then
        return error_response(cmd.id, "invalid_palette_index", "index must be a valid 0-based palette index")
    end
    local color, color_err = parse_hex_color(payload.color)
    if color_err then
        return error_response(cmd.id, "invalid_color", color_err)
    end

    palette:setColor(index, Color(color.r, color.g, color.b, color.a))
    spr:setPalette(palette)
    app.refresh()
    return ok_response(cmd.id, {
        palette = palette_index,
        index = index,
        color = palette_color_info(palette:getColor(index)),
    })
end

local function handle_resize_palette(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local palette_index = payload.palette or 1
    local palette = spr.palettes[palette_index]
    if not palette then
        return error_response(cmd.id, "palette_not_found", "Palette was not found", { palette = palette_index })
    end
    local count = payload.count
    if type(count) ~= "number" or count < 1 then
        return error_response(cmd.id, "invalid_palette_size", "count must be greater than zero")
    end

    palette:resize(math.floor(count))
    spr:setPalette(palette)
    app.refresh()
    return ok_response(cmd.id, {
        palette = palette_index,
        count = #palette,
    })
end

local function handle_run_app_command(cmd)
    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_command_name", "Aseprite app.command name is required")
    end
    if type(name) ~= "string" or not name:match("^[A-Za-z_][A-Za-z0-9_]*$") then
        return error_response(cmd.id, "invalid_command_name", "Command name must be an Aseprite identifier")
    end

    local factory, load_err = load("return function(params) return app.command." .. name .. "(params or {}) end")
    if not factory then
        return error_response(cmd.id, "command_load_failed", "Aseprite app.command wrapper could not be created", { command = name, reason = tostring(load_err) })
    end
    local ok_factory, command = pcall(factory)
    if not ok_factory or type(command) ~= "function" then
        return error_response(cmd.id, "command_load_failed", "Aseprite app.command wrapper could not be initialized", { command = name, reason = tostring(command) })
    end

    local params = payload.params or {}
    local ok, result = pcall(function()
        return command(params)
    end)
    if not ok then
        return error_response(cmd.id, "command_failed", "Aseprite app.command failed", { command = name, reason = tostring(result) })
    end

    app.refresh()
    return ok_response(cmd.id, {
        command = name,
        sprite = app.sprite and sprite_info(app.sprite) or nil,
    })
end

local function handle_clear_cel(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local target = get_target(cmd)
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local layer = ensure_layer(spr, target.layer)

    app.transaction("MCP Live Clear Cel", function()
        app.layer = layer
        app.frame = frame
        local cel = layer:cel(frame)
        if cel then
            cel.image = Image(spr.width, spr.height, spr.colorMode)
            cel.position = Point(0, 0)
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = layer.name,
    })
end

local function handle_draw_pixels(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local pixels = payload.pixels or {}
    if #pixels == 0 then
        return error_response(cmd.id, "invalid_pixels", "pixels cannot be empty")
    end
    for _, pixel in ipairs(pixels) do
        local _, color_err = parse_hex_color(pixel.color)
        if color_err then
            return error_response(cmd.id, "invalid_color", color_err)
        end
    end

    local target = get_target(cmd)
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local layer = ensure_layer(spr, target.layer)
    local pixel_count = 0

    app.transaction("MCP Live Draw Pixels", function()
        app.layer = layer
        app.frame = frame

        local cel = get_or_create_cel(spr, layer, frame)
        local img = cel.image
        local pos = cel.position

        for _, pixel in ipairs(pixels) do
            local color = parse_hex_color(pixel.color)

            img:drawPixel(
                pixel.x - pos.x,
                pixel.y - pos.y,
                color_to_pixel(color)
            )
            pixel_count = pixel_count + 1
        end
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        pixelCount = pixel_count,
        layer = layer.name,
    })
end

local function supported_tool(tool_name)
    return tool_name == "pencil"
        or tool_name == "line"
        or tool_name == "rectangle"
        or tool_name == "filled_rectangle"
        or tool_name == "ellipse"
        or tool_name == "filled_ellipse"
        or tool_name == "paint_bucket"
        or tool_name == "eraser"
end

local function handle_use_tool(cmd)
    local spr, err = ensure_sprite()
    if err then
        err.id = cmd.id
        return err
    end

    local payload = cmd.payload or {}
    local tool_name = payload.tool or "pencil"
    if not supported_tool(tool_name) then
        return error_response(cmd.id, "unsupported_tool", "Unsupported live tool: " .. tostring(tool_name))
    end

    local points = payload.points or {}
    if #points == 0 then
        return error_response(cmd.id, "invalid_points", "points cannot be empty")
    end

    local color, color_err = parse_hex_color(payload.color or "#000000ff")
    if color_err then
        return error_response(cmd.id, "invalid_color", color_err)
    end

    local target = get_target(cmd)
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local layer = ensure_layer(spr, target.layer)
    local lua_points = {}
    for _, point in ipairs(points) do
        table.insert(lua_points, Point(point.x, point.y))
    end

    app.transaction("MCP Live Use Tool", function()
        app.layer = layer
        app.frame = frame
        get_or_create_cel(spr, layer, frame)

        app.useTool {
            tool = tool_name,
            color = Color(color.r, color.g, color.b, color.a),
            brush = Brush(payload.brushSize or 1),
            points = lua_points,
        }
    end)

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        tool = tool_name,
        pointCount = #points,
        layer = layer.name,
    })
end

----------------------------------------------------------------------
-- SPEC-003: tilemap / tileset / autotile handlers.
-- A tilemap cel is an image whose "pixels" are tile indices, so placement
-- reuses Image:putPixel/getPixel almost verbatim. These commands require
-- Aseprite 1.3+ scripting of tilemaps; they fail loudly (never silently) on
-- older builds. Capability is advertised via get_capabilities `features`.
----------------------------------------------------------------------

-- Tile-reference pixel values carry flip flags in the high bits; the low 29 are
-- the tile index. Prefer the API helper when present, else mask.
local TILE_INDEX_MASK = 0x1fffffff
local function tile_index_of(value)
    if app.pixelColor and app.pixelColor.tileI then
        local ok, idx = pcall(app.pixelColor.tileI, value)
        if ok and idx then
            return idx
        end
    end
    if type(value) ~= "number" then
        return 0
    end
    return value & TILE_INDEX_MASK
end

-- A new tilemap inherits the ACTIVE tilemap layer's tileset grid (Aseprite
-- 1.3.x), ignoring spr.gridBounds. So before creating one we anchor onto a
-- non-tilemap layer, letting spr.gridBounds drive the new tile size. Returns
-- the first non-group, non-tilemap layer (searching groups), or nil.
local function first_non_tilemap_layer(layers)
    for _, l in ipairs(layers) do
        if safe_get(l, "isGroup") then
            local found = first_non_tilemap_layer(safe_get(l, "layers") or {})
            if found then return found end
        elseif not safe_get(l, "isTilemap") then
            return l
        end
    end
    return nil
end

-- Create a tilemap layer of the requested tile size, defeating the
-- active-tilemap grid inheritance three ways: anchor onto a non-tilemap layer,
-- set spr.gridBounds, and pass gridBounds to NewLayer (ignored if unsupported).
local function new_tilemap_layer(spr, tw, th, name)
    local anchor = first_non_tilemap_layer(spr.layers)
    if anchor then app.layer = anchor end
    spr.gridBounds = Rectangle(0, 0, tw, th)
    app.command.NewLayer { tilemap = true, gridBounds = Rectangle(0, 0, tw, th) }
    local layer = app.layer
    if layer and name then layer.name = name end
    return layer
end

local function tileset_info(ts)
    local grid = safe_get(ts, "grid")
    local size = grid and safe_get(grid, "tileSize") or nil
    local count = 0
    pcall(function() count = #ts end)
    return {
        name = safe_get(ts, "name"),
        tileCount = count,
        baseIndex = safe_get(ts, "baseIndex"),
        tileWidth = size and size.width or nil,
        tileHeight = size and size.height or nil,
    }
end

-- Resolve a Tileset by a tilemap layer name (preferred) or 1-based index.
local function resolve_tileset(spr, index, layer_name)
    if layer_name and layer_name ~= "" then
        local layer = find_layer(spr.layers, layer_name)
        if not layer then
            return nil, "layer_not_found"
        end
        if not safe_get(layer, "isTilemap") then
            return nil, "not_a_tilemap"
        end
        return safe_get(layer, "tileset"), nil
    end
    if index then
        local list = safe_get(spr, "tilesets") or {}
        local ts = list[index]
        if not ts then
            return nil, "tileset_not_found"
        end
        return ts, nil
    end
    return nil, "missing_selector"
end

-- Pack every tile in a tileset into a near-square grid Image (in the sprite's
-- colour mode, so drawImage never crosses colour modes). Returns the unsaved
-- sheet plus its geometry; callers save it (with a palette for indexed sprites).
local function pack_tileset_image(spr, ts, image_columns)
    local count = #ts
    local gs = safe_get(ts, "grid") and ts.grid.tileSize
    local tw = (gs and gs.width) or spr.gridBounds.width
    local th = (gs and gs.height) or spr.gridBounds.height
    local cols = image_columns
    if not cols or cols < 1 then
        cols = math.max(1, math.ceil(math.sqrt(math.max(count, 1))))
    end
    local rows = math.max(1, math.ceil(count / cols))
    local sheet = Image(cols * tw, rows * th, spr.colorMode)
    sheet:clear()
    for i = 0, count - 1 do
        local tile_img
        local ok = pcall(function() tile_img = ts:getTile(i) end)
        if (not ok) or (not tile_img) then
            pcall(function() tile_img = ts:tile(i).image end)
        end
        if tile_img then
            sheet:drawImage(tile_img, Point((i % cols) * tw, math.floor(i / cols) * th))
        end
    end
    return sheet, tw, th, cols, rows
end

local function save_sheet(spr, sheet, path)
    if spr.colorMode == ColorMode.INDEXED then
        sheet:saveAs { filename = path, palette = spr.palettes[1] }
    else
        sheet:saveAs(path)
    end
end

local function handle_create_tilemap_layer(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local name = payload.name
    if not name or name == "" then
        return error_response(cmd.id, "missing_name", "Tilemap layer name is required")
    end
    local tw = payload.tileWidth or 16
    local th = payload.tileHeight or tw
    if type(tw) ~= "number" or tw < 1 or tw ~= math.floor(tw)
        or type(th) ~= "number" or th < 1 or th ~= math.floor(th) then
        return error_response(cmd.id, "invalid_tile_size", "tileWidth and tileHeight must be positive integers")
    end

    local layer
    local ok_tx, tx_err = pcall(function()
        app.transaction("MCP Live Create Tilemap Layer", function()
            layer = new_tilemap_layer(spr, tw, th, name)
        end)
    end)
    if not ok_tx then
        return error_response(cmd.id, "tilemap_create_failed", "Could not create tilemap layer", { reason = tostring(tx_err) })
    end
    if not layer or not safe_get(layer, "isTilemap") then
        return error_response(cmd.id, "tilemap_unsupported",
            "Aseprite did not create a tilemap layer (NewLayer{tilemap=true} unsupported in this build?)",
            { appVersion = tostring(app.version) })
    end

    app.refresh()
    local ts = safe_get(layer, "tileset")
    return ok_response(cmd.id, {
        changed = true,
        layer = layer.name,
        tileset = ts and tileset_info(ts) or nil,
    })
end

local function handle_list_tilesets(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local tilesets = {}
    local list = safe_get(spr, "tilesets") or {}
    for index, ts in ipairs(list) do
        local info = tileset_info(ts)
        info.index = index
        table.insert(tilesets, info)
    end
    return ok_response(cmd.id, { tilesets = tilesets, count = #tilesets })
end

local function handle_get_tileset(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local ts, terr = resolve_tileset(spr, payload.index, payload.layer)
    if not ts then
        return error_response(cmd.id, terr or "tileset_not_found", "Could not resolve tileset",
            { index = payload.index, layer = payload.layer })
    end

    local count = #ts
    local tiles = {}
    for i = 0, count - 1 do
        local tile
        pcall(function() tile = ts:tile(i) end)
        table.insert(tiles, { index = i, data = tile and safe_get(tile, "data") or nil })
    end

    local result = { tileset = tileset_info(ts), tiles = tiles }

    if payload.dumpPath and payload.dumpPath ~= "" then
        local ok_dump, dump_err = pcall(function()
            local sheet, tw, th, cols, rows = pack_tileset_image(spr, ts, payload.imageColumns)
            save_sheet(spr, sheet, payload.dumpPath)
            result.packed = {
                path = payload.dumpPath,
                width = cols * tw, height = rows * th,
                tileWidth = tw, tileHeight = th,
                columns = cols, rows = rows,
            }
        end)
        if not ok_dump then
            return error_response(cmd.id, "tileset_dump_failed", "Could not pack tileset PNG", { reason = tostring(dump_err) })
        end
    end

    return ok_response(cmd.id, result)
end

local function handle_stamp_tiles(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local tiles = payload.tiles or {}
    if #tiles == 0 then
        return error_response(cmd.id, "invalid_tiles", "tiles cannot be empty")
    end

    local target = get_target(cmd)
    local layer = find_layer(spr.layers, target.layer)
    if not layer then
        return error_response(cmd.id, "layer_not_found", "Tilemap layer was not found", { layer = target.layer })
    end
    if not safe_get(layer, "isTilemap") then
        return error_response(cmd.id, "not_a_tilemap", "Layer is not a tilemap layer", { layer = target.layer })
    end
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local ts = layer.tileset
    local tile_count = #ts
    for _, t in ipairs(tiles) do
        if type(t.tileIndex) ~= "number" or t.tileIndex < 0 or t.tileIndex ~= math.floor(t.tileIndex) then
            return error_response(cmd.id, "invalid_tile_index", "tileIndex must be a non-negative integer", { tileIndex = t.tileIndex })
        end
        if t.tileIndex >= tile_count then
            return error_response(cmd.id, "tile_index_out_of_range", "tileIndex exceeds tileset size",
                { tileIndex = t.tileIndex, tileCount = tile_count })
        end
    end

    local placed, skipped = 0, 0
    local ok_tx, tx_err = pcall(function()
        app.transaction("MCP Live Stamp Tiles", function()
            app.layer = layer
            app.frame = frame
            local gs = safe_get(ts, "grid") and ts.grid.tileSize
            local tw = (gs and gs.width) or spr.gridBounds.width
            local th = (gs and gs.height) or spr.gridBounds.height
            local cols = math.max(1, math.ceil(spr.width / tw))
            local rows = math.max(1, math.ceil(spr.height / th))

            -- Read the existing grid into a Lua table (honouring the cel's tile
            -- offset), apply the stamps, then rebuild a fresh tilemap image and
            -- replace the cel. Mutating cel.image in place + reassigning does NOT
            -- persist for a tilemap cel; newCel (the path pack uses) does.
            local grid = {}
            for ry = 0, rows - 1 do
                grid[ry] = {}
                for cx = 0, cols - 1 do grid[ry][cx] = 0 end
            end
            local existing = layer:cel(frame)
            if existing and existing.image then
                local eimg = existing.image
                local ecx = math.floor((existing.position.x or 0) / tw)
                local ecy = math.floor((existing.position.y or 0) / th)
                for ry = 0, eimg.height - 1 do
                    for cx = 0, eimg.width - 1 do
                        local gx, gy = cx + ecx, ry + ecy
                        if grid[gy] and gx >= 0 and gx < cols then
                            grid[gy][gx] = tile_index_of(eimg:getPixel(cx, ry))
                        end
                    end
                end
            end

            for _, t in ipairs(tiles) do
                -- JSON numbers arrive as Lua floats; tilemap putPixel needs an
                -- INTEGER tile index (a float like 2.0 is silently written as the
                -- empty tile 0), so coerce x/y/index to integers.
                local tx = math.floor(t.x)
                local ty = math.floor(t.y)
                local ti = math.floor(t.tileIndex)
                if grid[ty] and tx >= 0 and tx < cols then
                    grid[ty][tx] = ti
                    placed = placed + 1
                else
                    skipped = skipped + 1
                end
            end

            local img = Image(ImageSpec { width = cols, height = rows, colorMode = ColorMode.TILEMAP })
            for ry = 0, rows - 1 do
                for cx = 0, cols - 1 do
                    img:putPixel(cx, ry, math.floor(grid[ry][cx]))
                end
            end
            if existing then
                spr:deleteCel(layer, frame)
            end
            spr:newCel(layer, frame, img, Point(0, 0))
        end)
    end)
    if not ok_tx then
        return error_response(cmd.id, "stamp_failed", "Could not stamp tiles", { reason = tostring(tx_err) })
    end

    app.refresh()
    return ok_response(cmd.id, {
        changed = true,
        layer = layer.name,
        placed = placed,
        skippedOutOfBounds = skipped,
    })
end

local function handle_set_tile_data(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local ts, terr = resolve_tileset(spr, payload.tilesetIndex, payload.layer)
    if not ts then
        return error_response(cmd.id, terr or "tileset_not_found", "Could not resolve tileset",
            { tilesetIndex = payload.tilesetIndex, layer = payload.layer })
    end
    local idx = payload.tileIndex
    if type(idx) ~= "number" or idx < 0 or idx >= #ts then
        return error_response(cmd.id, "tile_index_out_of_range", "tileIndex out of range", { tileIndex = idx, tileCount = #ts })
    end
    local tile
    pcall(function() tile = ts:tile(idx) end)
    if not tile then
        return error_response(cmd.id, "tile_not_found", "Tile not found", { tileIndex = idx })
    end

    local ok_tx, tx_err = pcall(function()
        app.transaction("MCP Live Set Tile Data", function()
            if payload.data ~= nil then
                tile.data = payload.data
            end
        end)
    end)
    if not ok_tx then
        return error_response(cmd.id, "set_tile_data_failed", "Could not set tile data", { reason = tostring(tx_err) })
    end

    app.refresh()
    return ok_response(cmd.id, { changed = true, tileIndex = idx, data = safe_get(tile, "data") })
end

-- Phase 2: deduplicate a painted mockup into a tileset + a tilemap that
-- reconstructs it. Tile 0 stays the empty tile; identical cells share an index.
local function handle_pack_similar_tiles(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local tw = payload.tileWidth
    local th = payload.tileHeight or tw
    if type(tw) ~= "number" or tw < 1 or type(th) ~= "number" or th < 1 then
        return error_response(cmd.id, "invalid_tile_size", "tileWidth and tileHeight must be positive integers")
    end

    local source_layer
    if payload.layer and payload.layer ~= "" then
        source_layer = find_layer(spr.layers, payload.layer)
        if not source_layer then
            return error_response(cmd.id, "layer_not_found", "Source layer was not found", { layer = payload.layer })
        end
    else
        source_layer = app.layer
    end

    local frame = app.frame or spr.frames[1]
    local cols = math.floor(spr.width / tw)
    local rows = math.floor(spr.height / th)
    if cols < 1 or rows < 1 then
        return error_response(cmd.id, "tile_too_large", "Tile size is larger than the canvas")
    end

    -- Flatten the source layer's active frame to a full-canvas image.
    local source = Image(spr.width, spr.height, spr.colorMode)
    source:clear()
    local scel = source_layer and source_layer:cel(frame)
    if scel and scel.image then
        source:drawImage(scel.image, scel.position)
    end

    -- Dedupe cells. uniques = list of Image; index_map[ry][cx] = tile index.
    local uniques = {}
    local index_map = {}
    local function extract(cx, ry)
        local t = Image(tw, th, spr.colorMode)
        t:clear()
        t:drawImage(source, Point(-cx * tw, -ry * th))
        return t
    end
    for ry = 0, rows - 1 do
        index_map[ry] = {}
        for cx = 0, cols - 1 do
            local tile = extract(cx, ry)
            if tile:isEmpty() then
                index_map[ry][cx] = 0  -- empty tile
            else
                local found
                for u = 1, #uniques do
                    if uniques[u]:isEqual(tile) then found = u break end
                end
                if not found then
                    table.insert(uniques, tile)
                    found = #uniques
                end
                index_map[ry][cx] = found  -- 1-based: tile 0 is empty, uniques start at 1
            end
        end
    end

    local tmlayer
    local ok_tx, tx_err = pcall(function()
        app.transaction("MCP Live Pack Similar Tiles", function()
            local tm_name = (payload.tilemapLayer and payload.tilemapLayer ~= "" and payload.tilemapLayer) or "Tilemap"
            tmlayer = new_tilemap_layer(spr, tw, th, tm_name)
            local ts = tmlayer.tileset
            -- Add one tile per unique (tile 0 already exists, empty).
            for _, uimg in ipairs(uniques) do
                local tile = spr:newTile(ts)
                tile.image = uimg
            end
            -- Build the tilemap cel referencing unique tiles.
            local image = Image(ImageSpec { width = cols, height = rows, colorMode = ColorMode.TILEMAP })
            for ry = 0, rows - 1 do
                for cx = 0, cols - 1 do
                    image:putPixel(cx, ry, index_map[ry][cx])
                end
            end
            spr:newCel(tmlayer, frame, image, Point(0, 0))
        end)
    end)
    if not ok_tx then
        return error_response(cmd.id, "pack_failed", "Could not pack similar tiles", { reason = tostring(tx_err) })
    end

    app.refresh()
    local cells = cols * rows
    local unique_tiles = #uniques
    return ok_response(cmd.id, {
        changed = true,
        layer = tmlayer and tmlayer.name or nil,
        columns = cols, rows = rows,
        cells = cells,
        uniqueTiles = unique_tiles,
        tilesetSize = unique_tiles + 1,  -- + empty tile 0
        summary = string.format("%d cells -> %d unique tiles", cells, unique_tiles),
    })
end

-- Phase 5 data fetch: read a tilemap layer's grid + pack its tileset PNG. The
-- engine-format bytes are serialized by the Rust tileset_export module.
local function handle_export_tilemap(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end

    local payload = cmd.payload or {}
    local target = get_target(cmd)
    local layer
    if target.layer and target.layer ~= "AI Draft" then
        layer = find_layer(spr.layers, target.layer)
    else
        layer = app.layer
    end
    if not layer or not safe_get(layer, "isTilemap") then
        layer = nil
        for _, l in ipairs(spr.layers) do
            if safe_get(l, "isTilemap") then layer = l break end
        end
    end
    if not layer then
        return error_response(cmd.id, "no_tilemap_layer", "No tilemap layer found to export")
    end
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return error_response(cmd.id, "invalid_frame", frame_err)
    end

    local ts = layer.tileset
    local gs = safe_get(ts, "grid") and ts.grid.tileSize
    local tw = (gs and gs.width) or spr.gridBounds.width
    local th = (gs and gs.height) or spr.gridBounds.height
    local tile_count = #ts

    local cel = layer:cel(frame)
    local cols, rows, grid = 0, 0, {}
    if cel and cel.image then
        local img = cel.image
        cols, rows = img.width, img.height
        for ry = 0, rows - 1 do
            local row = {}
            for cx = 0, cols - 1 do
                row[#row + 1] = tile_index_of(img:getPixel(cx, ry))
            end
            grid[#grid + 1] = row
        end
    end

    local packed_cols = 0
    if payload.imagePath and payload.imagePath ~= "" then
        local ok_dump, derr = pcall(function()
            local sheet, _, _, pcols = pack_tileset_image(spr, ts, payload.imageColumns)
            packed_cols = pcols
            save_sheet(spr, sheet, payload.imagePath)
        end)
        if not ok_dump then
            return error_response(cmd.id, "tileset_dump_failed", "Could not pack tileset PNG", { reason = tostring(derr) })
        end
    end

    return ok_response(cmd.id, {
        layer = layer.name,
        tileWidth = tw, tileHeight = th,
        columns = cols, rows = rows,
        tileCount = tile_count,
        imageColumns = packed_cols,
        grid = grid,
    })
end

----------------------------------------------------------------------
-- SPEC-004: constrained / semantic colour ops. The plugin only READS a
-- region's unique colours and REPLACES colours by a from→to map; all the
-- colour MATH (real CIELAB snap, darken/lighten hue-shift, etc.) lives in
-- the Rust `color_ops` module. RGB mode only for v1.
----------------------------------------------------------------------

local function pixel_to_hex(v)
    return string.format("#%02x%02x%02x%02x",
        app.pixelColor.rgbaR(v), app.pixelColor.rgbaG(v),
        app.pixelColor.rgbaB(v), app.pixelColor.rgbaA(v))
end

-- Resolve a colour-op target: a layer (named, else active) + frame + whether to
-- limit to the active selection. Does NOT reject by layer type (the write path
-- does that); the read path simply skips non-image layers.
local function resolve_color_target(spr, cmd)
    local payload = cmd.payload or {}
    local target = get_target(cmd)
    local layer
    if target.layer and target.layer ~= "AI Draft" then
        layer = find_layer(spr.layers, target.layer)
    else
        layer = app.layer
    end
    if not layer then
        return nil, nil, nil, error_response(cmd.id, "layer_not_found", "Layer was not found", { layer = target.layer })
    end
    local frame, frame_err = resolve_frame(spr, target.frame)
    if frame_err then
        return nil, nil, nil, error_response(cmd.id, "invalid_frame", frame_err)
    end
    local use_sel = false
    if payload.selectionOnly then
        local sel = spr.selection
        if not sel or sel.isEmpty then
            -- Refuse rather than silently widening scope to the whole layer: a
            -- caller asking selection_only with nothing selected almost certainly
            -- did not mean "recolour everything" (audit finding).
            return nil, nil, nil, error_response(cmd.id, "empty_selection",
                "selection_only was requested but there is no active selection", nil)
        end
        use_sel = true
    end
    return layer, frame, use_sel, nil
end

local function handle_get_region_colors(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end
    if spr.colorMode ~= ColorMode.RGB then
        return error_response(cmd.id, "unsupported_color_mode",
            "colour ops require RGB mode; convert the sprite to RGB first",
            { colorMode = tostring(spr.colorMode) })
    end

    local layer, frame, use_sel, terr = resolve_color_target(spr, cmd)
    if terr then return terr end

    -- Unique colours of the region (only for a regular image layer; the palette
    -- is read regardless, so this also serves a palette-only read). The
    -- `imageLayer` flag lets the colour-edit tools surface a clear error on a
    -- group/tilemap layer instead of a confusing "0 colours" no-op.
    local is_image_layer = (safe_get(layer, "isImage") == true)
        and not safe_get(layer, "isTilemap")
        and not safe_get(layer, "isGroup")
    local seen, colors = {}, {}
    local cel = layer:cel(frame)
    if cel and cel.image and is_image_layer then
        local img = cel.image
        local pos = cel.position
        local sel = spr.selection
        local ok_iter, iter_err = pcall(function()
            for px in img:pixels() do
                if (not use_sel) or sel:contains(px.x + pos.x, px.y + pos.y) then
                    local hex = pixel_to_hex(px())
                    if not seen[hex] then
                        seen[hex] = true
                        colors[#colors + 1] = hex
                    end
                end
            end
        end)
        if not ok_iter then
            return error_response(cmd.id, "pixel_read_failed", "Could not read cel pixels", { reason = tostring(iter_err) })
        end
    end

    local palette = {}
    local pal = spr.palettes[1]
    if pal then
        for i = 0, #pal - 1 do
            local c = pal:getColor(i)
            palette[#palette + 1] = string.format("#%02x%02x%02x%02x", c.red, c.green, c.blue, c.alpha)
        end
    end

    return ok_response(cmd.id, {
        layer = layer.name,
        colors = colors,
        palette = palette,
        selectionOnly = use_sel,
        imageLayer = is_image_layer,
    })
end

local function handle_apply_color_map(cmd)
    local spr, err = ensure_sprite()
    if err then err.id = cmd.id return err end
    if spr.colorMode ~= ColorMode.RGB then
        return error_response(cmd.id, "unsupported_color_mode",
            "colour ops require RGB mode; convert the sprite to RGB first",
            { colorMode = tostring(spr.colorMode) })
    end

    local payload = cmd.payload or {}
    local map = payload.map or {}
    if #map == 0 then
        return ok_response(cmd.id, { changed = false, pixels = 0, colors = 0 })
    end

    local layer, frame, use_sel, terr = resolve_color_target(spr, cmd)
    if terr then return terr end
    if safe_get(layer, "isGroup") or safe_get(layer, "isTilemap") then
        return error_response(cmd.id, "not_an_image_layer",
            "colour ops need a regular image layer (not a group or tilemap)", { layer = layer.name })
    end

    -- from-value → to-value lookup (RGBA pixel ints).
    local vmap = {}
    for _, m in ipairs(map) do
        local fc, ferr = parse_hex_color(m.from)
        local tc, terr2 = parse_hex_color(m.to)
        if ferr or terr2 then
            return error_response(cmd.id, "invalid_color", ferr or terr2, { from = m.from, to = m.to })
        end
        vmap[color_to_pixel(fc)] = color_to_pixel(tc)
    end

    local cel = layer:cel(frame)
    if not cel or not cel.image then
        return ok_response(cmd.id, { changed = false, pixels = 0, colors = #map })
    end

    local count = 0
    local ok_tx, tx_err = pcall(function()
        app.transaction("MCP Live Apply Color Map", function()
            app.layer = layer
            app.frame = frame
            -- Clone → mutate → reassign: the undo-safe pattern for image cels.
            local img = cel.image:clone()
            local pos = cel.position
            local sel = spr.selection
            for px in img:pixels() do
                local nv = vmap[px()]
                if nv ~= nil and ((not use_sel) or sel:contains(px.x + pos.x, px.y + pos.y)) then
                    px(nv)
                    count = count + 1
                end
            end
            cel.image = img
        end)
    end)
    if not ok_tx then
        return error_response(cmd.id, "apply_failed", "Could not apply colour map", { reason = tostring(tx_err) })
    end

    app.refresh()
    return ok_response(cmd.id, { changed = count > 0, pixels = count, colors = #map, layer = layer.name })
end

local HANDLERS = {
    ping = function(cmd)
        return ok_response(cmd.id, { status = "pong" })
    end,
    list_sprites = handle_list_sprites,
    new_sprite = handle_new_sprite,
    open_sprite = handle_open_sprite,
    activate_sprite = handle_activate_sprite,
    get_active_site = handle_get_active_site,
    get_sprite_info = handle_get_sprite_info,
    set_sprite_properties = handle_set_sprite_properties,
    save_sprite = handle_save_sprite,
    save_sprite_as = handle_save_sprite_as,
    save_copy_as = handle_save_copy_as,
    save_preview = handle_save_preview,
    close_sprite = handle_close_sprite,
    resize_canvas = handle_resize_canvas,
    resize_sprite = handle_resize_canvas,
    list_layers = handle_list_layers,
    ensure_layer = handle_ensure_layer,
    set_layer_visibility = handle_set_layer_visibility,
    set_active_layer = handle_set_active_layer,
    rename_layer = handle_rename_layer,
    create_group_layer = handle_create_group_layer,
    set_layer_properties = handle_set_layer_properties,
    delete_layer = handle_delete_layer,
    ensure_frames = handle_ensure_frames,
    list_frames = handle_list_frames,
    set_active_frame = handle_set_active_frame,
    set_frame_properties = handle_set_frame_properties,
    new_empty_frame = handle_new_empty_frame,
    new_frame = handle_new_frame,
    delete_frame = handle_delete_frame,
    list_cels = handle_list_cels,
    new_cel = handle_new_cel,
    set_cel_properties = handle_set_cel_properties,
    delete_cel = handle_delete_cel,
    list_tags = handle_list_tags,
    new_tag = handle_new_tag,
    set_tag_properties = handle_set_tag_properties,
    delete_tag = handle_delete_tag,
    list_slices = handle_list_slices,
    new_slice = handle_new_slice,
    set_slice_properties = handle_set_slice_properties,
    delete_slice = handle_delete_slice,
    get_selection = handle_get_selection,
    set_selection = handle_set_selection,
    list_palette = handle_list_palette,
    set_palette_color = handle_set_palette_color,
    resize_palette = handle_resize_palette,
    run_app_command = handle_run_app_command,
    clear_cel = handle_clear_cel,
    draw_pixels = handle_draw_pixels,
    use_tool = handle_use_tool,
    -- SPEC-003 tilemap / tileset / autotile
    create_tilemap_layer = handle_create_tilemap_layer,
    list_tilesets = handle_list_tilesets,
    get_tileset = handle_get_tileset,
    stamp_tiles = handle_stamp_tiles,
    set_tile_data = handle_set_tile_data,
    pack_similar_tiles = handle_pack_similar_tiles,
    export_tilemap = handle_export_tilemap,
    -- SPEC-004 constrained / semantic colour ops
    get_region_colors = handle_get_region_colors,
    apply_color_map = handle_apply_color_map,
}

local function sorted_handler_names()
    local names = {}
    for name, _ in pairs(HANDLERS) do
        table.insert(names, name)
    end
    table.sort(names)
    return names
end

local function handle_get_capabilities(cmd)
    return ok_response(cmd.id, {
        protocol = PROTOCOL,
        protocolVersion = VERSION,
        pluginVersion = PLUGIN_VERSION,
        appVersion = tostring(app.version or "unknown"),
        apiVersion = app.apiVersion or 0,
        commands = sorted_handler_names(),
        features = FEATURES,
        config = {
            host = CONFIG.host,
            port = CONFIG.port,
            reconnectTick = CONFIG.reconnect_tick,
        },
    })
end

HANDLERS.get_capabilities = handle_get_capabilities

local function handle_command(cmd)
    if cmd.protocol ~= PROTOCOL or cmd.version ~= VERSION then
        return error_response(cmd.id, "unsupported_protocol", "Unsupported live edit protocol")
    end

    if type(cmd.id) ~= "string" or cmd.id == "" then
        return error_response(nil, "invalid_id", "Command id is required")
    end

    local handler = HANDLERS[cmd.type]
    if not handler then
        return error_response(cmd.id, "unsupported_command", "Unknown command: " .. tostring(cmd.type), { command = cmd.type })
    end

    local ok, result = pcall(handler, cmd)
    if ok then
        return result
    end
    return error_response(cmd.id, "execution_error", "Live command failed", { reason = tostring(result) })
end

local connect

local function send_hello()
    if not ws then
        return
    end

    safe_send_text(json.encode({
        protocol = PROTOCOL,
        version = VERSION,
        type = "hello",
        ok = true,
        result = {
            pluginVersion = PLUGIN_VERSION,
            appVersion = tostring(app.version or "unknown"),
            apiVersion = app.apiVersion or 0,
            protocol = PROTOCOL,
            protocolVersion = VERSION,
        },
    }))
end

local function stop_reconnect_timer()
    if reconnect_timer and reconnect_timer.isRunning then
        reconnect_timer:stop()
    end
    reconnect_timer = nil
end

local function start_reconnect_timer()
    if reconnect_timer and reconnect_timer.isRunning then
        return
    end

    reconnect_timer = Timer {
        interval = CONFIG.reconnect_tick,
        ontick = function()
            if not should_connect then
                stop_reconnect_timer()
                return
            end

            -- Safety net: if a connect attempt never reaches OPEN (and no CLOSE
            -- fires either), force a reset so reconnection can proceed instead
            -- of being stuck on connecting=true forever (would need an Aseprite
            -- restart otherwise).
            if connecting then
                connecting_ticks = connecting_ticks + 1
                if connecting_ticks >= CONFIG.connect_max_ticks then
                    mark_disconnected("connect attempt timed out")
                end
            end

            if connected and ws then
                local ok = pcall(function()
                    ws:sendPing("mcp")
                end)
                if ok then
                    ping_misses = 0
                else
                    -- Tolerate transient misses (window unfocused / throttled);
                    -- only declare dead after several consecutive failures.
                    ping_misses = ping_misses + 1
                    if ping_misses >= CONFIG.ping_max_misses then
                        mark_disconnected("ping failed repeatedly")
                    end
                end
            end

            if not ws and not connecting then
                connect()
            end
        end,
    }
    reconnect_timer:start()
end

connect = function()
    if connected or connecting then
        return
    end
    if ws then
        ws = nil
    end

    local url = string.format("ws://%s:%d", CONFIG.host, CONFIG.port)
    log("Connecting to " .. url)

    connecting = true
    connecting_ticks = 0
    ws = WebSocket {
        url = url,
        onreceive = function(messageType, data)
            if messageType == WebSocketMessageType.OPEN then
                connected = true
                connecting = false
                connecting_ticks = 0
                ping_misses = 0
                log("Connected")
                send_hello()

            elseif messageType == WebSocketMessageType.TEXT then
                log("Received: " .. data)
                local decoded, cmd = pcall(json.decode, data)
                local response = nil

                if decoded and cmd then
                    response = handle_command(cmd)
                else
                    response = error_response(nil, "invalid_json", "Invalid JSON")
                end

                local encoded = json.encode(response)
                log("Sending: " .. encoded)
                safe_send_text(encoded)

            elseif messageType == WebSocketMessageType.CLOSE then
                mark_disconnected("close event")
            end
        end,
        deflate = false,
        minreconnectwait = CONFIG.reconnect_min,
        maxreconnectwait = CONFIG.reconnect_max,
    }

    ws:connect()
end

function init(plugin)
    log("Aseprite MCP Live plugin initialized")

    pcall(function()
        plugin:newMenuGroup {
            id = "mcp_group",
            title = "MCP Server",
            group = "help_about",
        }

        plugin:newCommand {
            id = "mcp_connect",
            title = "Connect to MCP Server",
            group = "mcp_group",
            onclick = function()
                should_connect = true
                start_reconnect_timer()
                connect()
                app.alert("MCP: Connecting to " .. CONFIG.host .. ":" .. CONFIG.port)
            end,
        }

        plugin:newCommand {
            id = "mcp_disconnect",
            title = "Disconnect from MCP Server",
            group = "mcp_group",
            onclick = function()
                should_connect = false
                stop_reconnect_timer()
                -- Use mark_disconnected so connecting/connecting_ticks are reset
                -- too; otherwise a later Connect could no-op on a stuck flag.
                mark_disconnected("user disconnect")
                app.alert("MCP: Disconnected")
            end,
        }

        plugin:newCommand {
            id = "mcp_status",
            title = "MCP Connection Status",
            group = "mcp_group",
            onclick = function()
                if connected then
                    app.alert("MCP: Connected to " .. CONFIG.host .. ":" .. CONFIG.port)
                else
                    app.alert("MCP: Not connected")
                end
            end,
        }
    end)

    should_connect = true
    start_reconnect_timer()
    connect()
end

function exit(plugin)
    should_connect = false
    stop_reconnect_timer()
    if ws then
        ws:close()
    end
    log("Aseprite MCP Live plugin exited")
end
