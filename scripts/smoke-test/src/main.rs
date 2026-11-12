//! Checks that edifabric-x12 loads the library embedded by its `bundled` feature.
//! Run it from a folder with no copy of the library.

fn main() {
    let loaded = edifabric_x12::load_library(None).expect("load the bundled library");
    println!("loaded {}", loaded.display());
    assert!(
        loaded.starts_with(std::env::temp_dir().join("edifabric-x12")),
        "expected the library extracted from the crate, got {}",
        loaded.display()
    );
    println!("get_app_version -> {}", edifabric_x12::get_app_version().expect("get_app_version"));

    // The license check uses OpenSSL on Linux, which loading alone doesn't.
    let serial = std::env::var("EDIFABRIC_SERIAL").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "bd96a836feca45cb91c86ee65d281f52".to_string());
    edifabric_x12::ensure_token(&serial, 3600).expect("ensure_token");
    println!("ensure_token -> ok");
}
