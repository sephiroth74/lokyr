use lokyr_plugin_sdk::command::Code;
use lokyr_plugin_sdk::fake::FakeHost;

use super::*;
use crate::{LuauPlugin, vms_made};

/// A package of `modules`, by their paths.
fn package(modules: &[(&str, &str)]) -> Package {
    Package::new(
        modules
            .iter()
            .map(|(path, source)| ((*path).to_owned(), (*source).to_owned())),
    )
}

/// Activates a plugin of `modules`, run as `execution` says: the plugin,
/// and its activation's result.
fn activated(
    modules: &[(&str, &str)],
    execution: Execution,
) -> (LuauPlugin, Result<(), lokyr_plugin_sdk::command::Error>) {
    let mut plugin = LuauPlugin::new("acme/test", package(modules), execution);
    let mut host = FakeHost::new("acme/test");
    let activated = host.activate(&mut plugin);
    (plugin, activated)
}

/// An entry whose `activate` computes, and fails of a wrong sum.
const SUMS: &str = r#"
local function sum(n: number): number
    local total = 0
    for i = 1, n do
        total += i
    end
    return total
end
return {
    activate = function()
        if sum(100000) ~= 5000050000 then
            error("a wrong sum")
        end
    end,
}
"#;

#[test]
fn a_plugin_activates_in_a_vm_of_the_native_code_generator_and_of_the_interpreter() {
    let (native, done) = activated(&[(ENTRY, SUMS)], Execution::Native);
    assert_eq!(done, Ok(()));
    assert_eq!(native.runs_native(), native_supported());
    let (interpreted, done) = activated(&[(ENTRY, SUMS)], Execution::Interpreter);
    assert_eq!(done, Ok(()));
    assert!(interpreted.has_vm());
    assert!(!interpreted.runs_native());
    // The Macs of Apple silicon have the generator.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert!(native_supported());
    }
}

#[test]
fn a_vm_is_made_as_its_plugin_first_activates() {
    let made = vms_made();
    let plugin = LuauPlugin::new("acme/test", package(&[(ENTRY, SUMS)]), Execution::Native);
    assert!(!plugin.has_vm());
    let (plugin, done) = activated(&[(ENTRY, SUMS)], Execution::Native);
    assert_eq!(done, Ok(()));
    assert!(plugin.has_vm());
    assert!(vms_made() > made);
}

#[test]
fn an_entry_or_an_activation_that_fails_tells_its_error() {
    let (_, done) = activated(
        &[(
            ENTRY,
            "return { activate = function() error(\"on purpose\") end }",
        )],
        Execution::Native,
    );
    let error = done.unwrap_err();
    assert_eq!(error.code, Code::Failed);
    assert!(error.message.contains("on purpose"), "{error}");
    assert!(error.message.contains("acme/test"), "{error}");
    let (_, done) = activated(&[(ENTRY, "return 42")], Execution::Native);
    assert!(
        done.unwrap_err()
            .message
            .contains("table of the plugin's callbacks")
    );
    let (_, done) = activated(&[], Execution::Native);
    assert!(done.unwrap_err().message.contains("holds no src/init.luau"));
    // An entry of no `activate` activates.
    let (_, done) = activated(&[(ENTRY, "return {}")], Execution::Native);
    assert_eq!(done, Ok(()));
}

#[test]
fn the_sandbox_holds_the_libraries_that_compute_alone() {
    // What a plugin reaches: nothing past the computation.
    let entry = r#"
local reached = {}
for _, name in { "os", "io", "debug", "package", "dofile", "loadfile", "load",
    "loadstring", "getfenv", "setfenv", "collectgarbage", "gcinfo", "print" } do
    if _G[name] ~= nil then
        table.insert(reached, name)
    end
end
for _, name in { "string", "table", "math", "bit32", "utf8", "buffer" } do
    if _G[name] == nil then
        error("no " .. name)
    end
end
return {
    activate = function()
        if #reached > 0 then
            error("reached " .. table.concat(reached, ", "))
        end
        -- The libraries are read-only.
        local wrote = pcall(function() string.lower = nil end)
        if wrote then
            error("wrote a library")
        end
    end,
}
"#;
    let (plugin, done) = activated(&[(ENTRY, entry)], Execution::Native);
    assert_eq!(done, Ok(()));
    let globals = plugin.vm.as_ref().unwrap().lua().globals();
    for name in TAKEN
        .iter()
        .chain(&["os", "io", "debug", "package", "load", "dofile"])
    {
        assert!(globals.raw_get::<Value>(*name).unwrap().is_nil(), "{name}");
    }
}

