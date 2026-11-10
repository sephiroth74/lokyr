//! Registry index client for Sleip.
//!
//! Reads the registry's repository, a file of each plugin of its published
//! versions; fetches entries from the sparse index, downloads package
//! archives, and verifies their checksums and signatures before anything
//! is extracted.

#![forbid(unsafe_code)]

pub mod registry;
pub mod signing;
