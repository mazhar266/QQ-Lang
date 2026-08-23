// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! C ABI tests, including the abuse cases the compiler cannot rule out.
//!
//! Run these under Miri when the FFI layer changes:
//! `cargo +nightly miri test --test ffi`

use qql::ffi::*;
use serde_json::Value;
use std::ffi::{c_char, CStr, CString};

/// Take ownership of a returned string, parse it, and free it.
fn take(ptr: *mut c_char) -> Value {
    assert!(!ptr.is_null(), "the library must never return null JSON");
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("returned string must be UTF-8")
        .to_owned();
    unsafe { qql_free_string(ptr) };
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON {text:?}: {e}"))
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn data_present() -> bool {
    std::path::Path::new("sources/quran/chapters/1.json").exists()
}

#[test]
fn version_is_static_and_matches_the_crate() {
    let ptr = qql_version();
    assert!(!ptr.is_null());
    let version = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(version, qql::VERSION);
    // Called twice, same static pointer — nothing to free.
    assert_eq!(qql_version(), ptr);
}

#[test]
fn round_trip_through_the_c_abi() {
    if !data_present() {
        eprintln!("skipping: sources/ submodules not checked out");
        return;
    }

    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };
    assert!(!ctx.is_null());

    let query = cstr("Q:2:255");
    let value = take(unsafe { qql_context_execute(ctx, query.as_ptr()) });

    assert_eq!(value["ok"], true);
    assert_eq!(value["results"][0]["ayah"], 255);
    assert!(!value["results"][0]["ar"].as_str().unwrap().is_empty());

    unsafe { qql_context_destroy(ctx) };
}

#[test]
fn returned_strings_outlive_their_context() {
    if !data_present() {
        return;
    }

    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };
    let query = cstr("Q:1:1");
    let result = unsafe { qql_context_execute(ctx, query.as_ptr()) };

    // Destroy first, read after — the string is independently owned.
    unsafe { qql_context_destroy(ctx) };

    let text = unsafe { CStr::from_ptr(result) }.to_str().unwrap().to_owned();
    assert!(text.contains("\"ok\":true"));
    unsafe { qql_free_string(result) };
}

#[test]
fn errors_come_back_as_json_not_as_null() {
    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };

    for (query, code) in [
        ("", "QQL_EMPTY_QUERY"),
        ("Q:2:5-1", "QQL_INVALID_RANGE"),
        ("XYZ:1", "QQL_UNKNOWN_SOURCE"),
        ("!!!", "QQL_INVALID_CHARACTER"),
    ] {
        let query = cstr(query);
        let value = take(unsafe { qql_context_execute(ctx, query.as_ptr()) });
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], code);
    }

    unsafe { qql_context_destroy(ctx) };
}

#[test]
fn null_data_directory_yields_null_rather_than_a_crash() {
    assert!(unsafe { qql_context_create(std::ptr::null()) }.is_null());
}

#[test]
fn null_context_yields_an_error_json() {
    let query = cstr("Q:1");
    let value = take(unsafe { qql_context_execute(std::ptr::null_mut(), query.as_ptr()) });
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "QQL_INTERNAL_ERROR");
}

#[test]
fn null_query_yields_an_error_json() {
    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };
    let value = take(unsafe { qql_context_execute(ctx, std::ptr::null()) });
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "QQL_INTERNAL_ERROR");
    unsafe { qql_context_destroy(ctx) };
}

#[test]
fn invalid_utf8_reports_the_offset_instead_of_being_replaced() {
    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };

    // "Q:1" followed by a lone continuation byte.
    let raw = CString::new(vec![b'Q', b':', b'1', 0x80]).unwrap();
    let value = take(unsafe { qql_context_execute(ctx, raw.as_ptr()) });

    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "QQL_INVALID_CHARACTER");
    assert_eq!(value["error"]["position"], 3);

    unsafe { qql_context_destroy(ctx) };
}

#[test]
fn destroying_and_freeing_null_are_no_ops() {
    unsafe { qql_context_destroy(std::ptr::null_mut()) };
    unsafe { qql_free_string(std::ptr::null_mut()) };
}

#[test]
fn the_default_context_works_and_survives_repeated_use() {
    if !data_present() {
        return;
    }
    // The default context reads $QQL_DATA; the tests run from the crate root,
    // where the default `sources` is already correct.
    let query = cstr("Q:1:1");
    for _ in 0..3 {
        let value = take(unsafe { qql_execute(query.as_ptr()) });
        assert_eq!(value["ok"], true);
    }
}

#[test]
fn many_allocations_are_balanced() {
    if !data_present() {
        return;
    }
    let dir = cstr("sources");
    let ctx = unsafe { qql_context_create(dir.as_ptr()) };
    let query = cstr("Q:2:1-10");

    // Under ASan/Miri this is where an unbalanced into_raw/from_raw shows up.
    for _ in 0..200 {
        let ptr = unsafe { qql_context_execute(ctx, query.as_ptr()) };
        assert!(!ptr.is_null());
        unsafe { qql_free_string(ptr) };
    }

    unsafe { qql_context_destroy(ctx) };
}
