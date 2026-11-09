--!strict
-- A plugin of wrong fields, which luau-lsp's analysis finds, of the API's
-- types: a field read that its table lacks, a command lacking a field
-- that it requires, and a node of no such kind.

local Plugin: Plugin = {}

function Plugin.activate(cx: Cx)
    cx.register_command({ name = "wrong.go", titel = "Go", category = "view" })
end

function Plugin.on_event(cx: Cx, events: { Event })
    for _, event in events do
        if event.kind == "tab-created" then
            cx.activate_tab(event.tabb)
        end
    end
end

function Plugin.on_command(cx: Cx, name: string, args: Args)
    local line = cx.line_text(1, 0, 0)
    if line then
        cx.set_clipboard_text(line.txt)
    end
end

function Plugin.render(cx: Cx, view: ViewId): Node?
    return { kind = "textt", text = "x" }
end

return Plugin
