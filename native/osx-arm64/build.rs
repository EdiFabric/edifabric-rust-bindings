const FILE: &str = "edifabric-x12-tools.dylib";

fn main() {
    println!("cargo::rerun-if-changed={FILE}");
    println!("cargo::rustc-check-cfg=cfg(has_library)");
    if std::path::Path::new(FILE).is_file() {
        println!("cargo::rustc-cfg=has_library");
    } else {
        println!("cargo::warning={FILE} is not bundled; run scripts/fetch-natives and scripts/release --dry-run to add it");
    }
}
