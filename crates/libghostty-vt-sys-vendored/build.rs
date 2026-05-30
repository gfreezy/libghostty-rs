//! Build script for `libghostty-vt-sys-vendored`.
//!
//! Unlike `libghostty-vt-sys`, this crate does not invoke Zig. Instead it
//! obtains a prebuilt `libghostty-vt` native library (plus the matching
//! `bindings.rs`) for the current target and links against it:
//!
//! 1. On docs.rs there is no network, so the checked-in `src/bindings.rs`
//!    fallback is used and no linking is emitted.
//! 2. If `GHOSTTY_VT_PREBUILT_DIR` is set, the library and (optionally)
//!    `bindings.rs` are taken from that directory. This is the offline / Nix /
//!    from-source escape hatch.
//! 3. Otherwise the artifact is downloaded from the pinned GitHub release,
//!    verified against the checked-in `SHA256SUMS`, and cached.
//!
//! Any failure is a hard error — there is intentionally no Zig source-build
//! fallback. The error message points users at `GHOSTTY_VT_PREBUILT_DIR` or the
//! source-building `libghostty-vt-sys` crate.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Release tag the prebuilt artifacts are pulled from. Bump together with the
/// checked-in `SHA256SUMS` and `src/bindings.rs` whenever a new release is cut.
const PREBUILT_TAG: &str = "vt-prebuilt-v0.1.1";

/// Base URL for GitHub release downloads. Must point at the repository that the
/// `release.yml` workflow publishes the `vt-prebuilt-v*` releases to.
const RELEASE_BASE: &str = "https://github.com/gfreezy/libghostty-rs/releases/download";

/// `sha256sum`-format manifest of expected artifact hashes, kept in sync with
/// the release by the release workflow.
const SHA256SUMS: &str = include_str!("SHA256SUMS");

#[derive(Clone, Copy)]
enum LinkMode {
    Static,
    Dynamic,
}

impl LinkMode {
    fn current() -> Self {
        if cfg!(feature = "link-dynamic") {
            Self::Dynamic
        } else {
            Self::Static
        }
    }

