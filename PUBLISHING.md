# Publishing to crates.io

The bindings are published as five crates with the same version:

| Crate | Contents | Used on |
| --- | --- | --- |
| `edifabric-x12` | the bindings; the default `bundled` feature depends on the crate below for the target | every platform |
| `edifabric-x12-native-win-x64` | `edifabric-x12-tools.dll` | Windows x64 |
| `edifabric-x12-native-linux-x64` | `edifabric-x12-tools.so` | Linux x64, glibc 2.34+ |
| `edifabric-x12-native-linux-arm64` | `edifabric-x12-tools.so` | Linux arm64, glibc 2.34+ |
| `edifabric-x12-native-osx-arm64` | `edifabric-x12-tools.dylib` | macOS 12+ on Apple silicon |

Cargo downloads and builds only the platform crate for the target. It embeds the library with
`include_bytes!`, and `edifabric-x12` writes it to `<temp>/edifabric-x12/<version>-<hash>/` and
loads it when no other library is found. The libraries come from the public downloads at
`https://www.edifabric.com/download/`, so publish a release only after the native build for that
version is uploaded there.

| File | Purpose |
| --- | --- |
| `Cargo.toml` | the workspace version shared by all five crates, and the `bundled` feature |
| `native/<rid>/` | the platform crates; their library file is copied in at release time and not committed |
| `scripts/fetch-natives.ps1`, `.sh` | download the four libraries into `natives/<rid>/` |
| `scripts/stage-natives.sh` | copy them into `native/<rid>/` (`release.ps1` does this itself) |
| `scripts/smoke-test/` | a consumer crate that checks the bundled library loads |
| `scripts/release.ps1`, `.sh` | fetch, package, smoke-test, and publish from your machine |
| `.github/workflows/publish-crates.yml` | the same pipeline in GitHub Actions, with a smoke test on every platform |

A platform crate built without its library (for example from a fresh clone) compiles with a
warning and bundles nothing; the release scripts and workflow always stage the libraries first.

## One-time setup

1. Sign in to [crates.io](https://crates.io) with the EdiFabric GitHub account and verify the
   account email (**Account Settings**), which crates.io requires before publishing.
2. Create an API token (**Account Settings > API Tokens**) with the scopes `publish-new` and
   `publish-update`. The first release creates the five crates, so it must use a token: add it to
   the GitHub repository as the secret `CARGO_REGISTRY_TOKEN`, add the repository variable
   `USE_CARGO_TOKEN` = `true` (**Settings > Secrets and variables > Actions**), and create the
   environment `crates-io` (**Settings > Environments**), optionally with required reviewers.
3. After the first release, switch each crate to
   [trusted publishing](https://crates.io/docs/trusted-publishing): open the crate on crates.io,
   **Settings > Trusted Publishing > Add**, GitHub, owner `EdiFabric`, repository
   `edifabric-rust-bindings`, workflow `publish-crates.yml`, environment `crates-io`. Then delete
   the `USE_CARGO_TOKEN` variable and the `CARGO_REGISTRY_TOKEN` secret, and revoke the token.
4. To let others publish, add them as owners: `cargo owner --add <github-user> <crate>` for each
   crate.
5. For publishing from your machine only: run `cargo login` with a token.

## Release

1. Upload the new native build to `https://www.edifabric.com/download/`.
2. In `Cargo.toml`, set `workspace.package.version` and the four `=x.y.z` platform dependency
   versions to the native library version, for example `1.1.1`. Update the version in
   `README.md`, run `cargo update --workspace`, and commit.
3. Validate the crates:
   - GitHub: **Actions > Publish to crates.io > Run workflow** with **publish** cleared
   - or locally: `./scripts/release.ps1 -DryRun` (Windows) or `scripts/release.sh --dry-run`
4. Create a GitHub release with the tag `v<version>`, for example `v1.1.1`. Publishing the
   release runs the workflow; it fails if the tag and the version differ. Locally, the equivalent
   is `./scripts/release.ps1` or `scripts/release.sh`.

`cargo publish --workspace` uploads the platform crates before `edifabric-x12`. crates.io never
accepts the same version twice; if a release fails partway, rerun it with only the remaining
crates (`cargo publish -p <crate>`), or bump the version. A bad version can be yanked with
`cargo yank --version <x.y.z> <crate>`, which stops new projects from picking it up.
