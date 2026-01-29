# Windows build: set OPENSSL_DIR then cargo build (avoids openssl-sys source build failure).
# Usage: .\build-windows.ps1 [--release]

$ErrorActionPreference = "Stop"

if (-not $env:OPENSSL_DIR) {
    $candidates = @(
        "C:\Program Files\OpenSSL-Win64",
        "C:\Program Files\OpenSSL",
        "C:\OpenSSL-Win64",
        "C:\OpenSSL"
    )
    foreach ($dir in $candidates) {
        $h = Join-Path $dir "include\openssl\ssl.h"
        if (Test-Path $h) {
            $env:OPENSSL_DIR = $dir
            Write-Host "Using OPENSSL_DIR=$dir"
            break
        }
    }
    if (-not $env:OPENSSL_DIR) {
        Write-Host "OpenSSL not found in: $($candidates -join ', ')"
        Write-Host ""
        Write-Host "Option 1 - Install Win64 OpenSSL (default path):"
        Write-Host "  https://slproweb.com/products/Win32OpenSSL.html"
        Write-Host "  Download 'Win64 OpenSSL v3.x' and install to default location, then run this script again."
        Write-Host ""
        Write-Host "Option 2 - If already installed elsewhere, set OPENSSL_DIR and run again:"
        Write-Host "  `$env:OPENSSL_DIR = 'C:\path\to\your\OpenSSL-Win64'"
        Write-Host "  .\build-windows.ps1"
        exit 1
    }
}

$release = ($args -contains '--release')
if ($release) {
    cargo build --release
} else {
    cargo build
}
