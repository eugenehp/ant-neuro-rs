# Download eego SDK vendor libraries with SHA-256 integrity verification.
# PowerShell version for native Windows (no Git Bash / WSL required).
#
# Usage:
#   .\scripts\download-sdk.ps1
#   $env:EEGO_SDK_BRANCH="v5.12.0"; .\scripts\download-sdk.ps1

param(
    [string]$Branch = "master",
    [string]$Repo = "brainflow-dev/brainflow",
    [switch]$SkipHash,
    [switch]$All  # download all platforms
)

$ErrorActionPreference = "Stop"
$FallbackCommit = "f4953923a9737d0dcd6f76a0aecc3fa333431f06"
$LibDir = Join-Path (Split-Path $PSScriptRoot) "lib"
New-Item -ItemType Directory -Force -Path $LibDir | Out-Null

# Known-good SHA-256 hashes (eego SDK v1.3.29, build 57168)
$Hashes = @{
    "libeego-SDK.so"  = "882867b584ceb52c5b12bc276430115ffb28c90b50884ee685073dfae473a94b"
    "eego-SDK.dll"    = "fe22d1e754b9545340ed1f57a2a16b8ea01a199ac4bc8d28c6a09d3868be9809"
    "eego-SDK32.dll"  = "c5b306edb7538cce03f81711c2282768f5221a7e41d593367889fbc7dbb660f2"
}

function Get-SdkFile {
    param([string]$Ref, [string]$Path, [string]$Name)
    $Url = "https://raw.githubusercontent.com/$Repo/$Ref/third_party/ant_neuro/$Path"
    $Dest = Join-Path $LibDir $Name

    if (Test-Path $Dest) {
        $size = (Get-Item $Dest).Length
        Write-Host "  + $Name (already exists, $([math]::Round($size/1KB)) KB)"
    } else {
        Write-Host "  > $Name"
        try {
            Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing
            $size = (Get-Item $Dest).Length
            if ($size -lt 10000) {
                Write-Host "  x $Name (received $size bytes - likely a 404 page)" -ForegroundColor Red
                Remove-Item $Dest -Force
                throw "Download failed for $Name (response too small)"
            }
            Write-Host "  + $Name ($([math]::Round($size/1KB)) KB)"
        } catch {
            Write-Host "  x $Name (download failed: $_)" -ForegroundColor Red
            if (Test-Path $Dest) { Remove-Item $Dest -Force }
            throw
        }
    }

    # Verify hash
    if (-not $SkipHash -and $Hashes.ContainsKey($Name)) {
        $expected = $Hashes[$Name]
        $actual = (Get-FileHash -Path $Dest -Algorithm SHA256).Hash.ToLower()
        if ($actual -eq $expected) {
            Write-Host "    [locked] SHA-256 verified: $($actual.Substring(0,16))..."
        } else {
            Write-Host "    [MISMATCH] SHA-256 does not match!" -ForegroundColor Red
            Write-Host "      expected: $expected"
            Write-Host "      got:      $actual"
            Remove-Item $Dest -Force
            throw "Hash verification failed for $Name"
        }
    }
}

function Do-Download {
    param([string]$Ref)
    if ($All) {
        Get-SdkFile $Ref "linux/libeego-SDK.so" "libeego-SDK.so"
        Get-SdkFile $Ref "windows/eego-SDK.dll" "eego-SDK.dll"
        Get-SdkFile $Ref "windows/eego-SDK32.dll" "eego-SDK32.dll"
    } else {
        Get-SdkFile $Ref "windows/eego-SDK.dll" "eego-SDK.dll"
        Get-SdkFile $Ref "windows/eego-SDK32.dll" "eego-SDK32.dll"
    }
}

Write-Host "Downloading eego SDK vendor libraries..."
Write-Host "  Source: $Repo"
Write-Host ""

Write-Host "  Trying $Branch..."
try {
    Do-Download $Branch
} catch {
    Write-Host ""
    Write-Host "  Warning: $Branch failed -- falling back to known-good commit $($FallbackCommit.Substring(0,12))..." -ForegroundColor Yellow
    Write-Host ""
    Do-Download $FallbackCommit
}

Write-Host ""
Write-Host "Done. Libraries in: $LibDir"
Get-ChildItem $LibDir -Filter "*.dll" | ForEach-Object { Write-Host "  $($_.Name) ($([math]::Round($_.Length/1KB)) KB)" }
Get-ChildItem $LibDir -Filter "*.so"  | ForEach-Object { Write-Host "  $($_.Name) ($([math]::Round($_.Length/1KB)) KB)" }
