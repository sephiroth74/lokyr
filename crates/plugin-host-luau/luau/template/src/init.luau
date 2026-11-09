--!strict
-- A plugin of a command, which shows a notice of how many panes the window
-- shows.

local Plugin: Plugin = {}

function Plugin.activate(cx: Cx)
    cx.register_command({ name = "hello.count", title = "Count the panes", category = "view" })
    cx.bind({ keys = "cmd+shift+h", command = "hello.count", when = "pane.focused" })
end

function Plugin.on_command(cx: Cx, name: string, args: Args)
    if name ~= "hello.count" then
        return
    end
    local window = cx.focused_window()
    if window == nil then
        return
    end
    local tabs, message = cx.tabs(window)
    if tabs == nil then
        error(message)
    end
    local panes = 0
    for _, tab in tabs do
        panes += #tab.panes
    end
    cx.set_state({ panes = panes })
end

function Plugin.render(cx: Cx, view: ViewId): Node?
    return { kind = "text", key = "count", text = "Panes", role = "heading" }
end

return Plugin
