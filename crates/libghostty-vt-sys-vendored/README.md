# libghostty-vt-sys-vendored

Prebuilt FFI bindings for `libghostty-vt`. This crate is a drop-in replacement
for [`libghostty-vt-sys`](../libghostty-vt-sys) that **downloads** a prebuilt
native library instead of building it from source with Zig, so downstream users
do not need a Zig toolchain.

It exposes exactly the same public API as `libghostty-vt-sys`. `libghostty-vt`
depends on it under the alias `libghostty-vt-sys`, so `use libghostty_vt_sys`
paths are unchanged.

## How it resolves the native library

The build script (`build.rs`) picks a source in this order:

1. **docs.rs** (`DOCS_RS` set): uses the checked-in `src/bindings.rs` fallback
   and emits no linking. Documentation builds need no native library.
2. **`GHOSTTY_VT_PREBUILT_DIR=<dir>`**: takes the native library from `<dir>` or
   `<dir>/lib`, and `bindings.rs` from `<dir>` if present (otherwise the
   checked-in fallback). This is the offline / Nix / from-source escape hatch.
3. **Download** (default): fetches the artifact for the current target and
   `bindings.rs` from the pinned GitHub release (`vt-prebuilt-*` tag), verifies
   them against the checked-in `SHA256SUMS`, and caches them.

If none of these can produce a library — unsupported target, download failure,
or a target with no published artifact — the build **fails with a clear error**.
There is intentionally no Zig source-build fallback. To build from source, set
`GHOSTTY_VT_PREBUILT_DIR` to a local checkout's artifacts or depend on
`libghostty-vt-sys` directly.

## Environment variables

- `GHOSTTY_VT_PREBUILT_DIR` — directory holding a prebuilt library (and
  optionally `bindings.rs`). Bypasses downloading.
- `GHOSTTY_VT_PREBUILT_CACHE` — override the download cache directory
  (default: `$CARGO_HOME/libghostty-vt-prebuilt/<tag>`).

## Features

- `link-dynamic` — link the shared library instead of the static archive
  (requires a published dynamic artifact for the target).
- `kitty-graphics` — no-op, kept for parity with `libghostty-vt-sys` so feature
  forwarding from `libghostty-vt` compiles.

## Maintainer note

`src/bindings.rs` and `SHA256SUMS` are generated and updated by
`.github/workflows/release.yml`. Do not edit them by hand. When cutting a
release, bump `PREBUILT_TAG` in `build.rs` to match.
