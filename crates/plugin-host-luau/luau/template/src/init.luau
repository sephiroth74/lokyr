--!strict
-- A plugin of a command, Say Hello, which shows a notice in a panel at the
-- top of the focused window: a click outside it or Escape closes it, and
-- it goes by itself after three seconds.

local Plugin: Plugin = {}

-- The notice's panel, and the timer that closes it, if it shows.
local NOTICE = "notice"
local closing: number? = nil

function Plugin.activate(cx: Cx)
    assert(cx.register_command({ name = "hello.say", title = "Say Hello", category = "view" }))
end

function Plugin.on_command(cx: Cx, name: string, args: Args)
    if name ~= "hello.say" then
        return
    end
    local window = cx.focused_window()
    if window == nil then
        return
    end
    assert(cx.open_panel(NOTICE, {
        anchor = { window_corner = { window = window, corner = "top" } },
        role = "overlay",
        close_outside = true,
        close_escape = true,
    }))
    if closing ~= nil then
        cx.cancel_timer(closing)
    end
    closing = assert(cx.start_timer(3000, false))
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        if event.kind == "timer" and event.id == closing then
            closing = nil
            cx.close_panel(NOTICE)
        elseif event.kind == "panel-closed" and event.id == NOTICE and closing ~= nil then
            cx.cancel_timer(closing)
            closing = nil
        end
    end
end

function Plugin.render(cx: Cx, view: ViewId): Node?
    -- The notice's text, of the plugin's catalog in the language shown.
    local text = cx.message("hello-notice") or "Hello!"
    local line: Node = { kind = "text", key = "text", text = text }
    return {
        kind = "container",
        key = "notice",
        role = "alert",
        layout = { padding = { top = 8, right = 12, bottom = 8, left = 12 } },
        style = {
            background = "panel.bg",
            border = { width = 1, color = "panel.border" },
            radius = 8,
            shadow = { color = "shadow", y = 2, blur = 12 },
        },
        children = { line },
    }
end

return Plugin
