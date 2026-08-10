//! The C entry point accepts bytes that are not valid UTF-8, so it needs its
//! own target rather than riding on `parse`.
//!
//! ```bash
//! cargo +nightly fuzz run ffi
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;

/// One context for the whole run, pointed at a directory that does not exist.
/// Lexing, parsing, registry lookup, and error serialization all still run;
/// resolution stops at "data file not found" instead of doing real I/O on
/// every iteration.
fn context() -> usize {
    static CTX: OnceLock<usize> = OnceLock::new();
    *CTX.get_or_init(|| {
        let dir = CString::new("/nonexistent-qql-fuzz").unwrap();
        unsafe { qql::ffi::qql_context_create(dir.as_ptr()) as usize }
    })
}

fuzz_target!(|data: &[u8]| {
    // An interior NUL just means a shorter C string — a valid input to fuzz,
    // not a case to skip.
    let Ok(query) = CString::new(data) else {
        return;
    };

    let ctx = context() as *mut _;
    let result = unsafe { qql::ffi::qql_context_execute(ctx, query.as_ptr()) };

    assert!(!result.is_null(), "the FFI layer must never return null JSON");
    let text = unsafe { CStr::from_ptr(result as *const c_char) };
    assert!(
        text.to_str().is_ok(),
        "the FFI layer must always return UTF-8"
    );

    unsafe { qql::ffi::qql_free_string(result) };
});
