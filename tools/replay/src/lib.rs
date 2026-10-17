//! What `bolt-replay`, `bolt-record` and their tests share: recordings, as
//! they are read, the screen, as it is printed, and a headless terminal
//! that runs a program.

pub mod harness;
pub mod recording;
pub mod screen;
