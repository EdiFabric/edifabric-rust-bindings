//! Rust bindings for ediFabric Native X12 (see `c-abi-edifabric_x12_tools.h`).
//!
//! Load the platform shared library (`edifabric-x12-tools.dll` / `.so` / `.dylib`) and call
//! the wrappers below. All payloads cross the boundary as UTF-8 byte buffers.
//! Native return codes: 0 = success, 1 = InsufficientCapacity (grow-and-retry),
//! anything else is an error whose message comes from [`get_error`].
//!
//! The wrappers handle grow-and-retry internally and return [`Error::Native`] on failure.
//! The library is loaded at runtime, so nothing is linked at build time.

use std::ffi::{CStr, c_char, c_int};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libloading::Library;

pub const SUCCESS: i32 = 0;
pub const INSUFFICIENT_CAPACITY: i32 = 1;

/// `mode` argument for [`parse`] and [`start_split`].
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseMode {
    Json = 1,
    JsonValidate = 2,
    JsonValidateAck = 3,
}

/// `min_level` argument for [`init_logger`].
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Information = 2,
    Warning = 3,
    Error = 4,
    All = 5,
}

/// Library-level error codes. Validation codes come from the engine.
pub mod error_code {
    pub const SUCCESS: i32 = 0;
    pub const INSUFFICIENT_CAPACITY: i32 = 1;
    pub const UNKNOWN: i32 = 501;
    pub const NO_CONNECTION: i32 = 502;
    pub const MODEL_MAP_INVALID: i32 = 503;
    pub const INCORRECT_INPUT: i32 = 611;
    pub const LOGGER_INITIALIZATION: i32 = 612;
    pub const MAP_DESERIALIZATION: i32 = 613;
    pub const INCORRECT_CAPACITY: i32 = 614;
    pub const MAP_NOT_SET: i32 = 615;
    pub const INCORRECT_MODE: i32 = 616;
    pub const NO_JSON: i32 = 617;
    pub const VALIDATION_UNAVAILABLE: i32 = 618;
    pub const VALIDATION_SERIALIZATION: i32 = 619;
    pub const INCORRECT_TOKEN: i32 = 620;
    pub const CONFIG_DESERIALIZATION: i32 = 621;
    pub const SPLIT_SEGMENT_ID_MISSING: i32 = 622;
    pub const SPLIT_NOT_STARTED: i32 = 623;
    pub const NO_RESULT: i32 = 624;
    pub const RESULT_SIZE_MISMATCH: i32 = 625;
    pub const MERGE_NOT_STARTED: i32 = 626;
    pub const INCORRECT_OUTPUT_POINTER: i32 = 627;
    pub const INCORRECT_SERIAL: i32 = 628;
    pub const LICENSE_NOT_INSTALLED: i32 = 629;
    pub const APP_VERSION_EXCEEDED: i32 = 630;
    pub const TOKEN_EXPIRED: i32 = 631;
    pub const TOKEN_MISSING: i32 = 632;
    pub const MAX_LICENSES_EXCEEDED: i32 = 633;
    pub const LICENSE_SNAPSHOT_MISSING: i32 = 634;
    pub const LICENSE_NOT_SET: i32 = 635;
    pub const RATE_EXCEEDED: i32 = 636;
    pub const INVALID_JSON: i32 = 637;
    pub const INCORRECT_LICENSE: i32 = 638;
    pub const LICENSE_QUOTA_EXCEEDED: i32 = 639;
}

/// Errors returned by the wrappers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The native library could not be found or bound.
    Load(String),
    /// A native call returned a non-zero status code.
    Native {
        code: i32,
        message: String,
        api: &'static str,
    },
    /// An argument can't be passed to the native library, for example a buffer over 2 GB.
    InvalidArgument(String),
}

