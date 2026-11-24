--!strict
-- The floating pane's scenario of Lokyr's benchmarks. As Lokyr starts, as
-- soon as its first window shows: a popup, a floating pane of its own over
-- the first pane, of a program at rest, `cat`, which waits for input and
-- draws nothing, as a floating pane that someone left open.

local Plugin: Plugin = {}

-- How often the plugin looks for the first window.
local LOOK_MS = 100

local looking: number? = nil

-- Opens the popup over the first window, once it shows: whether it did.
local function open(cx: Cx): boolean
    local window = cx.focused_window()
    if window == nil then
        return false
    end
    assert(cx.open_popup(window, "cat", nil, 0.6, 0.6))
    return true
end

function Plugin.activate(cx: Cx)
    looking = assert(cx.start_timer(LOOK_MS, true))
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        if event.kind == "timer" and event.id == looking and open(cx) then
            cx.cancel_timer(event.id)
            looking = nil
        end
    end
end

return Plugin
