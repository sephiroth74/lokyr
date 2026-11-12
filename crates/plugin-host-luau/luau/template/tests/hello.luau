--!strict
-- The plugin's tests, which `sleip test` runs on a host of the tests: each
-- function of the table a test, of a host of its own, which fails at its
-- first error, as `assert` raises it.

local test = {}

function test.the_command_shows_its_notice_which_goes_by_itself(host: TestHost)
    host.activate()
    host.command("hello.say")
    assert(host.panels()[1] == "notice", "the notice shows")
    local notice = host.render({ panel = "notice" })
    assert(notice ~= nil and notice.role == "alert", "the notice renders")
    host.advance(3000)
    assert(#host.panels() == 0, "the notice goes after three seconds")
end

return test
