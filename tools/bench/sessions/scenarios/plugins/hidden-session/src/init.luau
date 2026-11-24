--!strict
-- The hidden session's scenario of Lokyr's benchmarks. As Lokyr starts, as
-- soon as its first window shows: a second session, of a window of its
-- own, then the first session again, so that the second's window hides,
-- its shell running, as a session that someone switched away from.

local Plugin: Plugin = {}

-- How often the plugin looks for the first window, and how long the second
-- session's window takes to show before the first comes back.
local LOOK_MS = 100
local SHOW_MS = 1500

local looking: number? = nil
local waiting: number? = nil
local first: SessionId? = nil

-- Opens the second session, once the first window shows: whether it did.
local function open(cx: Cx): boolean
    if cx.focused_window() == nil then
        return false
    end
    local sessions = assert(cx.sessions())
    for _, session in sessions do
        if session.active then
            first = session.id
        end
    end
    assert(cx.new_session("hidden", nil))
    return true
end

function Plugin.activate(cx: Cx)
    looking = assert(cx.start_timer(LOOK_MS, true))
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        if event.kind ~= "timer" then
            continue
        end
        if event.id == looking and open(cx) then
            cx.cancel_timer(event.id)
            looking = nil
            waiting = assert(cx.start_timer(SHOW_MS, false))
        elseif event.id == waiting then
            waiting = nil
            if first ~= nil then
                assert(cx.switch_session(first))
            end
        end
    end
end

return Plugin
