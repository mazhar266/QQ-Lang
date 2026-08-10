//! The parser must never panic and never hang, for any input.
//!
//! Memory safety is already guaranteed by safe Rust, so this hunts panics:
//! arithmetic overflow, slicing on a non-char boundary, unwrap on a malformed
//! token, and unbounded allocation.
//!
//! ```bash
//! cargo +nightly fuzz run parse
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(query) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(parsed) = qql::parse(query) {
        // Expansion is the other panic risk: it must reject out-of-bounds
        // selectors rather than trying to allocate them.
        for reference in &parsed.references {
            let _ = reference.expand(1000);
        }
    }
});
