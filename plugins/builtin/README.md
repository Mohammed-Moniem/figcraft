# Bundled example modules

These six WebAssembly modules are compiled from this repository's Rust examples in plugins/examples using its local SDK. They are MIT OR Apache-2.0, as specified by the source workspace. No external plugin binaries are downloaded. PROVENANCE.json records exact compilation inputs and each module's SHA-256, byte count, manifest version and ABI imports/exports.

Rebuild from the source root with the private target directory configured:

    cargo build --locked --offline --manifest-path plugins/Cargo.toml --target wasm32-unknown-unknown --release --workspace

Copy the Counter, Dot grid, Export names, Focus list, Swatch grid and Tidy names output modules into this directory. Counter retains its API 5 panel and API 6 Line/SVG support; its version is 1.3.0. Focus list and Dot grid request API 6, so the existing Line/SVG capability gate is preserved. Source and isolated build receipts accompany the local fork; integration tests exercise the actual modules in the engine sandbox separately from SDK/model unit tests.
