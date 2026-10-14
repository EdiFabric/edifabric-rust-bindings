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
}
