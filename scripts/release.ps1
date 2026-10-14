<#
.SYNOPSIS
Package the bindings and the four platform crates, smoke-test them, and publish them to crates.io.

.DESCRIPTION
Publishing needs a crates.io API token in CARGO_REGISTRY_TOKEN or from `cargo login`.
-DryRun packages and verifies every crate without uploading.

.EXAMPLE
./scripts/release.ps1 -DryRun
./scripts/release.ps1
#>
param(
    [switch]$DryRun,
    [switch]$SkipFetch
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Invoke-Checked {
    # Native tools write progress to stderr; only the exit code decides failure.
    $ErrorActionPreference = 'Continue'
    & $args[0] $args[1..($args.Count - 1)] 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE -ne 0) { throw "$($args -join ' ') failed with exit code $LASTEXITCODE" }
}

if (-not $SkipFetch) { & "$PSScriptRoot/fetch-natives.ps1" -Destination natives }
foreach ($rid in 'win-x64', 'linux-x64', 'linux-arm64', 'osx-arm64') {
    $library = Get-ChildItem "natives/$rid" -File | Where-Object Extension -ne '.asc'
    if (-not $library) { throw "natives/$rid has no library; run scripts/fetch-natives.ps1" }
    Copy-Item $library.FullName "native/$rid/"
}

Invoke-Checked cargo publish --workspace --locked --dry-run --allow-dirty

$smoke = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory $smoke | Out-Null
try {
    $manifest = Resolve-Path scripts/smoke-test/Cargo.toml
    Push-Location $smoke
    try { Invoke-Checked cargo run --quiet --manifest-path $manifest }
    finally { Pop-Location }
}
finally {
    Remove-Item $smoke -Recurse -Force -ErrorAction SilentlyContinue
}

if ($DryRun) {
    Write-Host 'Dry run: packaged and verified 5 crates, nothing published.'
    return
}
Invoke-Checked cargo publish --workspace --locked
