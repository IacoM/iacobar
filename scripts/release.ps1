# Builds a release for GitHub and the Microsoft Store, and with -Publish
# publishes it on both. See "Publishing a release" in the README.
#
#   powershell -File scripts\release.ps1 -Notes "What's new"            # build only
#   powershell -File scripts\release.ps1 -Notes "What's new" -Publish   # build + publish
#
# Output in .\release:
#   Iacobar_<v>_x64-setup.exe + latest.json   GitHub release (in-app updater)
#   Iacobar_<v>_x64.msix                      Microsoft Store (the Store signs it)

param(
    # Shown in the app next to "Aggiorna ora" and used as the GitHub release notes.
    [string]$Notes = "",
    # Create the GitHub release (tag v<version>) and send the MSIX to the Store.
    [switch]$Publish,
    # Private key made with `npx tauri signer generate`, never committed.
    [string]$Key = "$HOME\.tauri\iacobar.key"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

function Need($cmd, $hint) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { throw "$cmd not found: $hint" }
}

if (-not (Test-Path $Key)) { throw "Signing key not found: $Key" }
$store = Get-Content "msix\store.json" -Raw | ConvertFrom-Json
$conf = Get-Content "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$version = $conf.version
$tag = "v$version"
$exe = "Iacobar_${version}_x64-setup.exe"
$msix = "Iacobar_${version}_x64.msix"

if ($Publish) {
    # check everything BEFORE a 5 minute build
    Need "gh" "install GitHub CLI (winget install GitHub.cli), then: gh auth login"
    Need "msstore" "install the Microsoft Store Developer CLI, then: msstore reconfigure (see README)"
    if (-not $store.productId) { throw "msix\store.json: productId is empty (see README)" }
    if (git status --porcelain) { throw "Uncommitted changes: commit them first" }
    git fetch -q origin
    if ((git rev-parse HEAD) -ne (git rev-parse origin/main)) { throw "Push main first: the release tag must point to it" }
    gh release view $tag *> $null
    if ($LASTEXITCODE -eq 0) { throw "Release $tag already exists: bump the version first" }
}

# ---------------------------------------------------------------- build --

Write-Host "Building Iacobar $version..." -ForegroundColor Cyan
# the build signs the installer for the updater with this key (no password)
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $Key -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "Build failed" }

$bundle = "src-tauri\target\release\bundle\nsis"
$sig = "$bundle\$exe.sig"
if (-not (Test-Path $sig)) { throw "Missing signature $sig (is createUpdaterArtifacts on?)" }

New-Item -ItemType Directory -Force "release" | Out-Null
Copy-Item "$bundle\$exe" "release\$exe" -Force

# ------------------------------------------------- GitHub: latest.json --

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
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText("$root\release\latest.json", ($latest | ConvertTo-Json -Depth 5), $utf8)

# ---------------------------------------------------- Store: the MSIX --

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName | Select-Object -Last 1
if (-not $sdk) { throw "makeappx.exe not found: install the Windows SDK" }

$stage = "src-tauri\target\release\msix"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force "$stage\Assets" | Out-Null
Copy-Item "src-tauri\target\release\iacobar.exe" $stage
foreach ($logo in "StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png") {
    Copy-Item "src-tauri\icons\$logo" "$stage\Assets\$logo"
}
# placeholders until the name is reserved in Partner Center (fine for a local test)
$identity = if ($store.identityName) { $store.identityName } else { "IacoM.Iacobar" }
$publisher = if ($store.publisher) { $store.publisher } else { "CN=IacoM" }
$manifest = Get-Content "msix\AppxManifest.xml" -Raw
$manifest = $manifest.Replace("{{IDENTITY_NAME}}", $identity).Replace("{{PUBLISHER}}", $publisher)
$manifest = $manifest.Replace("{{PUBLISHER_DISPLAY_NAME}}", $store.publisherDisplayName).Replace("{{VERSION}}", $version)
[System.IO.File]::WriteAllText("$root\$stage\AppxManifest.xml", $manifest, $utf8)

& $sdk.FullName pack /o /d $stage /p "release\$msix" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makeappx failed" }

Write-Host ""
Write-Host "Built in release\:" -ForegroundColor Green
Write-Host "  $exe + latest.json   (GitHub)"
Write-Host "  $msix   (Microsoft Store)"
if (-not $store.identityName) { Write-Host "  (MSIX with placeholder identity: fill msix\store.json for the Store)" -ForegroundColor Yellow }

if (-not $Publish) {
    Write-Host ""
    Write-Host "Not published. Add -Publish to release it on GitHub and the Store."
    exit 0
}

# -------------------------------------------------------------- publish --

Write-Host ""
Write-Host "Publishing $tag on GitHub..." -ForegroundColor Cyan
gh release create $tag "release\$exe" "release\latest.json" --target main --title $tag --notes $(if ($Notes) { $Notes } else { $tag }) --latest
if ($LASTEXITCODE -ne 0) { throw "GitHub release failed" }

Write-Host "Sending $msix to the Microsoft Store..." -ForegroundColor Cyan
msstore publish "release\$msix" -id $store.productId
if ($LASTEXITCODE -ne 0) { throw "Store submission failed (the GitHub release is out: retry with msstore publish)" }

Write-Host ""
Write-Host "Done: $tag is on GitHub; the Store version is in certification (usually 1-3 days)." -ForegroundColor Green
