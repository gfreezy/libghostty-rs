# libghostty-rs

Rust bindings and safe API for [libghostty-vt](https://ghostty.org), the virtual terminal emulator library extracted from [Ghostty](https://ghostty.org).

## Workspace Layout

- `crates/libghostty-vt-sys` — raw FFI bindings generated from `ghostty/vt.h`
- `crates/libghostty-vt` — safe Rust wrappers (Terminal, RenderState, KeyEncoder, MouseEncoder, etc.)
- `example/grid_ref_tracked_rs` — focused example of tracked grid references following cells through scrollback and reset
- `example/ghostling_rs` — Rust port of [ghostling](https://github.com/ghostty-org/ghostling), a minimal terminal emulator using [macroquad](https://macroquad.rs)

## Quick Start

```rust
use libghostty_vt::{Terminal, TerminalOptions, RenderState};
use libghostty_vt::render::{RowIterator, CellIterator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a terminal with 80 columns, 24 rows, and scrollback.
    let mut terminal = Terminal::new(TerminalOptions {
        cols: 80,
        rows: 24,
        max_scrollback: 10_000,
    })?;

    // Register an effect handler for PTY write-back (e.g. query responses).
    terminal.on_pty_write(|_term, data| {
        println!("PTY response: {} bytes", data.len());
    })?;

    // Feed VT-encoded data into the terminal.
    terminal.vt_write(b"Hello, \x1b[1;32mworld\x1b[0m!\r\n");
    terminal.vt_write(b"\x1b[38;2;255;128;0morange text\x1b[0m\r\n");

    // Capture a render snapshot and iterate rows/cells.
    let mut render_state = RenderState::new()?;
    let mut rows = RowIterator::new()?;
    let mut cells = CellIterator::new()?;

    let snapshot = render_state.update(&terminal)?;
    let mut row_iter = rows.update(&snapshot)?;

    while let Some(row) = row_iter.next() {
        let mut cell_iter = cells.update(row)?;
        while let Some(cell) = cell_iter.next() {
            let graphemes: Vec<char> = cell.graphemes()?;
            print!("{graphemes:?}");
        }
        println!();
    }

    Ok(())
}
```

## Building

**No Zig toolchain required by default.** `libghostty-vt` depends on
[`libghostty-vt-sys-vendored`](crates/libghostty-vt-sys-vendored), which at build
time **downloads** a prebuilt `libghostty-vt` static library (and the matching
`bindings.rs`) for your target from a pinned GitHub release, verifies it against
a checked-in `SHA256SUMS`, and caches it. The dependency is aliased to
`libghostty-vt-sys`, so `use libghostty_vt_sys` paths are unchanged.

If a prebuilt artifact cannot be obtained (unsupported target, no network, or a
target with no published artifact) the build **fails with a clear error** — there
is no Zig source-build fallback. Two escape hatches:

- **Use a locally built library** — set `GHOSTTY_VT_PREBUILT_DIR` to a directory
  containing the native library (in it or a `lib/` subdir) and optionally a
  `bindings.rs`. To produce one with Zig:

  ```sh
  cargo build -p libghostty-vt-sys   # builds libghostty-vt.a via Zig
  export GHOSTTY_VT_PREBUILT_DIR=$(dirname "$(find target -path '*ghostty-install/lib/libghostty-vt.a')")
  cargo build -p libghostty-vt
  ```

- **Build from source** — depend on
  [`libghostty-vt-sys`](crates/libghostty-vt-sys) directly. It builds the native
  library from Ghostty sources with [Zig](https://ziglang.org/) 0.15.x and is the
  crate the release artifacts are produced from.

### Building libghostty-vt-sys from source (Zig)

`libghostty-vt-sys` requires Zig 0.15.x on PATH. By default the ghostty source is
fetched automatically at build time from the pinned commit in `build.rs`. Set
`GHOSTTY_SOURCE_DIR` to make the build use a local Ghostty checkout instead.
Package managers that need network-free builds can also set
`GHOSTTY_ZIG_SYSTEM_DIR` to a pre-fetched Zig package directory; this is passed
to `zig build --system` so Zig does not download package dependencies during
the Cargo build script.

Vendored builds derive Zig's optimize mode from Cargo's profile: dev builds use
`Debug`, size-optimized builds use `ReleaseSmall`, and other release builds use
`ReleaseFast`. Set `LIBGHOSTTY_VT_SYS_OPTIMIZE` to `Debug`, `ReleaseSafe`,
`ReleaseFast`, or `ReleaseSmall` to override that choice explicitly.

The `pkg-config` path is opt-in. If you enable `libghostty-vt-sys/pkg-config`,
the build will prefer an installed `libghostty-vt` discovered through
`pkg-config` when `GHOSTTY_SOURCE_DIR` is unset. libghostty-vt is pre-1.0, so
the checked-in bindings are expected to move with the pinned Ghostty source and
do not guarantee compatibility with arbitrary installed C API revisions. An
explicit `GHOSTTY_SOURCE_DIR` always wins.

Nix builds in this repository prefetch the pinned Ghostty source and Ghostty's
Zig package dependencies up front. The flake builds the static library from
source in a separate derivation and sets `GHOSTTY_VT_PREBUILT_DIR` (so the
vendored crate links it instead of downloading), plus `GHOSTTY_SOURCE_DIR` and
`GHOSTTY_ZIG_SYSTEM_DIR` for the `libghostty-vt-sys` member. Downstream Nix
packaging should use the same contract rather than allowing network access in
the sandbox.

By default the native library is linked statically (`libghostty-vt.a`). This
statically links the Ghostty VT archive, but the final binary may still depend
on platform runtime libraries. To link the shared library instead, enable
`libghostty-vt/link-dynamic` (requires a published dynamic prebuilt artifact, or
a `GHOSTTY_VT_PREBUILT_DIR` containing the shared library).

### Releasing prebuilt artifacts

`.github/workflows/release.yml` builds the static library for every supported
target plus `bindings.rs`, and publishes them to a `vt-prebuilt-v*` GitHub
release with a `SHA256SUMS` manifest. To cut a release:

1. Bump `PREBUILT_TAG` in `crates/libghostty-vt-sys-vendored/build.rs` to the new
   tag and commit it.
2. Push the matching tag (e.g. `vt-prebuilt-v0.1.1`). The workflow builds and
   uploads all artifacts.
3. Merge the auto-opened PR that syncs `SHA256SUMS` and the checked-in
   `bindings.rs` fallbacks, so downloads verify against the published hashes.

```sh
nix develop
cargo check
cargo test -p libghostty-vt-sys
cargo build -p ghostling_rs
```

### Running the example

```sh
cargo run -p ghostling_rs
cargo run -p grid_ref_tracked_rs
```

When building with `link-dynamic`, set `LD_LIBRARY_PATH` on Linux or
`DYLD_LIBRARY_PATH` on macOS to the directory containing the generated
`libghostty-vt` shared library.
