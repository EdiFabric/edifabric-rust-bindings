#!/usr/bin/env bash
# Package the bindings and the four platform crates, smoke-test them, and publish them to crates.io.
#
# Publishing needs a crates.io API token in CARGO_REGISTRY_TOKEN or from `cargo login`.
#
#   scripts/release.sh --dry-run   package and verify every crate without uploading
#   scripts/release.sh             publish
set -euo pipefail
cd "$(dirname "$0")/.."

dry_run=false skip_fetch=false
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=true ;;
        --skip-fetch) skip_fetch=true ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

# Cargo counts the staged (git-ignored) libraries as dirty, so publishing needs --allow-dirty;
# check here instead that nothing else is uncommitted.
if ! $dry_run && [ -n "$(git status --porcelain)" ]; then
    echo "Commit or stash your changes first." >&2
    exit 1
fi

$skip_fetch || bash scripts/fetch-natives.sh natives
bash scripts/stage-natives.sh

cargo publish --workspace --locked --dry-run --allow-dirty

manifest="$PWD/scripts/smoke-test/Cargo.toml"
smoke=$(mktemp -d)
trap 'rm -rf "$smoke"' EXIT
(cd "$smoke" && cargo run --quiet --manifest-path "$manifest")

if $dry_run; then
    echo "Dry run: packaged and verified 5 crates, nothing published."
    exit 0
fi
cargo publish --workspace --locked --allow-dirty
