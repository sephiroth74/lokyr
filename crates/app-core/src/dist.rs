//! The first-party plugins of the app's distributions, which `cargo xtask
//! gen-dist` writes of `dist/*.toml`: each distribution is a feature of
//! the crate, `dist-<name>`, of a feature of each of its plugins,
//! `plugin-<name>`; a build of several takes the plugins of each.

use lokyr_kernel::plugins::Registration;

/// The registrations of the build's plugins, in the order of their
/// distributions' files, which the app adds and activates in it.
pub(crate) fn plugins() -> Vec<Registration> {
    vec![
        #[cfg(all(feature = "plugin-keymap-macos", target_os = "macos"))]
        lokyr_keymap_macos::registration(),
        #[cfg(feature = "plugin-core-actions")]
        lokyr_core_actions::registration(),
        #[cfg(feature = "plugin-links")]
        lokyr_links::registration(),
    ]
}
