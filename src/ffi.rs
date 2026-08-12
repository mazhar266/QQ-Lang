// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! C ABI.
//!
//! The only module in the crate where `unsafe` is allowed. Everything here is
//! a thin wrapper over [`Context`] — no query logic lives at this layer.
//!
//! # Contract
//!
//! - Strings handed out are NUL-terminated UTF-8 and belong to the caller,
//!   who must release them with [`qql_free_string`] — never with `free()` or a
//!   host-language allocator. The single exception is [`qql_version`], which
//!   returns a static string that must **not** be freed.
//! - Null and misaligned pointers are handled defensively: a null context or
//!   query yields an error JSON string, never a crash.
//! - Panics never cross the boundary. Every entry point catches unwinds and
//!   converts them into `QQL_INTERNAL_ERROR`.
//! - One `qql_context_t` must not be used from two threads at once. Separate
//!   contexts on separate threads are fine.
//!
//! # Safety
//!
//! Callers must pass either null or a valid pointer of the documented kind.
//! Passing a dangling pointer, freeing a string twice, or using a context
//! after [`qql_context_destroy`] is undefined behavior — the usual C rules.

#![allow(unsafe_code)]
#![allow(non_camel_case_types)]

use crate::context::Context;
use crate::error::Error;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

/// Opaque handle to a [`Context`].
///
/// C sees an incomplete type and only ever holds the pointer, so the layout of
/// this struct is not part of the ABI.
pub struct qql_context_t {
    inner: Context,
}

/// Data directory used by [`qql_execute`], overridable with `QQL_DATA`.
const DEFAULT_DATA_DIR: &str = "sources";

/// Last-resort response when even error serialization fails.
const FALLBACK_JSON: &str =
    r#"{"ok":false,"error":{"code":"QQL_INTERNAL_ERROR","message":"panic in qql"}}"#;

/// Library version as a static NUL-terminated string.
///
/// The returned pointer is owned by the library and must **not** be passed to
/// [`qql_free_string`].
#[no_mangle]
pub extern "C" fn qql_version() -> *const c_char {
    static VERSION: OnceLock<CString> = OnceLock::new();
    VERSION
        .get_or_init(|| CString::new(crate::VERSION).unwrap_or_default())
        .as_ptr()
}

/// Create a context reading data from `data_directory`.
///
/// Returns null if `data_directory` is null, is not valid UTF-8, or if
/// allocation fails. The returned pointer must be released with
/// [`qql_context_destroy`].
///
/// # Safety
///
/// `data_directory` must be null or point to a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn qql_context_create(data_directory: *const c_char) -> *mut qql_context_t {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(dir) = (unsafe { borrow_str(data_directory) }) else {
            return std::ptr::null_mut();
        };
        let Ok(dir) = dir else {
            return std::ptr::null_mut();
        };
        Box::into_raw(Box::new(qql_context_t {
            inner: Context::new(dir),
        }))
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Execute `query` against `ctx` and return an allocated JSON string.
///
/// Never returns null and never returns invalid JSON: failures are serialized
/// into the response. Release the result with [`qql_free_string`].
///
/// # Safety
///
/// `ctx` must be null or a pointer from [`qql_context_create`] that has not
/// been destroyed. `query` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn qql_context_execute(
    ctx: *mut qql_context_t,
    query: *const c_char,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(handle) = (unsafe { ctx.as_mut() }) else {
            return into_c_string(
                Error::Internal {
                    detail: "null context".into(),
                }
                .to_json("")
                .to_string(),
            );
        };

        // SAFETY: forwarded to the caller's contract on `query`.
        let (text, query) = unsafe { query_str(query) };
        match text {
            Ok(text) => into_c_string(handle.inner.execute_json(text)),
            Err(e) => into_c_string(e.to_json(query).to_string()),
        }
    }))
    .unwrap_or_else(|_| into_c_string(FALLBACK_JSON.to_string()))
}