impl Error {
    /// The native error code, if this is a native error.
    pub fn code(&self) -> Option<i32> {
        match self {
            Error::Native { code, .. } => Some(*code),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Load(message) => f.write_str(message),
            Error::Native {
                code,
                message,
                api: "",
            } => write!(f, "error {code}: {message}"),
            Error::Native { code, message, api } => write!(f, "{api}: error {code}: {message}"),
            Error::InvalidArgument(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Native functions
// ---------------------------------------------------------------------------

type BufferFn = unsafe extern "C" fn(*const u8, c_int) -> c_int;

struct Native {
    path: PathBuf,
    init_logger: unsafe extern "C" fn(*const u8, c_int, c_int) -> c_int,
    shutdown_logger: unsafe extern "C" fn() -> c_int,
    clear_cache: unsafe extern "C" fn() -> c_int,
    ensure_token: unsafe extern "C" fn(*const u8, c_int, c_int) -> c_int,
    get_app_version: unsafe extern "C" fn(*mut c_int) -> c_int,
    get_token: unsafe extern "C" fn(*const u8, c_int, *mut u8, c_int, *mut c_int) -> c_int,
    validate_token: BufferFn,
    set_token: BufferFn,
    get_token_expiration: unsafe extern "C" fn(*mut i64) -> c_int,
    set_serial: BufferFn,
    set_map: BufferFn,
    #[allow(clippy::type_complexity)]
    parse: unsafe extern "C" fn(
        *const u8,
        c_int,
        c_int,
        *const u8,
        c_int,
        *mut u8,
        c_int,
        *mut c_int,
        *mut c_int,
    ) -> c_int,
    start_split: unsafe extern "C" fn(*const u8, c_int, c_int, *const u8, c_int) -> c_int,
    split: unsafe extern "C" fn(*mut c_int, *mut c_int, *mut u8) -> c_int,
    build:
        unsafe extern "C" fn(*const u8, c_int, *const c_char, *mut u8, c_int, *mut c_int) -> c_int,
    start_merge: BufferFn,
    merge: unsafe extern "C" fn(*mut c_int) -> c_int,
    get_result: unsafe extern "C" fn(*mut u8, c_int) -> c_int,
    get_error: unsafe extern "C" fn(c_int) -> *mut c_char,
    free_error: Option<unsafe extern "C" fn(*mut c_char)>,
    // Keeps the function pointers above valid.
    _library: Library,
}

static NATIVE: Mutex<Option<Arc<Native>>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Library loading
// ---------------------------------------------------------------------------

/// The native library file name for this platform.
pub fn library_filename() -> &'static str {
    if cfg!(windows) {
        "edifabric-x12-tools.dll"
    } else if cfg!(target_os = "macos") {
        "edifabric-x12-tools.dylib"
    } else {
        "edifabric-x12-tools.so"
    }
}

fn resolve_candidate(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(library_filename())
    } else {
        path.to_path_buf()
    }
}

fn candidates(explicit: Option<&Path>) -> Vec<PathBuf> {
    let name = library_filename();
    let mut found = Vec::new();
    if let Some(path) = explicit {
        found.push(resolve_candidate(path));
    }
    if let Some(env) = std::env::var_os("EDIFABRIC_X12_LIB").filter(|v| !v.is_empty()) {
        found.push(resolve_candidate(Path::new(&env)));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        found.push(dir.join(name));
    }
    if let Ok(cwd) = std::env::current_dir() {
        found.push(cwd.join(name));
        if let Some(parent) = cwd.parent() {
            found.push(parent.join(name));
        }
    }
    found
}

unsafe fn symbol<T: Copy>(library: &Library, path: &Path, name: &str) -> Result<T> {
    unsafe { library.get::<T>(name) }.map(|s| *s).map_err(|_| {
        Error::Load(format!(
            "cannot find function '{name}' in {}",
            path.display()
        ))
    })
}

fn bind(path: &Path) -> Result<Native> {
    // SAFETY: loading the library runs its initializers; the ediFabric library has no unsafe ones.
    let library = unsafe { Library::new(path) }.map_err(|e| Error::Load(e.to_string()))?;
    // SAFETY: the signatures match c-abi-edifabric_x12_tools.h.
    unsafe {
        Ok(Native {
            path: path.to_path_buf(),
            init_logger: symbol(&library, path, "init_logger")?,
            shutdown_logger: symbol(&library, path, "shutdown_logger")?,
            clear_cache: symbol(&library, path, "clear_cache")?,
            ensure_token: symbol(&library, path, "ensure_token")?,
            get_app_version: symbol(&library, path, "get_app_version")?,
            get_token: symbol(&library, path, "get_token")?,
            validate_token: symbol(&library, path, "validate_token")?,
            set_token: symbol(&library, path, "set_token")?,
            get_token_expiration: symbol(&library, path, "get_token_expiration")?,
            set_serial: symbol(&library, path, "set_serial")?,
            set_map: symbol(&library, path, "set_map")?,
            parse: symbol(&library, path, "parse")?,
            start_split: symbol(&library, path, "start_split")?,
            split: symbol(&library, path, "split")?,
            build: symbol(&library, path, "build")?,
            start_merge: symbol(&library, path, "start_merge")?,
            merge: symbol(&library, path, "merge")?,
            get_result: symbol(&library, path, "get_result")?,
            get_error: symbol(&library, path, "get_error")?,
            free_error: symbol(&library, path, "free_error").ok(),
            _library: library,
        })
    }
}

/// Load and bind the native library, returning the path that was loaded.
///
/// `path` may be the library file or the folder containing it. When `None`, the
/// `EDIFABRIC_X12_LIB` variable, the executable's folder, the working directory,
/// and its parent are tried in that order. Cached after the first success; every
/// wrapper calls this with `None` on first use.
pub fn load_library(path: Option<&Path>) -> Result<PathBuf> {
    load(path).map(|native| native.path.clone())
}

fn load(path: Option<&Path>) -> Result<Arc<Native>> {
    let mut current = NATIVE.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(native), None) = (current.as_ref(), path) {
        return Ok(Arc::clone(native));
    }
    let tried = candidates(path);
    let mut last_error = None;
    for candidate in &tried {
        if !candidate.is_file() {
            continue;
        }
        match bind(candidate) {
            Ok(native) => {
                let native = Arc::new(native);
                *current = Some(Arc::clone(&native));
                return Ok(native);
            }
            Err(error) => last_error = Some(error),
        }
    }
    let tried: Vec<String> = tried.iter().map(|p| p.display().to_string()).collect();
    Err(Error::Load(format!(
        "could not load {}. Tried: {}. Last error: {}",
        library_filename(),
        tried.join(", "),
        last_error.map_or_else(|| "none".to_string(), |e| e.to_string())
    )))
}

fn native() -> Result<Arc<Native>> {
    load(None)
}

// ---------------------------------------------------------------------------
// Buffer / error helpers
// ---------------------------------------------------------------------------

/// Return a pointer and length; null and 0 for empty input so the ABI receives a null pointer.
fn input(data: &[u8]) -> Result<(*const u8, c_int)> {
    if data.is_empty() {
        return Ok((std::ptr::null(), 0));
    }
    let length = c_int::try_from(data.len()).map_err(|_| {
        Error::InvalidArgument(format!(
            "input of {} bytes exceeds the 2 GB limit",
            data.len()
        ))
    })?;
    Ok((data.as_ptr(), length))
}

fn optional_input(data: Option<&str>) -> Result<(*const u8, c_int)> {
    input(data.map_or(&[][..], str::as_bytes))
}

/// Copy a NUL-terminated native string.
///
/// # Safety
/// `ptr` must be null or point to a NUL-terminated string.
pub unsafe fn decode_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn crt_free() -> Option<unsafe extern "C" fn(*mut c_char)> {
    static CRT: Mutex<Option<Option<(Library, unsafe extern "C" fn(*mut c_char))>>> =
        Mutex::new(None);
    let mut crt = CRT.lock().unwrap_or_else(|e| e.into_inner());
    crt.get_or_insert_with(|| {
        let names: &[&str] = if cfg!(windows) {
            &["ucrtbase.dll", "msvcrt.dll"]
        } else if cfg!(target_os = "macos") {
            &["/usr/lib/libSystem.B.dylib"]
        } else {
            &["libc.so.6", "libc.so"]
        };
        names.iter().find_map(|name| {
            // SAFETY: the C runtime is already loaded in the process; free has this signature.
            let library = unsafe { Library::new(*name) }.ok()?;
            let free = unsafe { library.get::<unsafe extern "C" fn(*mut c_char)>("free") }
                .ok()
                .map(|s| *s)?;
            Some((library, free))
        })
    })
    .as_ref()
    .map(|(_, free)| *free)
}

/// Call the raw `get_error` export. The caller owns the returned string and must
/// release it with [`free_error`].
pub fn raw_get_error(error_code: i32) -> Result<*mut c_char> {
    let native = native()?;
    // SAFETY: get_error takes any code and returns a heap string or null.
    Ok(unsafe { (native.get_error)(error_code) })
}

/// Release a string returned by [`raw_get_error`].
///
/// [`get_error`] already does this; call `free_error` only when you invoke the raw
/// export yourself. Builds that do not export `free_error` fall back to the C runtime `free`.
///
/// # Safety
/// `ptr` must be null or a pointer returned by [`raw_get_error`] that wasn't freed yet.
pub unsafe fn free_error(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let free = native().ok().and_then(|n| n.free_error).or_else(crt_free);
    if let Some(free) = free {
        unsafe { free(ptr) };
    }
}

/// Return the message for an error code, releasing the native string.
pub fn get_error(error_code: i32) -> String {
    let ptr = match raw_get_error(error_code) {
        Ok(ptr) => ptr,
        Err(error) => return error.to_string(),
    };
    // SAFETY: ptr comes from get_error and is freed exactly once.
    let message = unsafe { decode_string(ptr) };
    unsafe { free_error(ptr) };
    if message.is_empty() {
        format!("Unknown error {error_code}")
    } else {
        message
    }
}

/// Return [`Error::Native`] unless `rc` is [`SUCCESS`].
pub fn check(rc: i32, api: &'static str) -> Result<()> {
    if rc == SUCCESS {
        Ok(())
    } else {
        Err(Error::Native {
            code: rc,
            message: get_error(rc),
            api,
        })
    }
}

fn next_capacity(capacity: usize, required: c_int, api: &'static str) -> Result<usize> {
    let max = c_int::MAX as usize;
    let next = (required.max(0) as usize)
        .max(capacity.saturating_mul(2))
        .min(max);
    if next <= capacity {
        return check(INSUFFICIENT_CAPACITY, api).map(|_| capacity);
    }
    Ok(next)
}

/// Run a grow-and-retry output call; `call(output, capacity, output_length)` returns the status code.
fn with_growth(
    capacity: usize,
    api: &'static str,
    mut call: impl FnMut(*mut u8, c_int, *mut c_int) -> c_int,
) -> Result<Vec<u8>> {
    let mut capacity = capacity.clamp(1, c_int::MAX as usize);
    loop {
        let mut output = vec![0u8; capacity];
        let mut output_length: c_int = 0;
        let rc = call(output.as_mut_ptr(), capacity as c_int, &mut output_length);
        if rc == INSUFFICIENT_CAPACITY {
            capacity = next_capacity(capacity, output_length, api)?;
            continue;
        }
        check(rc, api)?;
        output.truncate(output_length.max(0) as usize);
        return Ok(output);
    }
}

fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

// ---------------------------------------------------------------------------
// Lifecycle and logging
// ---------------------------------------------------------------------------

/// Write a log file at `path`; `min_level` is [`LogLevel::Trace`] through [`LogLevel::Error`].
pub fn init_logger(path: impl AsRef<Path>, min_level: LogLevel) -> Result<()> {
    let native = native()?;
    let path = path.as_ref().to_string_lossy();
    let (ptr, length) = input(path.as_bytes())?;
    check(
        unsafe { (native.init_logger)(ptr, length, min_level as c_int) },
        "init_logger",
    )
}

/// Flush and stop the logger.
pub fn shutdown_logger() -> Result<()> {
    let native = native()?;
    check(unsafe { (native.shutdown_logger)() }, "shutdown_logger")
}

/// Reset the model map, split/merge state, last result, license, and logger.
pub fn clear_cache() -> Result<()> {
    let native = native()?;
    check(unsafe { (native.clear_cache)() }, "clear_cache")
}

// ---------------------------------------------------------------------------
// Licensing
// ---------------------------------------------------------------------------

/// Cache a token for runtime authorization against the license server.
///
/// Retrieves a new token if the cached token expires within the specified
/// number of seconds.
pub fn ensure_token(serial: &str, seconds: i32) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(serial.as_bytes())?;
    check(
        unsafe { (native.ensure_token)(ptr, length, seconds) },
        "ensure_token",
    )
}

