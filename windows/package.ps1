<#
Builds the Windows app as one signed .msixbundle holding an arm64 and an x64
package: Tomoshibi-<version>.msixbundle, what the deployment's download
shelf and an update hand out (the shelf's file ends .msixbundle).

    windows\build-core.ps1 -Arch arm64; windows\build-core.ps1 -Arch x64
    windows\package.ps1 -Version 0.1.0.2 -Thumbprint <certificate> -Out C:\out

The core for each architecture is whatever build-core.ps1 last built there:
`release` for distribution, which talks to the home deployment alone, and
`debug` with -Configuration Debug for a package a test can point at a local
deployment (Launch reads its switches from a file there, since a package is
started without an environment).

The version is written into a copy of Package.appxmanifest for the build,
and the publisher is the signing certificate's subject: a package installs
over an earlier one only from the same publisher, and the app checks the
same before installing an update. The certificate is found by its
thumbprint in the current user's store, so no password is ever on a command
line. What the shelf's apps.json needs (size and SHA-256) is printed last.
#>
param(
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')] [string] $Version,
    [Parameter(Mandatory)] [string] $Thumbprint,
    [ValidateSet("Debug", "Release")] [string] $Configuration = "Release",
    [string] $Out = (Join-Path $PSScriptRoot "..\out"),
    [string[]] $Arch = @("arm64", "x64")
)
$ErrorActionPreference = "Stop"

$project = Join-Path $PSScriptRoot "Tomoshibi"
$manifest = Join-Path $project "Package.appxmanifest"
$certificate = Get-Item "Cert:\CurrentUser\My\$Thumbprint"
$publisher = $certificate.Subject

# The SDK's tools, from the build tools package the project already uses.
$packages = if ($env:NUGET_PACKAGES) { $env:NUGET_PACKAGES } else { Join-Path $env:USERPROFILE ".nuget\packages" }
$host_ = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
$tools = Get-ChildItem (Join-Path $packages "microsoft.windows.sdk.buildtools") -Recurse -Filter makeappx.exe |
    Where-Object { $_.Directory.Name -eq $host_ } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $tools) { throw "no makeappx.exe for $host_ under $packages" }
$makeappx = $tools.FullName
$signtool = Join-Path $tools.Directory.FullName "signtool.exe"

$staging = Join-Path $Out "staging-$Version"
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force $staging | Out-Null

$original = Get-Content -Raw $manifest
try {
    $written = $original -replace '(<Identity [^>]*Version=")[^"]+(")', "`${1}$Version`${2}" `
                         -replace '(<Identity [^>]*Publisher=")[^"]+(")', "`${1}$publisher`${2}"
    Set-Content -NoNewline -Encoding UTF8 $manifest $written

    foreach ($a in $Arch) {
        $platform = @{ arm64 = "ARM64"; x64 = "x64" }[$a]
        $packageDir = (Join-Path $Out "packages-$a") + "\"
        if (Test-Path $packageDir) { Remove-Item -Recurse -Force $packageDir }
        & dotnet build (Join-Path $project "Tomoshibi.csproj") -c $Configuration -r "win-$a" -p:Platform=$platform `
            -p:WindowsPackageType=MSIX -p:EnableMsixTooling=true -p:GenerateAppxPackageOnBuild=true -p:AppxPackageSigningEnabled=false `
            -p:AppxBundle=Never -p:UapAppxPackageBuildMode=SideloadOnly -p:AppxPackageDir=$packageDir `
            "-p:AssemblyVersion=$Version" "-p:FileVersion=$Version" -nologo -clp:NoSummary
        if ($LASTEXITCODE -ne 0) { throw "the $a package did not build" }
        $msix = Get-ChildItem $packageDir -Recurse -Filter *.msix | Select-Object -First 1
        if (-not $msix) { throw "no .msix for $a under $packageDir" }
        Copy-Item $msix.FullName (Join-Path $staging "Tomoshibi_${Version}_$a.msix")
    }
} finally {
    Set-Content -NoNewline -Encoding UTF8 $manifest $original
}

$bundle = Join-Path $Out "Tomoshibi-$Version.msixbundle"
& $makeappx bundle /o /bv $Version /d $staging /p $bundle | Out-Null
if ($LASTEXITCODE -ne 0) { throw "the bundle did not build" }
& $signtool sign /fd SHA256 /sha1 $Thumbprint /s My $bundle | Out-Null
if ($LASTEXITCODE -ne 0) { throw "the bundle was not signed" }

$file = Get-Item $bundle
$hash = (Get-FileHash -Algorithm SHA256 $bundle).Hash.ToLowerInvariant()
"built $($file.Name), $($file.Length) bytes, sha256 $hash, publisher $publisher"
