<#
Builds the core for Windows as tomoshibi_core.dll, generates the C# bindings
from the library that was actually built, and puts both where the app
project expects them.

    windows\build-core.ps1                         # debug, arm64
    windows\build-core.ps1 -Profile local          # optimised, talks to the server it is given
    windows\build-core.ps1 -Profile release -Arch x64

As on Apple and Android, nothing generated is committed: the bindings and the
library are only ever what this script last produced from the core beside it.

The profiles are the other platforms': `release` admits only the home
deployment (protocol api::admitted, gated on debug_assertions), so anything
pointed at a local deployment is `debug` or `local`.
#>
param(
    [ValidateSet("debug", "release", "local")] [string] $Profile = "debug",
    [ValidateSet("arm64", "x64")] [string] $Arch = "arm64"
)
$ErrorActionPreference = "Stop"

$here = $PSScriptRoot
$core = (Resolve-Path (Join-Path $here "..\core")).Path
$triple = @{ arm64 = "aarch64-pc-windows-msvc"; x64 = "x86_64-pc-windows-msvc" }[$Arch]
$flag = @{ debug = @(); release = @("--release"); local = @("--profile", "local") }[$Profile]

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
# ring compiles its C with clang when it targets Windows on Arm (its
# build.rs), and an MSVC-only machine has none.
$llvm = "$env:ProgramFiles\LLVM\bin"
if (Test-Path $llvm) { $env:PATH = "$llvm;$env:PATH" }

# UniFFI 0.32 has no released C# generator; NordSecurity's latest release
# (v0.11.1) is built on 0.31, whose bindings refuse a 0.32 library by its
# checksums. This is the open upstream pull request that moves the generator
# to 0.32 (NordSecurity/uniffi-bindgen-cs#176), pinned to one commit and built
# here, outside the repository's history. Move to the first release made on
# 0.32 when there is one.
$generatorRepo = "https://github.com/dennisameling/uniffi-bindgen-cs"
$generatorRev = "cf0aa7a8fb59358a846708730a58a02bf206ac05"
$tools = Join-Path $here ".tools"
$generator = Join-Path $tools "bin\uniffi-bindgen-cs.exe"
$stamp = Join-Path $tools "rev.txt"
if (-not (Test-Path $generator) -or (Get-Content -ErrorAction SilentlyContinue $stamp) -ne $generatorRev) {
    # Built for this machine, before anything below sets up a cross build.
    cargo install uniffi-bindgen-cs --git $generatorRepo --rev $generatorRev --locked --force --root $tools
    if ($LASTEXITCODE -ne 0) { throw "the C# binding generator did not build" }
    Set-Content -Path $stamp -Value $generatorRev
}

Push-Location $core
try {
    # A DLL for Windows only, asked for here rather than in Cargo.toml: a
    # cdylib there would be linked for every Apple target too, and the
    # staticlib Cargo.toml asks for would bundle all of WebRTC into a second
    # copy nobody links.
    cargo rustc --lib --crate-type cdylib --target $triple @flag
    if ($LASTEXITCODE -ne 0) { throw "the core did not build" }
} finally {
    Pop-Location
}

$built = Join-Path $core "target\$triple\$Profile"
$dll = Join-Path $built "tomoshibi_core.dll"

$app = Join-Path $here "Tomoshibi"
$generated = Join-Path $app "Generated"
if (Test-Path $generated) { Remove-Item -Recurse -Force $generated }
New-Item -ItemType Directory -Force $generated | Out-Null
# Run from the core, whose cargo metadata the generator reads for each
# crate's own configuration.
Push-Location $core
try {
    & $generator --library $dll --out-dir $generated --no-format
    if ($LASTEXITCODE -ne 0) { throw "the C# bindings were not generated" }
} finally {
    Pop-Location
}

$native = Join-Path $app "Native\win-$Arch"
New-Item -ItemType Directory -Force $native | Out-Null
Copy-Item -Force $dll $native
$pdb = Join-Path $built "tomoshibi_core.pdb"
if (Test-Path $pdb) { Copy-Item -Force $pdb $native }
Write-Output "built $Profile core for $Arch into $app"