/// Return the library application version.
pub fn get_app_version() -> Result<i32> {
    let native = native()?;
    let mut version: c_int = 0;
    check(
        unsafe { (native.get_app_version)(&mut version) },
        "get_app_version",
    )?;
    Ok(version)
}

/// Fetch a signed license token for the serial. Requires internet access.
pub fn get_token(serial: &str) -> Result<String> {
    let native = native()?;
    let (serial_ptr, serial_length) = input(serial.as_bytes())?;
    with_growth(
        4096,
        "get_token",
        |output, capacity, output_length| unsafe {
            (native.get_token)(serial_ptr, serial_length, output, capacity, output_length)
        },
    )
    .map(into_string)
}

/// Validate a token without caching it.
pub fn validate_token(token: &str) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(token.as_bytes())?;
    check(
        unsafe { (native.validate_token)(ptr, length) },
        "validate_token",
    )
}

/// Cache a token for this process.
pub fn set_token(token: &str) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(token.as_bytes())?;
    check(unsafe { (native.set_token)(ptr, length) }, "set_token")
}

/// Return the token expiry as .NET UTC ticks; 0 when no token is set.
pub fn get_token_expiration() -> Result<i64> {
    let native = native()?;
    let mut expiration: i64 = 0;
    check(
        unsafe { (native.get_token_expiration)(&mut expiration) },
        "get_token_expiration",
    )?;
    Ok(expiration)
}

