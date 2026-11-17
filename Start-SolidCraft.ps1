[CmdletBinding()]
param(
    [switch]$Sample,
    [switch]$BuildOnly
)

$ErrorActionPreference = 'Stop'
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location $PSScriptRoot
try {
    $env:CARGO_TARGET_DIR = Join-Path $PSScriptRoot 'target'
    & cargo build --locked --profile ci -p solidcraft
    if ($LASTEXITCODE -ne 0) {
        throw "SolidCraft build failed (exit code $LASTEXITCODE)."
    }
    if (-not $BuildOnly) {
        $executable = Join-Path $env:CARGO_TARGET_DIR 'ci/solidcraft.exe'
        if ($Sample) {
            Start-Process -FilePath $executable -WorkingDirectory $PSScriptRoot -ArgumentList '--sample' -WindowStyle Normal
        } else {
            Start-Process -FilePath $executable -WorkingDirectory $PSScriptRoot -WindowStyle Normal
        }
    }
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
