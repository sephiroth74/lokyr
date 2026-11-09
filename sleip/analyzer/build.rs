//! Builds Luau's type checker, the `Analysis` library that `luau/` holds,
//! and the shim that Sleip calls, against the headers of Luau's other
//! libraries, of the defines that `luau0-src` builds them of, whose code
//! `mlua` links.

use std::path::Path;

fn main() {
    let luau = Path::new("luau");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .warnings(false)
        .define("LUAI_MAXCSTACK", "1000000")
        .define("LUA_VECTOR_SIZE", "3")
        .define("LUA_API", "extern \"C\"")
        .include(luau.join("Analysis/include"))
        .include(luau.join("Analysis/src"));
    for library in ["Ast", "Common", "Config", "Compiler", "Bytecode", "VM"] {
        build.include(luau.join(library).join("include"));
    }
    // As `luau0-src` builds Luau's other libraries.
    if cfg!(debug_assertions) {
        build.define("LUAU_ENABLE_ASSERT", None);
    } else {
        build.flag_if_supported("-fno-math-errno");
    }
    let mut sources: Vec<_> = std::fs::read_dir(luau.join("Analysis/src"))
        .expect("the Analysis sources")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "cpp"))
        .collect();
    sources.sort();
    build.files(&sources).file("src/shim.cpp");
    build.compile("sleipanalysis");
    println!("cargo:rerun-if-changed=luau");
    println!("cargo:rerun-if-changed=src/shim.cpp");
}
