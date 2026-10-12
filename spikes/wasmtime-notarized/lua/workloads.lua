-- The plugin-like workloads of spike-workload, written in Lua the way a
-- plugin author would write them. They must return the same results as the
-- Rust versions: the host checks the counts. The code sticks to Lua 5.1, so
-- it runs on both LuaJIT and Luau.

local byte, find, lower, sub = string.byte, string.find, string.lower, string.sub

local KEYWORDS = {
    DEBUG = true, ERROR = true, INFO = true, WARN = true,
    ["else"] = true, ["false"] = true, fn = true, ["for"] = true,
    ["if"] = true, let = true, match = true, pub = true,
    ["return"] = true, ["true"] = true, use = true,
}

local function lower_byte(b)
    if b >= 65 and b <= 90 then
        return b + 32
    end
    return b
end

local function is_alnum(b)
    return (b >= 48 and b <= 57) or (b >= 65 and b <= 90) or (b >= 97 and b <= 122)
end

local function is_alpha(b)
    return (b >= 65 and b <= 90) or (b >= 97 and b <= 122)
end

local function is_digit(b)
    return b >= 48 and b <= 57
end

-- The bytes that Rust's u8::is_ascii_whitespace accepts.
local function is_space(b)
    return b == 32 or b == 9 or b == 10 or b == 12 or b == 13
end

-- Same algorithm as spike_workload::fuzzy_score, one byte at a time.
local function fuzzy_score(line, query)
    local wanted_count = #query
    local wanted_index = 1
    local wanted = lower_byte(byte(query, 1))
    local score = 0
    local previous_matched = false
    local previous = 32
    for i = 1, #line do
        if wanted_index > wanted_count then
            break
        end
        local c = byte(line, i)
        if lower_byte(c) == wanted then
            score = score + 1
            if previous_matched then
                score = score + 2
            end
            if not is_alnum(previous) then
                score = score + 3
            end
            wanted_index = wanted_index + 1
            if wanted_index <= wanted_count then
                wanted = lower_byte(byte(query, wanted_index))
            end
            previous_matched = true
        else
            previous_matched = false
        end
        previous = c
    end
    if wanted_index > wanted_count then
        return score
    end
    return nil
end

-- A subsequence pattern for `query`: "abc" becomes "a.-b.-c".
local function subsequence_pattern(query)
    local parts = {}
    for i = 1, #query do
        local c = lower(sub(query, i, i))
        if find(c, "%w") then
            parts[#parts + 1] = c
        else
            parts[#parts + 1] = "%" .. c
        end
    end
    return table.concat(parts, ".-")
end

-- Splits a line into spans {start, len, kind}, with a 0-based start, using
-- the standard library's patterns. Kinds match spike_workload::kind.
local function highlight_line(line)
    local spans, count = {}, 0
    local i, n = 1, #line
    while i <= n do
        if find(line, "^%s", i) then
            i = i + 1
        else
            local s, e = find(line, "^[%a_][%w_]*", i)
            local kind
            if s then
                kind = KEYWORDS[sub(line, s, e)] and 1 or 0
            else
                s, e = find(line, "^%d[%d%.]*", i)
                if s then
                    kind = 2
                else
                    s, e = find(line, '^"[^"]*"?', i)
                    if s then
                        kind = 3
                    else
                        s, e, kind = i, i, 4
                    end
                end
            end
            count = count + 1
            spans[count] = { s - 1, e - s + 1, kind }
            i = e + 1
        end
    end
    return spans
end

-- The same spans, found one byte at a time with the algorithm of
-- spike_workload::highlight: code that LuaJIT's compiler can turn into machine
-- code, which it cannot do for string patterns.
local function highlight_line_bytes(line)
    local spans, count = {}, 0
    local i, n = 1, #line
    while i <= n do
        local start = i
        local b = byte(line, i)
        local kind
        if is_alpha(b) or b == 95 then
            i = i + 1
            while i <= n do
                b = byte(line, i)
                if not (is_alnum(b) or b == 95) then
                    break
                end
                i = i + 1
            end
            kind = KEYWORDS[sub(line, start, i - 1)] and 1 or 0
        elseif is_digit(b) then
            i = i + 1
            while i <= n do
                b = byte(line, i)
                if not (is_digit(b) or b == 46) then
                    break
                end
                i = i + 1
            end
            kind = 2
        elseif b == 34 then
            i = i + 1
            while i <= n and byte(line, i) ~= 34 do
                i = i + 1
            end
            if i <= n then
                i = i + 1
            end
            kind = 3
        elseif is_space(b) then
            i = i + 1
        else
            i = i + 1
            kind = 4
        end
        if kind then
            count = count + 1
            spans[count] = { start - 1, i - start, kind }
        end
    end
    return spans
end

local workloads = {}

-- The number of lines: measures the cost of handing lines to the plugin.
function workloads.line_count(lines)
    return #lines
end

-- 0-based indices of the lines that match `query`, scored one byte at a time.
function workloads.fuzzy(lines, query)
    local matches = {}
    for i = 1, #lines do
        if fuzzy_score(lines[i], query) then
            matches[#matches + 1] = i - 1
        end
    end
    return matches
end

-- The same matches, found the idiomatic way: a pattern run by string.find.
function workloads.fuzzy_pattern(lines, query)
    local pattern = subsequence_pattern(query)
    local matches = {}
    for i = 1, #lines do
        if find(lower(lines[i]), pattern) then
            matches[#matches + 1] = i - 1
        end
    end
    return matches
end

-- The spans of every line, found with string patterns.
function workloads.highlight(lines)
    local result = {}
    for i = 1, #lines do
        result[i] = highlight_line(lines[i])
    end
    return result
end

-- The spans of every line, found one byte at a time.
function workloads.highlight_bytes(lines)
    local result = {}
    for i = 1, #lines do
        result[i] = highlight_line_bytes(lines[i])
    end
    return result
end

return workloads
