# Decisions

Decisions taken without the maintainer, with their reasons (WORK-6 in [RULES.md](RULES.md)).

## Fuzz targets compile `src/` modules by path

favicons is a single binary crate with no library target, and turning it into a library would make every `pub` item public API that `missing_docs` then requires documenting. The fuzz workspace instead compiles the decoder modules directly through `#[path]` declarations in `fuzz/fuzz_targets/modules.rs`: `sandbox`, `discover`, `render`, `vector`, `palette`, `slugs`, `policy`, `domain` and what they need. The server-side modules (`app`, `http`, `store`, `age`) stay out, because `store` pulls in `object_store`, whose `crc-fast` dependency builds a `cdylib` that cannot link with coverage instrumentation. `fuzz/Cargo.toml` repeats the dependencies those modules use; a new dependency of a fuzzed module goes there too.

## Toolchain pinned to 1.99

`rust-toolchain.toml` and `mise.toml` pin Rust 1.99 like the other repositories of the workspace (WORK-2). `rust-version` in `Cargo.toml` stays 1.88: it is the minimum for crates.io users, not the development toolchain.
