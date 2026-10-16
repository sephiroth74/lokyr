//! What the program inherits of this process's environment. A test binary
//! of its own, with one test, since the test changes the environment.

use std::io::Read;

use saetta_pty::{Options, Program, spawn};

#[test]
fn the_variables_of_another_terminal_do_not_reach_the_program() {
    // SAFETY: this binary runs this one test, so no other thread reads the
    // environment while it changes.
    unsafe {
        std::env::set_var("TERMINFO", "/opt/other/terminfo");
        std::env::set_var("COLUMNS", "132");
        std::env::set_var("SAETTA_RESOURCES_DIR", "/stale");
        std::env::set_var("SAETTA_TEST_INHERITED", "kept");
    }
    let script = r#"echo "[$TERMINFO][$COLUMNS][$SAETTA_RESOURCES_DIR][$SAETTA_TEST_INHERITED]""#;
    let pty = spawn(&Options::new(
        Program::command("/bin/sh", ["-c", script]),
        "xterm-256color",
    ))
    .unwrap();
    let mut text = String::new();
    pty.reader().unwrap().read_to_string(&mut text).unwrap();
    assert_eq!(text, "[][][][kept]\r\n");
}
