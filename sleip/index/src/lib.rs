//! Registry index client for Sleip.
//!
//! Fetches entries from the sparse index, downloads package archives, and
//! verifies their checksums and signatures before anything is extracted.

#![forbid(unsafe_code)]
