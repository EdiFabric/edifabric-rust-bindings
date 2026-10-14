//! The ediFabric Native X12 library for Linux arm64 (glibc 2.34+), embedded for the
//! `edifabric-x12` crate. Depend on `edifabric-x12` instead of this crate.
#![no_std]

/// The file name the library must be saved under before it is loaded.
pub const FILE_NAME: &str = "edifabric-x12-tools.so";

/// The library, or `None` when this crate was built from a checkout without it.
#[cfg(has_library)]
pub static LIBRARY: Option<&[u8]> = Some(include_bytes!("../edifabric-x12-tools.so"));
#[cfg(not(has_library))]
pub static LIBRARY: Option<&[u8]> = None;
