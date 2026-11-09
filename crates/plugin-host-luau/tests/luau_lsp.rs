//! luau-lsp's analysis of plugins against the API's types, `lokyr.d.luau`:
//! the template's plugin of no error, and one of wrong fields whose errors
//! it tells. luau-lsp is the one that `LUAU_LSP` names, which `just
//! luau-lsp` finds, as no formula installs it: ignored without it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The crate's folder of Luau's files.
fn luau() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("luau")
}

/// luau-lsp's analysis of `file`, a path in a copy of the template that
/// holds it, and of the definitions: whether it found no error, and what
/// it told.
fn analyzed(file: &str, source: &str) -> (bool, String) {
    let lsp = std::env::var("LUAU_LSP").expect("LUAU_LSP names luau-lsp");
    let folder = std::env::temp_dir().join(format!("lokyr-luau-lsp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("src")).unwrap();
    let template = luau().join("template");
    std::fs::copy(template.join(".luaurc"), folder.join(".luaurc")).unwrap();
    std::fs::copy(luau().join("lokyr.d.luau"), folder.join("lokyr.d.luau")).unwrap();
    std::fs::write(folder.join(file), source).unwrap();
    let output = Command::new(lsp)
        .current_dir(&folder)
        .args([
            "analyze",
            "--platform",
            "standard",
            "--definitions",
            "@lokyr=lokyr.d.luau",
            "--base-luaurc",
            ".luaurc",
            file,
        ])
        .output()
        .expect("luau-lsp runs");
    let _ = std::fs::remove_dir_all(&folder);
    let told = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), told)
}

#[test]
#[ignore = "needs luau-lsp, which `just luau-lsp` finds"]
fn the_template_s_plugin_is_of_no_error_and_a_plugin_s_wrong_fields_are_told() {
    let template = std::fs::read_to_string(luau().join("template/src/init.luau")).unwrap();
    let (clean, told) = analyzed("src/init.luau", &template);
    assert!(clean && !told.contains("Error"), "{told}");
    let wrong = std::fs::read_to_string(luau().join("checks/wrong.luau")).unwrap();
    let (clean, told) = analyzed("src/init.luau", &wrong);
    assert!(!clean, "{told}");
    for error in [
        "missing field 'title'",
        "Key 'tabb' not found",
        "Key 'txt' not found in table 'LineText'",
        "\"textt\"",
    ] {
        assert!(told.contains(error), "{error}: {told}");
    }
}