/// Destroy a context created by [`qql_context_create`]. Null is a no-op.
///
/// Strings previously returned by [`qql_context_execute`] stay valid — they
/// are independently owned — and must still be freed individually.
///
/// # Safety
///
/// `ctx` must be null or a pointer from [`qql_context_create`] that has not
/// already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn qql_context_destroy(ctx: *mut qql_context_t) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: the caller guarantees this came from `Box::into_raw` above and
    // has not already been reclaimed.
    let boxed = unsafe { Box::from_raw(ctx) };
    let _ = catch_unwind(AssertUnwindSafe(move || drop(boxed)));
}

/// Execute `query` against a process-wide default context.
///
/// The default context reads from `$QQL_DATA`, or `./sources` if that is
/// unset, and is guarded by a mutex — safe from any thread, but serialized.
/// Prefer [`qql_context_execute`] with your own context for anything
/// concurrent. Release the result with [`qql_free_string`].
///
/// # Safety
///
/// `query` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn qql_execute(query: *const c_char) -> *mut c_char {
    static DEFAULT: OnceLock<Mutex<Context>> = OnceLock::new();

    catch_unwind(AssertUnwindSafe(|| {
        let mutex = DEFAULT.get_or_init(|| {
            let dir = std::env::var("QQL_DATA").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
            Mutex::new(Context::new(dir))
        });

        // A panic in an earlier call must not permanently break this entry
        // point, so recover the context rather than propagating poisoning.
        let mut ctx = mutex.lock().unwrap_or_else(|e| e.into_inner());

        // SAFETY: forwarded to the caller's contract on `query`.
        let (text, query) = unsafe { query_str(query) };
        match text {
            Ok(text) => into_c_string(ctx.execute_json(text)),
            Err(e) => into_c_string(e.to_json(query).to_string()),
        }
    }))
    .unwrap_or_else(|_| into_c_string(FALLBACK_JSON.to_string()))
}

/// Free a string returned by [`qql_context_execute`] or [`qql_execute`].
///
/// Null is a no-op. Do not pass the result of [`qql_version`], a pointer not
/// produced by this library, or the same pointer twice.
///
/// # Safety
///
/// `ptr` must be null or a pointer previously returned by this library and not
/// yet freed.
#[no_mangle]
pub unsafe extern "C" fn qql_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller guarantees this came from `CString::into_raw`.
    let owned = unsafe { CString::from_raw(ptr) };
    drop(owned);
}

/// Borrow a C string. `None` for null, `Err` for invalid UTF-8.
///
/// # Safety
///
/// `ptr` must be null or NUL-terminated.
unsafe fn borrow_str<'a>(ptr: *const c_char) -> Option<Result<&'a str, std::str::Utf8Error>> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees NUL termination.
    Some(unsafe { CStr::from_ptr(ptr) }.to_str())
}

/// Resolve a query pointer into text plus the string to echo back in the
/// response. Invalid UTF-8 reports the byte offset where it went wrong rather
/// than being lossily replaced.
///
/// # Safety
///
/// `ptr` must be null or NUL-terminated.
unsafe fn query_str<'a>(ptr: *const c_char) -> (Result<&'a str, Error>, &'a str) {
    match unsafe { borrow_str(ptr) } {
        None => (
            Err(Error::Internal {
                detail: "null query".into(),
            }),
            "",
        ),
        Some(Ok(text)) => (Ok(text), text),
        Some(Err(e)) => (
            Err(Error::InvalidCharacter {
                position: e.valid_up_to(),
            }),
            "",
        ),
    }
}

/// Hand a Rust string to C. An interior NUL cannot survive a C string, so it
/// degrades to the fallback response rather than truncating the JSON.
fn into_c_string(text: String) -> *mut c_char {
    CString::new(text)
        .or_else(|_| CString::new(FALLBACK_JSON))
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
