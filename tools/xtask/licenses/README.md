# The licenses' texts

The texts of the licenses that `deny.toml` allows, and of the LLVM exception, as SPDX's [license list](https://github.com/spdx/license-list-data) gives them, of its release v3.29.0, `text/<id>.txt`, unchanged. `cargo xtask bundle` takes them for a crate whose package carries no file of its license, its copyright's line made of its authors, so that every crate of the binary has its notice in the bundle's `Contents/Resources/licenses/crates`. A license that `deny.toml` comes to allow needs its text here; a test fails without it.
