//! The links of the app's command line tools, which the platform makes in
//! a folder of the shells' `PATH`, as another terminal and the scripts find
//! them there.

use std::path::PathBuf;

/// A link that [`Platform::link_tools`](crate::Platform::link_tools) makes:
/// a symbolic link at `link` to `target`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolLink {
    /// The link's path, as `/usr/local/bin/sleip`.
    pub link: PathBuf,
    /// The file that it points to, as the bundle's binary.
    pub target: PathBuf,
}

/// How [`Platform::link_tools`](crate::Platform::link_tools) ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkOutcome {
    /// The links are there.
    Linked,
    /// The user declined the system's question of an administrator's
    /// password.
    Cancelled,
    /// They could not be made, of the reason, as the system told it.
    Failed(String),
}
