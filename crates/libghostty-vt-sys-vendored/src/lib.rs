#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(clippy::all)]
#![allow(rustdoc::all)]

// The bindings are not checked into this crate's compilation directly. The
// build script always places a `bindings.rs` in `OUT_DIR` — either downloaded
// from the GitHub release alongside the prebuilt native library, or copied from
// the checked-in `src/bindings.rs` fallback (docs.rs / offline). See build.rs.
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use std::ops::Deref;

pub use bindings::*;

/// Initialize a "sized" FFI object.
#[macro_export]
macro_rules! sized {
    ($ty:ty) => {{
        let mut t = <$ty as ::std::default::Default>::default();
        t.size = ::std::mem::size_of::<$ty>();
        t
    }};
}

impl<S> From<S> for bindings::String
where
    S: Deref<Target = str>,
{
    fn from(value: S) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }
}

impl bindings::String {
    /// # Safety
    ///
    /// The caller must uphold that the associated lifetime is valid
    /// with the given context behind the FFI string, and that it contains
    /// valid UTF-8 data.
    pub unsafe fn to_str<'a>(self) -> &'a str {
        // SAFETY: To be upheld by caller
        let slice = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
        unsafe { std::str::from_utf8_unchecked(slice) }
    }
}