    fn link_kind(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dynamic => "dylib",
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTTY_VT_PREBUILT_DIR");
    println!("cargo:rerun-if-env-changed=GHOSTTY_VT_PREBUILT_CACHE");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=SHA256SUMS");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));

    // docs.rs has no network and never links: use the checked-in bindings only.
    if env::var_os("DOCS_RS").is_some() {
        copy_fallback_bindings(&out_dir);
        return;
    }

    let target = env::var("TARGET").expect("TARGET must be set");
    let link_mode = LinkMode::current();

    // Local override: take the library (and optionally bindings) from a dir.
    if let Some(dir) = env::var_os("GHOSTTY_VT_PREBUILT_DIR") {
        use_local_dir(&PathBuf::from(dir), &out_dir, &target, link_mode);
        return;
    }

    download_and_link(&out_dir, &target, link_mode);
}

/// Copy the checked-in `src/bindings.rs` fallback into `OUT_DIR/bindings.rs`.
fn copy_fallback_bindings(out_dir: &Path) {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let fallback = manifest_dir.join("src").join("bindings.rs");
    fs::copy(&fallback, out_dir.join("bindings.rs")).unwrap_or_else(|e| {
        panic!(
            "failed to copy fallback bindings {} -> OUT_DIR: {e}",
            fallback.display()
        )
    });
}

/// Use a library + bindings from a local directory (`GHOSTTY_VT_PREBUILT_DIR`).
///
/// The directory must contain the native library (either directly or under a
/// `lib/` subdirectory). If it also contains `bindings.rs` that is used,
/// otherwise the checked-in fallback bindings are used.
fn use_local_dir(dir: &Path, out_dir: &Path, target: &str, link_mode: LinkMode) {
    assert!(
        dir.is_dir(),
        "GHOSTTY_VT_PREBUILT_DIR is not a directory: {}",
        dir.display()
    );

    let source_lib = find_local_library(dir, target, link_mode).unwrap_or_else(|| {
        panic!(
            "no {} libghostty-vt library found in {} (searched it and its lib/ subdir)",
            link_mode.link_kind(),
            dir.display()
        )
    });
    install_library(&source_lib, out_dir, target, link_mode);

    let local_bindings = dir.join("bindings.rs");
    if local_bindings.exists() {
        fs::copy(&local_bindings, out_dir.join("bindings.rs")).unwrap_or_else(|e| {
            panic!(
                "failed to copy {} -> OUT_DIR: {e}",
                local_bindings.display()
            )
        });
    } else {
        copy_fallback_bindings(out_dir);
    }
}

/// Download the prebuilt artifact + bindings from the pinned release, verify
/// their checksums, cache them, and emit the link directives.
fn download_and_link(out_dir: &Path, target: &str, link_mode: LinkMode) {
    let lib_asset = asset_name(target, link_mode);
    let cache = cache_dir();

    // Library.
    let cached_lib = fetch_to_cache(&cache, &lib_asset);
    install_library(&cached_lib, out_dir, target, link_mode);

    // Bindings (target independent).
    let cached_bindings = fetch_to_cache(&cache, "bindings.rs");
    fs::copy(&cached_bindings, out_dir.join("bindings.rs"))
        .unwrap_or_else(|e| panic!("failed to copy cached bindings -> OUT_DIR: {e}"));
}

/// Ensure `name` exists in the cache dir with a verified checksum, downloading
/// it from the release if necessary. Returns the cached file path.
fn fetch_to_cache(cache: &Path, name: &str) -> PathBuf {
    let expected = expected_sha256(name);
    let dest = cache.join(name);

    if dest.exists() && file_sha256(&dest) == expected {
        return dest;
    }

    let url = format!("{RELEASE_BASE}/{PREBUILT_TAG}/{name}");
    let bytes = http_get(&url);

    let actual = bytes_sha256(&bytes);
    assert!(
        actual == expected,
        "checksum mismatch for {name}\n  expected {expected}\n  actual   {actual}\n  from {url}"
    );

    fs::create_dir_all(cache)
        .unwrap_or_else(|e| panic!("failed to create cache dir {}: {e}", cache.display()));
    // Write to a temp file then rename for atomicity against concurrent builds.
    let tmp = cache.join(format!("{name}.tmp-{}", std::process::id()));
    fs::write(&tmp, &bytes).unwrap_or_else(|e| panic!("failed to write {}: {e}", tmp.display()));
    fs::rename(&tmp, &dest)
        .unwrap_or_else(|e| panic!("failed to finalize cache file {}: {e}", dest.display()));
    dest
}

/// Copy a resolved library file into `OUT_DIR/lib/<normalized>` and emit the
/// link search path and link directive.
fn install_library(source: &Path, out_dir: &Path, target: &str, link_mode: LinkMode) {
    let lib_dir = out_dir.join("lib");
    fs::create_dir_all(&lib_dir)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", lib_dir.display()));

    let dest = lib_dir.join(linker_file_name(target, link_mode));
    let _ = fs::remove_file(&dest);
    fs::copy(source, &dest).unwrap_or_else(|e| {
        panic!(
            "failed to install library {} -> {}: {e}",
            source.display(),
            dest.display()
        )
    });

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib={}=ghostty-vt", link_mode.link_kind());
}

/// The release asset name for a (target, link mode) pair.
fn asset_name(target: &str, link_mode: LinkMode) -> String {
    assert_supported(target);
    let ext = match link_mode {
        LinkMode::Static => {
            if target.contains("windows") && target.contains("msvc") {
                "lib"
            } else {
                "a"
            }
        }
        LinkMode::Dynamic => {
            if target.contains("darwin") {
                "dylib"
            } else if target.contains("windows") {
                panic!(
                    "dynamic linking of prebuilt libghostty-vt is not yet supported on Windows \
                     (target {target}); use the default static linking, or set \
                     GHOSTTY_VT_PREBUILT_DIR"
                )
            } else {
                "so"
            }
        }
    };
    let mode = match link_mode {
        LinkMode::Static => "static",
        LinkMode::Dynamic => "dynamic",
    };
    format!("libghostty-vt-{target}-{mode}.{ext}")
}

/// The file name the linker expects in the search dir for the given target.
fn linker_file_name(target: &str, link_mode: LinkMode) -> String {
    let msvc = target.contains("windows") && target.contains("msvc");
    match link_mode {
        LinkMode::Static => {
            if msvc {
                "ghostty-vt.lib".to_owned()
            } else {
                "libghostty-vt.a".to_owned()
            }
        }
        LinkMode::Dynamic => {
            if target.contains("darwin") {
                "libghostty-vt.dylib".to_owned()
            } else {
                "libghostty-vt.so".to_owned()
            }
        }
    }
}

/// Find a local library file matching the link mode in `dir` or `dir/lib`.
fn find_local_library(dir: &Path, target: &str, link_mode: LinkMode) -> Option<PathBuf> {
    let candidates = [dir.to_path_buf(), dir.join("lib")];
    for cand in candidates {
        let Ok(entries) = fs::read_dir(&cand) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if matches_library(target, link_mode, name) {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Whether `file_name` looks like the libghostty-vt artifact for the link mode.
/// Mirrors the matcher in `libghostty-vt-sys/build.rs`.
fn matches_library(target: &str, link_mode: LinkMode, file_name: &str) -> bool {
    match link_mode {
        LinkMode::Dynamic => {
            if target.contains("darwin") {
                file_name.starts_with("libghostty-vt") && file_name.ends_with(".dylib")
            } else if target.contains("windows") {
                file_name == "ghostty-vt.dll"
                    || file_name == "libghostty-vt.dll.lib"
                    || file_name == "libghostty-vt.dll.a"
            } else {
                file_name == "libghostty-vt.so" || file_name.starts_with("libghostty-vt.so.")
            }
        }
        LinkMode::Static => {
            if target.contains("windows") && target.contains("msvc") {
                file_name == "ghostty-vt.lib" || file_name == "ghostty-vt-static.lib"
            } else {
                file_name == "libghostty-vt.a"
            }
        }
    }
}

/// Targets we publish prebuilt artifacts for (mirrors `zig_target` in the sys
/// crate's build script).
fn assert_supported(target: &str) {
    const SUPPORTED: &[&str] = &[
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
    ];
    assert!(
        SUPPORTED.contains(&target),
        "no prebuilt libghostty-vt artifact for target '{target}'.\n\
         Set GHOSTTY_VT_PREBUILT_DIR to a directory containing a locally built \
         library + bindings.rs, or depend on the source-building `libghostty-vt-sys` \
         crate (which requires the Zig toolchain)."
    );
}

/// Cache directory for downloaded artifacts: `$GHOSTTY_VT_PREBUILT_CACHE` or
/// `$CARGO_HOME/libghostty-vt-prebuilt/<tag>`.
fn cache_dir() -> PathBuf {
    if let Some(dir) = env::var_os("GHOSTTY_VT_PREBUILT_CACHE") {
        return PathBuf::from(dir).join(PREBUILT_TAG);
    }
    let base = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("libghostty-vt-prebuilt").join(PREBUILT_TAG)
}

/// Look up the expected sha256 (hex) for `name` in the checked-in manifest.
fn expected_sha256(name: &str) -> String {
    for line in SHA256SUMS.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `sha256sum` format: "<hex>  <filename>" (two spaces, or one + '*').
        let mut parts = line.split_whitespace();
        let (Some(hex), Some(file)) = (parts.next(), parts.next()) else {
            continue;
        };
        let file = file.trim_start_matches('*');
        if file == name {
            return hex.to_lowercase();
        }
    }
    panic!(
        "no checksum pinned for '{name}' in SHA256SUMS (tag {PREBUILT_TAG}).\n\
         The release may be missing this artifact, or SHA256SUMS is out of date."
    );
}

fn http_get(url: &str) -> Vec<u8> {
    let resp = ureq::get(url)
        .call()
        .unwrap_or_else(|e| panic!("failed to download {url}: {e}"));
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .unwrap_or_else(|e| panic!("failed to read response body from {url}: {e}"));
    buf
}

fn file_sha256(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    bytes_sha256(&bytes)
}

fn bytes_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