/// Convert .NET UTC ticks (100 ns intervals since year 1) to [`SystemTime`]; `None` for 0.
pub fn ticks_to_system_time(ticks: i64) -> Option<SystemTime> {
    const TICKS_PER_SECOND: i64 = 10_000_000;
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    if ticks == 0 {
        return None;
    }
    let since_unix = ticks - UNIX_EPOCH_TICKS;
    let duration = Duration::new(
        since_unix.unsigned_abs() / TICKS_PER_SECOND as u64,
        (since_unix.unsigned_abs() % TICKS_PER_SECOND as u64 * 100) as u32,
    );
    if since_unix >= 0 {
        UNIX_EPOCH.checked_add(duration)
    } else {
        UNIX_EPOCH.checked_sub(duration)
    }
}

/// Cache a serial for runtime authorization against the license server.
pub fn set_serial(serial: &str) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(serial.as_bytes())?;
    check(unsafe { (native.set_serial)(ptr, length) }, "set_serial")
}

// ---------------------------------------------------------------------------
// Model map
// ---------------------------------------------------------------------------

/// Load the template map JSON. Call once before [`parse`] or [`start_split`].
pub fn set_map(map_json: &str) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(map_json.as_bytes())?;
    check(unsafe { (native.set_map)(ptr, length) }, "set_map")
}

