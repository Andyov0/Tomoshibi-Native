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
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')] [string] $Version,
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
$file = Get-Item $setup
$hash = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLowerInvariant()
"built $($file.Name), $($file.Length) bytes, sha256 $hash"
