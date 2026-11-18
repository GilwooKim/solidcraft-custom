[CmdletBinding()]
param([string]$Iscc, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$version = (Get-Content (Join-Path $PSScriptRoot 'custom-version.txt') -Raw).Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Use a stable major.minor.patch version.' }
if (-not $Iscc) {
    $Iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (-not $Iscc) {
        $compiler = Get-ChildItem "${env:ProgramFiles(x86)}/Inno Setup */ISCC.exe" -ErrorAction SilentlyContinue |
            Sort-Object FullName | Select-Object -Last 1
        if ($compiler) { $Iscc = $compiler.FullName }
    }
}
if (-not $Iscc -or -not (Test-Path -LiteralPath $Iscc)) { throw 'Install Inno Setup, or pass -Iscc with its ISCC.exe path.' }
$previousVersion = $env:SOLIDCRAFT_VERSION
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location $repoRoot
try {
    $env:SOLIDCRAFT_VERSION = $version
    $env:CARGO_TARGET_DIR = Join-Path $repoRoot 'target'
    if (-not $SkipBuild) {
        # This distribution targets Windows desktop PCs, including the CLI. Share the desktop
        # feature set and optimized kernel; upstream's separate CLI build targets Server Core too.
        & cargo build --locked --profile ci -p solidcraft -p solidcraft-cli
        if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
    }
    # Each run owns a fresh staging directory: a same-version rebuild must never ship stale files.
    $stage = Join-Path $repoRoot "target/custom-package/$version-$([Guid]::NewGuid().ToString('N'))"
    $dist = Join-Path $repoRoot "dist/custom/$version"
    New-Item -ItemType Directory -Force -Path $stage,$dist | Out-Null
    foreach ($exe in 'solidcraft.exe','solidcraft-cli.exe') {
        Copy-Item -LiteralPath (Join-Path $repoRoot "target/ci/$exe") -Destination $stage
        # SkipBuild may reuse binaries from a different release. Check the copied payload too,
        # so the installer, portable archive and executable resources agree on their version.
        $payloadVersion = (Get-Item -LiteralPath (Join-Path $stage $exe)).VersionInfo
        if ($exe -eq 'solidcraft.exe' -and ($payloadVersion.ProductVersion -ne $version -or $payloadVersion.FileVersion -ne $version)) {
            throw "$exe does not match package version $version (product: $($payloadVersion.ProductVersion), file: $($payloadVersion.FileVersion)). Rebuild before packaging."
        }
    }
    foreach ($license in 'LICENSE-MIT','LICENSE-APACHE','NOTICE','THIRD-PARTY-LICENSES.txt') {
        Copy-Item -LiteralPath (Join-Path $repoRoot $license) -Destination $stage
    }
    # App-local VC runtime: staff machines do not need developer tools or an elevated redist install.
    $runtime = Get-ChildItem -Path @(
        "${env:ProgramFiles(x86)}/Microsoft Visual Studio/*/*/VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT",
        "$env:ProgramFiles/Microsoft Visual Studio/*/*/VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT"
    ) -Directory -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
    if (-not $runtime) { throw 'Visual C++ x64 redistributable runtime files were not found.' }
    Copy-Item -Path (Join-Path $runtime.FullName '*.dll') -Destination $stage
    # The CLI has no Windows version resource; query it with the staged app-local runtime.
    $cliVersion = & (Join-Path $stage 'solidcraft-cli.exe') --version
    if ($LASTEXITCODE -ne 0 -or ($cliVersion -join "`n").Trim() -ne "solidcraft-cli $version") {
        throw "solidcraft-cli.exe does not report package version $version. Rebuild before packaging."
    }
    # The updater selects this fixed asset name and checks SHA256SUMS.txt first.
    $portable = Join-Path $dist 'solidcraft-windows-x64-portable.zip'
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $portable -Force
    & $Iscc "/DBuildVersion=$version" "/DStageDir=$stage" "/DOutputDir=$dist" (Join-Path $PSScriptRoot 'custom.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    $installer = Join-Path $dist "solidcraft-$version-windows-x64-setup.exe"
    $sums = foreach ($file in $portable,$installer) {
        $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $([IO.Path]::GetFileName($file))"
    }
    [IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'), $sums, [Text.UTF8Encoding]::new($false))
    Get-Item -LiteralPath $portable,$installer
} finally {
    $env:SOLIDCRAFT_VERSION = $previousVersion
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
