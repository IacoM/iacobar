# Regenerates THIRD-PARTY-NOTICES.md from the local dependency metadata
# (Cargo.lock + node_modules). Run from the repository root:
#   powershell -File scripts/third-party-notices.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$out = New-Object System.Text.StringBuilder
[void]$out.AppendLine("# Third-party notices")
[void]$out.AppendLine("")
[void]$out.AppendLine("Iacobar is distributed under the MIT license (see LICENSE). It includes or is")
[void]$out.AppendLine("built with the open-source components listed below, each under its own license.")
[void]$out.AppendLine("The full license texts are included in each package's source distribution")
[void]$out.AppendLine("(crates.io / npm). The Microsoft Edge WebView2 Runtime is not bundled: the")
[void]$out.AppendLine("installer uses the copy that ships with Windows or downloads it from Microsoft.")
[void]$out.AppendLine("")

# ---- Rust crates (everything in Cargo.lock that is not this project) ----
$meta = cargo metadata --format-version 1 --manifest-path src-tauri/Cargo.toml --locked --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
$crates = $meta.packages | Where-Object { $_.name -ne "iacobar" } | Sort-Object name, version
[void]$out.AppendLine("## Rust crates ($($crates.Count))")
[void]$out.AppendLine("")
[void]$out.AppendLine("| Crate | Version | License |")
[void]$out.AppendLine("|---|---|---|")
foreach ($c in $crates) {
  $lic = if ($c.license) { $c.license } elseif ($c.license_file) { "see license file" } else { "unspecified" }
  [void]$out.AppendLine("| $($c.name) | $($c.version) | $lic |")
}
[void]$out.AppendLine("")

# ---- JavaScript packages compiled into the user interface ----
$js = @("svelte", "@tauri-apps/api", "@sveltejs/kit")
[void]$out.AppendLine("## JavaScript packages")
[void]$out.AppendLine("")
[void]$out.AppendLine("| Package | Version | License |")
[void]$out.AppendLine("|---|---|---|")
foreach ($name in $js) {
  $pkg = Join-Path $root "node_modules/$name/package.json"
  if (Test-Path $pkg) {
    $p = Get-Content $pkg -Raw | ConvertFrom-Json
    [void]$out.AppendLine("| $name | $($p.version) | $($p.license) |")
  }
}

[IO.File]::WriteAllText((Join-Path $root "THIRD-PARTY-NOTICES.md"), $out.ToString(), (New-Object Text.UTF8Encoding($false)))
Write-Output "THIRD-PARTY-NOTICES.md: $($crates.Count) crates"
