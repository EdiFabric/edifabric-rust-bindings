//! Walkthrough of every function in the ediFabric Native X12 C ABI.
//!
//! Each section calls one group of entry points and prints the result, so the
//! output doubles as a reference for what each call returns.
//!
//! Usage:
//!     cargo run --example example_all_functions
//!     cargo run --example example_all_functions -- --serial YOUR_SERIAL
//!     cargo run --example example_all_functions -- --lib /path/to/edifabric-x12-tools.dll

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::UNIX_EPOCH;

use edifabric_x12 as ef;
use serde_json::{Value, json};

// Free plan serial published in the ediFabric Native documentation.
const DEFAULT_SERIAL: &str = "bd96a836feca45cb91c86ee65d281f52";

struct Samples {
    root: PathBuf,
    map_path: PathBuf,
    edi: Vec<u8>,
    edi_invalid: Vec<u8>,
    parse_config: String,
    split_config: String,
}

// Validation levels are 0 - no validation; 1 - syntax only; 2 - limits and codes; 3 - balancing; 4 - inter segment. The levels match the HIPAA SNIP validation levels.
fn parse_settings() -> Value {
    json!({
        "validate": {
            "regex": null,
            "date_format": null,
            "time_format": null,
            "skip_seq_count": false,
            "skip_hl_seq": false,
            "snip_level": 4,
            "max_errors": 100
        },
        "ack": {
            "supress_ta1": false,
            "ak901p": false,
            "gen_for_valid": true,
            "gen997": false
        }
    })
}

impl Samples {
    fn load() -> std::io::Result<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut split_settings = parse_settings();
        split_settings["split"] =
            json!({"segment_id": "LX", "segment_depth": 6, "loop_id": "2400"});
        Ok(Self {
            map_path: root.join("map").join("map.json"),
            edi: std::fs::read(root.join("edi").join("837p.txt"))?,
            edi_invalid: std::fs::read(root.join("edi").join("837p_error.txt"))?,
            parse_config: parse_settings().to_string(),
            split_config: split_settings.to_string(),
            root,
        })
    }

    fn read_map(&self) -> Value {
        let text = std::fs::read_to_string(&self.map_path).expect("map/map.json is readable");
        serde_json::from_str(&text).expect("map/map.json is valid JSON")
    }

    /// Load map/map.json and point each entry at the local map folder.
    fn load_local_map(&self) -> Value {
        let mut model_map = self.read_map();
        let map_dir = self.map_path.parent().unwrap().display().to_string();
        if let Some(maps) = model_map["maps"].as_object_mut() {
            for entry in maps.values_mut() {
                entry["location"] = json!(map_dir);
            }
        }
        model_map
    }

    /// Load map/map.json and resolve unmapped transaction sets online with the serial.
    fn load_online_map(&self, serial: &str) -> Value {
        let mut model_map = self.read_map();
        model_map["default"] = json!(serial);
        model_map
    }

    fn print_map(&self, model_map: &Value) {
        let keys = model_map["maps"]
            .as_object()
            .map(|maps| maps.keys().cloned().collect::<Vec<_>>().join(", "))
            .filter(|keys| !keys.is_empty())
            .unwrap_or_else(|| "(none)".to_string());
        let relative = self
            .map_path
            .strip_prefix(&self.root)
            .unwrap_or(&self.map_path);
        println!("  set_map <- {}", relative.display());
        println!("  default={}, local maps={keys}", model_map["default"]);
    }
}

fn section(title: &str) {
    println!();
    println!("{}", "=".repeat(70));
    println!("{title}");
    println!("{}", "=".repeat(70));
}

fn preview(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{} ...", &text[..end]),
        None => text.to_string(),
    }
}

/// get_error, raw_get_error, free_error
fn demo_errors() -> ef::Result<()> {
    section("Error messages: get_error, raw_get_error, free_error");

    for code in [
        ef::error_code::INSUFFICIENT_CAPACITY,
        ef::error_code::MAP_NOT_SET,
        ef::error_code::LICENSE_NOT_SET,
    ] {
        println!("  get_error({code:>3}) -> {}", ef::get_error(code));
    }

    // The raw export hands back a heap pointer that the caller owns.
    let ptr = ef::raw_get_error(ef::error_code::TOKEN_EXPIRED)?;
    // SAFETY: ptr comes from raw_get_error and is freed once.
    let message = unsafe { ef::decode_string(ptr) };
    unsafe { ef::free_error(ptr) };
    println!("  raw_get_error(631) -> {message}");
    println!("  free_error(ptr) released the string");
    Ok(())
}