// ---------------------------------------------------------------------------
// Parse, split, build, merge
// ---------------------------------------------------------------------------

/// Parse a whole interchange and return `(output, offset)`.
///
/// `offset` is 0 for [`ParseMode::Json`]; for the other modes it marks where the
/// validation and acknowledgment section begins inside `output`.
pub fn parse(
    edi: impl AsRef<[u8]>,
    mode: ParseMode,
    config: Option<&str>,
) -> Result<(String, usize)> {
    let native = native()?;
    let edi = edi.as_ref();
    let (input_ptr, input_length) = input(edi)?;
    let (config_ptr, config_length) = optional_input(config)?;
    let mut capacity = edi
        .len()
        .saturating_mul(12)
        .clamp(4096, c_int::MAX as usize);
    loop {
        let mut output = vec![0u8; capacity];
        let mut output_length: c_int = 0;
        let mut output_offset: c_int = 0;
        let rc = unsafe {
            (native.parse)(
                input_ptr,
                input_length,
                mode as c_int,
                config_ptr,
                config_length,
                output.as_mut_ptr(),
                capacity as c_int,
                &mut output_length,
                &mut output_offset,
            )
        };
        if rc == INSUFFICIENT_CAPACITY {
            capacity = next_capacity(capacity, output_length, "parse")?;
            continue;
        }
        check(rc, "parse")?;
        output.truncate(output_length.max(0) as usize);
        return Ok((into_string(output), output_offset.max(0) as usize));
    }
}

/// Begin a streaming split. `config` must contain a split section with `segment_id`.
pub fn start_split(edi: impl AsRef<[u8]>, mode: ParseMode, config: Option<&str>) -> Result<()> {
    let native = native()?;
    let (input_ptr, input_length) = input(edi.as_ref())?;
    let (config_ptr, config_length) = optional_input(config)?;
    check(
        unsafe {
            (native.start_split)(
                input_ptr,
                input_length,
                mode as c_int,
                config_ptr,
                config_length,
            )
        },
        "start_split",
    )
}

