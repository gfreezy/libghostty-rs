# libghostty-rs

## Building

- Default build needs **no Zig**: `libghostty-vt` → `libghostty-vt-sys-vendored`
  downloads a prebuilt native lib. Building `libghostty-vt-sys` from source needs
  Zig 0.15.x on PATH (use the Nix dev shell). Offline: set `GHOSTTY_VT_PREBUILT_DIR`.
- Enter dev shell: `nix develop`
- Check: `cargo check`
- Test: `cargo test -p libghostty-vt-sys`
- Build example: `cargo build -p ghostling_rs`
- Run example: `cargo run -p ghostling_rs`

## Code Conventions

- Rust workspace: `libghostty-vt-sys` (FFI bindings, builds native lib from source via Zig), `libghostty-vt-sys-vendored` (same FFI, downloads a prebuilt native lib), `libghostty-vt` (safe wrappers; depends on vendored aliased as `libghostty-vt-sys`), `ghostling_rs` (example)
- Opaque pointer pattern: `NonNull<ffi::GhosttyFoo>` + `PhantomData<*mut ()>` + `Drop`
- Sized structs: set `size` field to `std::mem::size_of::<Type>()` before FFI calls
- `from_result()` maps `GhosttyResult` to `Result<(), Error>`
- Ghostty source is fetched at build time by `build.rs` (pinned commit). Override with `GHOSTTY_SOURCE_DIR` env var to use a local checkout.
- Comment heavily — explain *why*, not just *what*
