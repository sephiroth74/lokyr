--!strict
-- The broadcast's scenario of Lokyr's benchmarks. As Lokyr starts, as soon
-- as its first window shows: seven panes below the first; once their
-- shells run, each of a program that takes the keys and draws nothing; the
-- keyboard back on the first pane, which the measure types into; and the
-- typed input broadcast to the tab, so that each key goes to eight panes.

local Plugin: Plugin = {}

-- How often the plugin looks for the first window, and how long the new
-- panes' shells take to start before they take their program.
local LOOK_MS = 100
local SHELLS_MS = 1500

-- The panes beside, which draw nothing once their program runs.
local SILENT = "stty -echo; exec cat > /dev/null\r"

local looking: number? = nil
local waiting: number? = nil
local first: PaneId? = nil
local window: WindowId? = nil
local beside: { PaneId } = {}

-- Splits the first pane's part once across, the first pane above of the
-- window's whole width, which the measure's line of keys takes, then the
-- part below six times more, seven panes side by side, once the first
-- window shows: whether it did.
local function split(cx: Cx): boolean
    local focused = cx.focused_window()
    local pane = cx.focused_pane()
    if focused == nil or pane == nil then
        return false
    end
    window, first = focused, pane
    local last = assert(cx.split(pane, "vertical", "second"))
    table.insert(beside, last)
    for _ = 2, 7 do
        last = assert(cx.split(last, "horizontal", "second"))
        table.insert(beside, last)
    end
    return true
end

-- Gives the panes beside their program, the keyboard back to the first,
-- and broadcasts the typed input to the tab.
local function broadcast(cx: Cx)
    for _, pane in beside do
        assert(cx.send_text(pane, SILENT))
    end
    if first ~= nil then
        assert(cx.focus_pane(first))
    end
    if window ~= nil then
        assert(cx.set_broadcast(window, "tab"))
    end
end

function Plugin.activate(cx: Cx)
    looking = assert(cx.start_timer(LOOK_MS, true))
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        if event.kind ~= "timer" then
            continue
        end
        if event.id == looking and split(cx) then
            cx.cancel_timer(event.id)
            looking = nil
            waiting = assert(cx.start_timer(SHELLS_MS, false))
        elseif event.id == waiting then
            waiting = nil
            broadcast(cx)
        end
    end
end

return Plugin