#[test]
fn require_takes_the_package_s_modules_alone() {
    let modules = [
        (
            ENTRY,
            r#"
local util = require("./util")
local strings = require("./text")
local again = require("./util")
assert(util == again, "a module runs once")
return {
    activate = function()
        if util.double(21) ~= 42 or strings.shout("hi") ~= "HI!" then
            error("wrong modules")
        end
    end,
}
"#,
        ),
        (
            "src/util.luau",
            "return { double = function(n) return n * 2 end }",
        ),
        (
            "src/text/init.luau",
            "local up = require(\"./upper\")\nreturn { shout = function(s) return up(s) .. \"!\" end }",
        ),
        ("src/text/upper.luau", "return string.upper"),
    ];
    let (_, done) = activated(&modules, Execution::Native);
    assert_eq!(done, Ok(()));
    // Out of the package, of no `./`, missing, or of a cycle.
    let refused = |name: &str| {
        let entry = format!("return require(\"{name}\")");
        let modules = [
            (ENTRY, entry.as_str()),
            ("src/a.luau", "return require(\"./b\")"),
            ("src/b.luau", "return require(\"./a\")"),
        ];
        activated(&modules, Execution::Native)
            .1
            .unwrap_err()
            .message
    };
    assert!(refused("../../etc/passwd").contains("leaves the package"));
    assert!(refused("util").contains("of a path from `./` or `../`"));
    assert!(refused("@lokyr/x").contains("of a path from `./` or `../`"));
    assert!(refused("./missing").contains("holds no module `./missing`"));
    assert!(refused("./a").contains("requires itself"));
    // A module above its directory, within the package.
    let package = package(&[("src/a/b.luau", ""), ("lib/c.luau", "")]);
    assert_eq!(
        resolve("src/a/b.luau", "../../lib/c", &package),
        Ok("lib/c.luau".to_owned())
    );
    assert!(resolve("src/a/b.luau", "../../../c", &package).is_err());
}

/// Short deadlines, which the tests' endless loops meet soon.
fn short() -> Deadlines {
    Deadlines {
        activate: std::time::Duration::from_millis(80),
        command: std::time::Duration::from_millis(40),
        render: std::time::Duration::from_millis(20),
    }
}

/// The VM of `modules`, run as `execution` says, within short deadlines.
fn vm(modules: &[(&str, &str)], execution: Execution) -> Result<Vm, Failure> {
    Vm::new(&Arc::new(package(modules)), "acme/test", execution, short())
}

#[test]
fn an_endless_loop_stops_at_its_deadline_and_its_plugin_faults() {
    for execution in [Execution::Native, Execution::Interpreter] {
        // In `activate`, as the plugin tells it.
        let looping = "return { activate = function() while true do end end }";
        let mut plugin = LuauPlugin::new("acme/test", package(&[(ENTRY, looping)]), execution)
            .with_deadlines(short());
        let start = std::time::Instant::now();
        let error = FakeHost::new("acme/test")
            .activate(&mut plugin)
            .unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(error.code, Code::Fault);
        assert!(
            error
                .message
                .contains("past its deadline of 80 ms in `activate`"),
            "{error}"
        );
        // At the entry's top, of a function's call at each step.
        let calling = "local function f() return 1 end\nwhile true do f() end";
        assert!(matches!(
            vm(&[(ENTRY, calling)], execution),
            Err(Failure::PastDeadline)
        ));
        // In a render, of its own deadline; the VM runs the next call.
        let rendering = r#"
local n = 0
return {
    render = function()
        n += 1
        if n == 1 then
            while true do end
        end
    end,
}
"#;
        let made = vm(&[(ENTRY, rendering)], execution).unwrap();
        let start = std::time::Instant::now();
        assert!(matches!(
            made.call(Callback::Render),
            Err(Failure::PastDeadline)
        ));
        assert!(start.elapsed() >= std::time::Duration::from_millis(20));
        assert!(made.call(Callback::Render).is_ok());
        assert!(made.call(Callback::Render).is_ok());
    }
}

#[test]
fn a_vm_runs_out_of_its_memory_and_runs_on() {
    let bomb = r#"
local kept = {}
return {
    activate = function()
        while true do
            -- A buffer each, as strings of the same text are one.
            table.insert(kept, buffer.create(1024 * 1024))
        end
    end,
    render = function()
        kept = {}
    end,
}
"#;
    let made = vm(&[(ENTRY, bomb)], Execution::Native).unwrap();
    let failed = made.call(Callback::Activate);
    assert!(matches!(failed, Err(Failure::OutOfMemory)), "{failed:?}");
    // Its memory freed, it runs on.
    assert!(made.call(Callback::Render).is_ok());
    let mut plugin = LuauPlugin::new("acme/test", package(&[(ENTRY, bomb)]), Execution::Native);
    let error = FakeHost::new("acme/test")
        .activate(&mut plugin)
        .unwrap_err();
    assert_eq!(error.code, Code::Fault);
    assert!(
        error.message.contains("ran out of its 64 MB of memory"),
        "{error}"
    );
}

#[test]
fn a_call_within_its_deadline_is_left_alone() {
    let quick = r#"
return {
    render = function()
        local total = 0
        for i = 1, 1000 do
            total += i
        end
    end,
}
"#;
    let made = vm(&[(ENTRY, quick)], Execution::Native).unwrap();
    for _ in 0..200 {
        assert!(made.call(Callback::Render).is_ok());
    }
}

#[test]
fn the_watchdog_sleeps_while_no_call_runs() {
    let quick = "return { render = function() end }";
    let made = vm(&[(ENTRY, quick)], Execution::Native).unwrap();
    for _ in 0..10 {
        assert!(made.call(Callback::Render).is_ok());
    }
    // Its last wakes, of the last call's end, settle; then none.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let woke = made.watchdog_wakes();
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(made.watchdog_wakes(), woke);
}
