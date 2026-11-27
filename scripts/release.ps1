# Builds a signed release and the `latest.json` the in-app updater reads.
#
#   powershell -File scripts\release.ps1 -Notes "What's new in this version"
#
# Output in .\release: the installer and latest.json, the two files to attach
# to the GitHub release (tag v<version>). See "Publishing a release" in the README.

param(
    # Shown in the app next to "Aggiorna ora" (one line or a few short ones).
    [string]$Notes = "",
    # Private key made with `npx tauri signer generate`, never committed.
    [string]$Key = "$HOME\.tauri\iacobar.key"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not (Test-Path $Key)) {
    throw "Signing key not found: $Key"
}

$conf = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$version = $conf.version
$tag = "v$version"
$exe = "Iacobar_${version}_x64-setup.exe"
Write-Host "Building Iacobar $version..." -ForegroundColor Cyan

# the build signs the installer with this key (it has no password)
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $Key -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "Build failed" }

$bundle = "src-tauri\target\release\bundle\nsis"
$sig = "$bundle\$exe.sig"
if (-not (Test-Path $sig)) { throw "Missing signature $sig (is createUpdaterArtifacts on?)" }

New-Item -ItemType Directory -Force "release" | Out-Null
Copy-Item "$bundle\$exe" "release\$exe" -Force

$latest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = (Get-Content $sig -Raw).Trim()
            url       = "https://github.com/IacoM/iacobar/releases/download/$tag/$exe"
        }
    }
}
# UTF-8 without BOM: the updater's JSON parser rejects a BOM
$json = $latest | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText("$root\release\latest.json", $json, (New-Object System.Text.UTF8Encoding $false))

Write-Host ""
Write-Host "Done. Create the GitHub release with tag $tag and attach:" -ForegroundColor Green
Write-Host "  release\$exe"
Write-Host "  release\latest.json"