/// One step of a split stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitStep {
    pub size: usize,
    pub offset: usize,
    pub is_last: bool,
}

/// Advance the split stream.
///
/// Fetch the payload with [`get_result`] whenever `size` is greater than zero.
pub fn split() -> Result<SplitStep> {
    let native = native()?;
    let mut size: c_int = 0;
    let mut offset: c_int = 0;
    let mut last: u8 = 0;
    check(
        unsafe { (native.split)(&mut size, &mut offset, &mut last) },
        "split",
    )?;
    Ok(SplitStep {
        size: size.max(0) as usize,
        offset: offset.max(0) as usize,
        is_last: last != 0,
    })
}

/// Build X12 EDI from transaction-set JSON.
///
/// `postfix` is appended after each segment terminator, for example `"\r\n"`.
/// Pass `None` for compact single-line output.
pub fn build(json: impl AsRef<[u8]>, postfix: Option<&str>) -> Result<String> {
    let native = native()?;
    let json = json.as_ref();
    let (input_ptr, input_length) = input(json)?;
    let postfix = postfix
        .map(|p| {
            std::ffi::CString::new(p)
                .map_err(|_| Error::InvalidArgument("postfix contains a NUL byte".into()))
        })
        .transpose()?;
    let postfix_ptr = postfix.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
    with_growth(
        json.len().max(4096),
        "build",
        |output, capacity, output_length| unsafe {
            (native.build)(
                input_ptr,
                input_length,
                postfix_ptr,
                output,
                capacity,
                output_length,
            )
        },
    )
    .map(into_string)
}

/// Begin a streaming merge from a full interchange JSON document.
pub fn start_merge(json: impl AsRef<[u8]>) -> Result<()> {
    let native = native()?;
    let (ptr, length) = input(json.as_ref())?;
    check(unsafe { (native.start_merge)(ptr, length) }, "start_merge")
}

/// Return the size of the next segment, or 0 at the end of the stream.
pub fn merge() -> Result<usize> {
    let native = native()?;
    let mut size: c_int = 0;
    check(unsafe { (native.merge)(&mut size) }, "merge")?;
    Ok(size.max(0) as usize)
}

/// Copy the last split or merge result. `size` must match the reported size.
pub fn get_result(size: usize) -> Result<Vec<u8>> {
    let native = native()?;
    let length = c_int::try_from(size)
        .map_err(|_| Error::InvalidArgument(format!("size {size} is too large")))?;
    let mut result = vec![0u8; size];
    let ptr = if size == 0 {
        std::ptr::null_mut()
    } else {
        result.as_mut_ptr()
    };
    check(unsafe { (native.get_result)(ptr, length) }, "get_result")?;
    Ok(result)
}

// ---------------------------------------------------------------------------
// Convenience iterators
// ---------------------------------------------------------------------------

/// One payload produced by [`iter_split`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPart {
    pub payload: Vec<u8>,
    pub offset: usize,
    pub is_last: bool,
}

/// Iterator returned by [`iter_split`]. Stops after the first error.
pub struct SplitIter {
    done: bool,
}

impl Iterator for SplitIter {
    type Item = Result<SplitPart>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.done {
            let step = match split() {
                Ok(step) => step,
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            self.done = step.is_last;
            if step.size > 0 {
                let part = get_result(step.size).map(|payload| SplitPart {
                    payload,
                    offset: step.offset,
                    is_last: step.is_last,
                });
                self.done |= part.is_err();
                return Some(part);
            }
        }
        None
    }
}

/// Start a split and yield every step that produces data.
pub fn iter_split(
    edi: impl AsRef<[u8]>,
    mode: ParseMode,
    config: Option<&str>,
) -> Result<SplitIter> {
    start_split(edi, mode, config)?;
    Ok(SplitIter { done: false })
}

/// Iterator returned by [`iter_merge`]. Stops after the first error.
pub struct MergeIter {
    done: bool,
}

impl Iterator for MergeIter {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let segment = merge().and_then(|size| {
            if size == 0 {
                Ok(None)
            } else {
                get_result(size).map(Some)
            }
        });
        match segment {
            Ok(Some(segment)) => Some(Ok(segment)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

/// Start a merge and yield each X12 segment.
pub fn iter_merge(json: impl AsRef<[u8]>) -> Result<MergeIter> {
    start_merge(json)?;
    Ok(MergeIter { done: false })
}
