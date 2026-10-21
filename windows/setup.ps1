<#
Builds the Windows app as it is given out: Tomoshibi-Setup-<version>.exe, one
unsigned setup program holding the arm64 and the x64 app, which installs the
one the machine runs (installer\Tomoshibi.iss). What the deployment's download
shelf and an update hand out (the shelf's file ends .exe).

    windows\build-core.ps1 -Profile release -Arch arm64
    windows\build-core.ps1 -Profile release -Arch x64
    windows\setup.ps1 -Version 0.1.0.2 -Out C:\out

The core for each architecture is whatever build-core.ps1 last built there:
`release` for distribution, which talks to the home deployment alone. A
release build of the app reads no test switches either, so a setup for a
test is built with -Configuration Debug over a debug or local core.

The version goes into the app (its assembly version, which the user agent and
the update check read) and into the setup's own version stamp, which the app
checks before running a setup it downloaded. Needs Inno Setup 6.3 or later.
What the shelf's apps.json needs (size and SHA-256) is printed last.
#>
param(
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+$')] [string] $Version,
    [ValidateSet("Debug", "Release")] [string] $Configuration = "Release",
    [string] $Out = (Join-Path $PSScriptRoot "..\out"),
    [string] $Iscc = ""
)
$ErrorActionPreference = "Stop"

$project = Join-Path $PSScriptRoot "Tomoshibi\Tomoshibi.csproj"
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

if (-not $Iscc) {
    $Iscc = @(
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Iscc) { throw "no Inno Setup 6 (ISCC.exe); install it or pass -Iscc" }

$dirs = @{}
foreach ($a in "arm64", "x64") {
    $platform = @{ arm64 = "ARM64"; x64 = "x64" }[$a]
    $dir = Join-Path $Out "app-$Version-$a"
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    # Built rather than published: publish runs the XAML compiler's second
    # pass in a way that fails on this project (WMC9999, a null reference in
    # the compiler), where a build is clean, and a self-contained build's
    # output is the whole app, runtime and all.
    & dotnet build $project -c $Configuration -r "win-$a" -p:Platform=$platform --self-contained `
        "-p:AssemblyVersion=$Version" "-p:FileVersion=$Version" "-p:InformationalVersion=$Version" `
        "-p:OutDir=$dir\" -nologo -clp:NoSummary
    if ($LASTEXITCODE -ne 0) { throw "the $a app did not build" }
    foreach ($needed in "Tomoshibi.exe", "tomoshibi_core.dll", "coreclr.dll", "hostfxr.dll", "Microsoft.WindowsAppRuntime.dll") {
        if (-not (Test-Path (Join-Path $dir $needed))) { throw "no $needed in the $a app" }
    }
    # Symbols stay with the build; nobody installing needs them.
    Get-ChildItem $dir -Recurse -Filter *.pdb | Remove-Item -Force
    $dirs[$a] = $dir
}

& $Iscc /Q "/DVersion=$Version" "/DArmDir=$($dirs.arm64)" "/DX64Dir=$($dirs.x64)" "/DOutDir=$Out" (Join-Path $PSScriptRoot "installer\Tomoshibi.iss")
if ($LASTEXITCODE -ne 0) { throw "the setup did not build" }

$setup = Join-Path $Out "Tomoshibi-Setup-$Version.exe"

# Inno Setup pads the strings of the setup's version stamp with spaces to a
# width of its own ("Tomoshibi" and fifty-one spaces). Tomoshibi 0.1.0.2 reads
# the product name there untrimmed and so refused every setup as not the
# version offered: nobody on it could update. The padding is turned to nulls,
# which end each string where Windows reads it while the stamp keeps every
# size it had, so the setup's name and version read as they are.
$bytes = [System.IO.File]::ReadAllBytes($setup)
# Byte for character, to search with IndexOf: the stamp is among the
# loader's resources, at the head of the file.
$latin = [System.Text.Encoding]::GetEncoding(28591)
$head = $latin.GetString($bytes, 0, [Math]::Min($bytes.Length, 8MB))
function Find([string]$text, [int]$from, [int]$to) {
    $head.IndexOf($latin.GetString([System.Text.Encoding]::Unicode.GetBytes($text)), $from, $to - $from, [StringComparison]::Ordinal)
}
$at = Find "VS_VERSION_INFO" 0 $head.Length
if ($at -lt 6) { throw "no version stamp in the setup" }
$start = $at - 6
$end = $start + [BitConverter]::ToUInt16($bytes, $start)
$cleared = 0
foreach ($key in "CompanyName", "FileDescription", "FileVersion", "LegalCopyright", "ProductName", "ProductVersion", "OriginalFileName", "Comments") {
    # A String: its length, its value's length in characters, its type, then
    # the key; the value starts on the next four-byte boundary of the stamp.
    $k = Find "$key`0" $start $end
    if ($k -lt 6) { continue }
    $count = [BitConverter]::ToUInt16($bytes, $k - 4)
    if ($count -eq 0) { continue }
    $v = $k + ($key.Length + 1) * 2
    $v += (4 - (($v - $start) % 4)) % 4
    for ($c = $v + ($count - 1) * 2; $c -ge $v; $c -= 2) {
        if ($bytes[$c + 1] -ne 0) { break }
        if ($bytes[$c] -eq 0) { continue }
        if ($bytes[$c] -ne 0x20) { break }
        $bytes[$c] = 0
        $cleared++
    }
}
[System.IO.File]::WriteAllBytes($setup, $bytes)
$stamp = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($setup)
if ($stamp.ProductName -cne "Tomoshibi" -or $stamp.ProductVersion -cne $Version) {
    throw "the setup's stamp reads '$($stamp.ProductName)' '$($stamp.ProductVersion)'"
}
"stamp unpadded ($cleared spaces): '$($stamp.ProductName)' '$($stamp.ProductVersion)'"

$file = Get-Item $setup
$hash = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLowerInvariant()
"built $($file.Name), $($file.Length) bytes, sha256 $hash"