/// init_logger, get_app_version
fn demo_logging(log_path: &Path) -> ef::Result<()> {
    section("Lifecycle: init_logger, get_app_version");

    ef::init_logger(log_path, ef::LogLevel::Trace)?;
    println!("  init_logger -> logging to {}", log_path.display());
    println!("  get_app_version -> {}", ef::get_app_version()?);
    Ok(())
}

/// get_token_expiration
fn demo_token_expiration() -> ef::Result<()> {
    section("Licensing: get_token_expiration");

    let ticks = ef::get_token_expiration()?;
    let when = match ef::ticks_to_system_time(ticks) {
        Some(time) => match time.duration_since(UNIX_EPOCH) {
            Ok(since) => format!("{} seconds after the Unix epoch", since.as_secs()),
            Err(_) => "before the Unix epoch".to_string(),
        },
        None => "no token set".to_string(),
    };
    println!("  get_token_expiration -> {ticks} ({when})");
    Ok(())
}

/// set_map
fn demo_set_online_map(samples: &Samples, serial: &str) -> ef::Result<()> {
    section("Model map: set_map online, default is the serial key");

    let model_map = samples.load_online_map(serial);
    ef::set_map(&model_map.to_string())?;
    samples.print_map(&model_map);
    Ok(())
}

/// set_map
fn demo_set_local_map(samples: &Samples) -> ef::Result<()> {
    section("Model map: set_map local, default is blank");

    let model_map = samples.load_local_map();
    ef::set_map(&model_map.to_string())?;
    samples.print_map(&model_map);
    Ok(())
}

/// parse in mode 1
fn demo_parse(samples: &Samples) -> ef::Result<String> {
    section("Parse: parse (mode 1, JSON only)");

    let (output, offset) = ef::parse(&samples.edi, ef::ParseMode::Json, None)?;
    println!("  {} bytes of JSON, offset={offset}", output.len());
    println!("  {}", preview(&output, 400));
    Ok(output)
}

/// parse in mode 2
fn demo_parse_validate(samples: &Samples) -> ef::Result<()> {
    section("Parse: parse (mode 2, JSON + validation report)");

    let (output, offset) = ef::parse(
        &samples.edi_invalid,
        ef::ParseMode::JsonValidate,
        Some(&samples.parse_config),
    )?;
    println!(
        "  {} bytes total, validation starts at offset {offset}",
        output.len()
    );
    println!("  validation -> {}", preview(&output[offset..], 400));
    Ok(())
}

/// parse in mode 3
fn demo_parse_ack(samples: &Samples, title: &str, edi: &[u8]) -> ef::Result<()> {
    section(title);

    let (output, offset) = ef::parse(
        edi,
        ef::ParseMode::JsonValidateAck,
        Some(&samples.parse_config),
    )?;
    println!(
        "  {} bytes total, report starts at offset {offset}",
        output.len()
    );
    println!("  report -> {}", preview(&output[offset..], 600));
    Ok(())
}

/// start_split, split, get_result
fn demo_split(samples: &Samples) -> ef::Result<()> {
    section("Split: start_split, split, get_result");

    ef::start_split(
        &samples.edi,
        ef::ParseMode::Json,
        Some(&samples.split_config),
    )?;
    for step_number in 1.. {
        let step = ef::split()?;
        let payload = if step.size > 0 {
            ef::get_result(step.size)?
        } else {
            Vec::new()
        };
        println!(
            "  step {step_number}: size={} offset={} last={}",
            step.size, step.offset, step.is_last
        );
        if !payload.is_empty() {
            println!("    {}", preview(&String::from_utf8_lossy(&payload), 160));
        }
        if step.is_last {
            break;
        }
    }
    Ok(())
}

/// build
fn demo_build(parsed_json: &str) -> ef::Result<()> {
    section("Build: build");

    let edi = ef::build(parsed_json, Some("\r\n"))?;
    println!("  {} bytes of X12", edi.len());
    println!("  {}", edi.replace("\r\n", "\n  ").trim_end());
    Ok(())
}

