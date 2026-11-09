-- The conformance suite's test plugin: each call of `run` makes the calls that the host gives it through their
-- functions of `cx`, in their order, and returns what each answered, all its values; it returns each batch of
-- events as it took it; its panel's tree is of a field and a list.

local Plugin = {}

function Plugin.run(cx, calls)
    local answers = {}
    for at, call in calls do
        answers[at] = table.pack(cx[call.name](table.unpack(call.args, 1, call.n)))
    end
    return answers
end

function Plugin.on_event(_cx, events)
    return events
end

function Plugin.render(_cx, view)
    if view.panel == nil then
        return nil
    end
    return {
        kind = "container",
        children = {
            { kind = "field", placeholder = "Find", key = "query" },
            {
                kind = "list",
                row = { kind = "text", text = "row" },
                count = 2,
                first = 0,
                items = { {}, {} },
                key = "results",
            },
        },
    }
end

return Plugin
