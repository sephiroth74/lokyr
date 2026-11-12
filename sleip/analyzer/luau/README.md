# Luau's type checker, vendored

The type checker of [Luau](https://github.com/luau-lang/luau) 0.740, its
`Analysis` library, which `sleip-analyzer` builds for `sleip check`, and the
headers of the libraries that it uses, `Ast`, `Common`, `Config`,
`Compiler`, `Bytecode` and `VM`, whose code it links of the `luau0-src`
that `mlua` builds, of the same version, so that one Luau holds them in a
binary.

- Taken from the tag `0.740`'s source archive,
  `https://github.com/luau-lang/luau/archive/refs/tags/0.740.tar.gz`, of the
  SHA-256 `419f96e4ecf5a0dd415a2625ea7ea9dc8a407e86d64b25e00936302b25a7afbd`,
  as it was, the other libraries' sources, the tests and the tools left out.
- The headers of `Ast`, `Common`, `Config`, `Compiler`, `Bytecode` and
  `VM` are those of `luau0-src` 0.22.0+luau740, byte for byte.
- Luau's license, `LICENSE.txt`, MIT; Lua's, `lua_LICENSE.txt`, of the VM's
  headers.

A new Luau comes with `mlua`'s: the `Analysis` folder and the headers are
taken anew of the same tag.