/// start_merge, merge, get_result
fn demo_merge(parsed_json: &str) -> ef::Result<()> {
    section("Merge: start_merge, merge, get_result");

    ef::start_merge(parsed_json)?;
    let mut count = 0;
    loop {
        let size = ef::merge()?;
        if size == 0 {
            break;
        }
        let segment = ef::get_result(size)?;
        count += 1;
        println!(
            "  segment {count:>2}: {}",
            String::from_utf8_lossy(&segment)
        );
    }
    println!("  merge produced {count} segments");
    Ok(())
}

/// iter_split, iter_merge convenience wrappers
fn demo_iterators(samples: &Samples, parsed_json: &str) -> ef::Result<()> {
    section("Convenience wrappers: iter_split, iter_merge");

    let parts = ef::iter_split(
        &samples.edi,
        ef::ParseMode::Json,
        Some(&samples.split_config),
    )?
    .collect::<ef::Result<Vec<_>>>()?;
    let sizes: Vec<usize> = parts.iter().map(|part| part.payload.len()).collect();
    println!("  iter_split -> {} payloads, sizes {sizes:?}", parts.len());

    let segments = ef::iter_merge(parsed_json)?.collect::<ef::Result<Vec<_>>>()?;
    println!("  iter_merge -> {} segments", segments.len());
    Ok(())
}

/// clear_cache, shutdown_logger
fn demo_teardown() -> ef::Result<()> {
    section("Teardown: clear_cache, shutdown_logger");

    ef::clear_cache()?;
    println!("  clear_cache -> map, license, stream state and results reset");
    ef::shutdown_logger()?;
    println!("  shutdown_logger -> logger stopped");
    Ok(())
}

fn run_demos(samples: &Samples, serial: &str) -> ef::Result<()> {
    // Community authorizes with the serial only. Developer can also use ensure_token, Enterprise set_token.
    ef::set_serial(serial)?;
    println!("  set_serial -> ok");

    demo_token_expiration()?;

    demo_set_local_map(samples)?;

    let parsed_json = demo_parse(samples)?;

    demo_set_online_map(samples, serial)?;

    demo_parse_validate(samples)?;
    demo_parse_ack(
        samples,
        "Parse: parse (mode 3, JSON + validation + acknowledgment)",
        &samples.edi,
    )?;
    demo_parse_ack(
        samples,
        "Parse: parse (mode 3, invalid file, JSON + validation + acknowledgment)",
        &samples.edi_invalid,
    )?;
    demo_split(samples)?;
    demo_build(&parsed_json)?;
    demo_merge(&parsed_json)?;
    demo_iterators(samples, &parsed_json)?;
    demo_teardown()
}

struct Args {
    serial: Option<String>,
    lib: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        serial: None,
        lib: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| iter.next())
                .ok_or(format!("{name} needs a value"))
        };
        match name.as_str() {
            "--serial" => args.serial = Some(value()?),
            "--lib" => args.lib = Some(PathBuf::from(value()?)),
            "-h" | "--help" => {
                println!("ediFabric Native X12: every ABI function");
                println!(
                    "  --serial  license serial (default: EDIFABRIC_SERIAL or the free plan serial)"
                );
                println!("  --lib     path to edifabric-x12-tools.dll/.so/.dylib or its folder");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let serial = args
        .serial
        .or_else(|| {
            std::env::var("EDIFABRIC_SERIAL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_SERIAL.to_string());
    let samples = match Samples::load() {
        Ok(samples) => samples,
        Err(error) => {
            eprintln!("could not read the sample files: {error}");
            return ExitCode::FAILURE;
        }
    };

    section("Load: load_library");
    match ef::load_library(args.lib.as_deref()) {
        Ok(path) => println!("  loaded {}", path.display()),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    }

    let result = demo_errors()
        .and_then(|_| demo_logging(Path::new("edifabric.log")))
        .and_then(|_| run_demos(&samples, &serial));

    // Release the log file. On the success path demo_teardown already shut the logger down.
    let _ = ef::shutdown_logger();

    if let Err(error) = result {
        eprintln!("\n{error}");
        eprintln!(
            "Check that the serial is valid and that map/map.json resolves the \
             transaction set to a local model file under map/."
        );
        return ExitCode::FAILURE;
    }

    section("Finished");
    println!("Every entry point in c-abi-edifabric_x12_tools.h was called.");
    ExitCode::SUCCESS
}
