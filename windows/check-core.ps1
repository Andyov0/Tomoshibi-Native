<#
Refuses a core that is not one to hand anybody, and says why.

    windows\check-core.ps1 <path to tomoshibi_core.dll>

build-core.ps1 builds a debug core unless told -Profile release, and a
Release app around a debug or local core built and ran as well as any: the
only difference was that the core admitted whatever deployment a link named
(protocol api::admitted keys on debug assertions) and printed its relay
timings. A checkout with no deployment.local builds for meet.example.com and
reaches nothing, and that built cleanly too; and a core with no update key
installs whatever build the deployment hands it. The core writes what it is
into itself (protocol api BUILD_MARK: profile, host, update key), and this
reads it out of the library. Where this checkout names a host or an update
key (TOMOSHIBI_HOST and TOMOSHIBI_UPDATE_KEY in the environment, else in
deployment.local), the core must have been built with the same. setup.ps1
and package.ps1 call it for every Release build.
#>
param([Parameter(Mandatory)] [string] $Library)
$ErrorActionPreference = "Stop"

if (-not (Test-Path $Library)) { throw "check-core: no core at $Library" }
# Byte for character, so that the mark is found wherever it lies.
$latin = [System.Text.Encoding]::GetEncoding(28591)
$text = $latin.GetString([System.IO.File]::ReadAllBytes($Library))
$marks = @([regex]::Matches($text, 'tomoshibi-core-build:[a-z]*:[a-z0-9.-]*:[0-9a-f]*;') | ForEach-Object { $_.Value } | Sort-Object -Unique)
if ($marks.Count -ne 1) {
    throw "check-core: $Library carries $($marks.Count) core build marks, not one; is it older than the mark?"
}

$parts = $marks[0].TrimEnd(';').Split(':')
$profile_ = $parts[1]
$builtFor = $parts[2]
$key = $parts[3]
if ($profile_ -ne "release") {
    throw "check-core: $Library is a $profile_ core, which admits any deployment; run windows\build-core.ps1 -Profile release and build again"
}
if ($builtFor -eq "meet.example.com") {
    throw "check-core: $Library is built for meet.example.com, which reaches nothing; set TOMOSHIBI_HOST or deployment.local (see deployment.example) and build the core again"
}
if ($key.Length -ne 64) {
    throw "check-core: $Library has no update key and would install any build the deployment hands it; set TOMOSHIBI_UPDATE_KEY (see deployment.example) and build the core again"
}

# What this checkout says, as the core's build reads it: the environment
# first, then deployment.local, `// comments` allowed.
$local = Join-Path $PSScriptRoot "..\deployment.local"
function Said([string]$name) {
    $value = [Environment]::GetEnvironmentVariable($name)
    if (-not $value -and (Test-Path $local)) {
        foreach ($line in Get-Content $local) {
            $line = ($line -split '//', 2)[0]
            $pair = $line -split '=', 2
            if ($pair.Count -eq 2 -and $pair[0].Trim() -eq $name -and $pair[1].Trim()) { $value = $pair[1].Trim(); break }
        }
    }
    if ($value) { $value.Trim().ToLowerInvariant() } else { "" }
}
$wantHost = Said "TOMOSHIBI_HOST"
$wantKey = Said "TOMOSHIBI_UPDATE_KEY"
if ($wantHost -and $wantHost -ne $builtFor) {
    throw "check-core: $Library is built for $builtFor, and this checkout is for $wantHost"
}
if ($wantKey -and $wantKey -ne $key) {
    throw "check-core: $Library holds updates to another key than this checkout's TOMOSHIBI_UPDATE_KEY"
}
"core: release, for $builtFor, updates held to its key"
