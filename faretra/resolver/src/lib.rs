//! Version resolution for Faretra.
//!
//! Picks one version of each plugin that satisfies every semver requirement,
//! including the host API version required by the installed app, and explains
//! conflicts in readable terms.

#![forbid(unsafe_code)]
