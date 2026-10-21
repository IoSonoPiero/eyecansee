# Collects name, version, licence and licence texts of every crate linked into EyeCanSee, for the
# THIRD-PARTY-LICENSES.txt file shipped with each release. Crates without a licence file are covered
# by the standard texts from the SPDX License List, appended at the end.
# Usage: .\tools\third-party-licenses.ps1 -Project . -Out THIRD-PARTY-LICENSES.txt
param([string]$Project, [string]$Out, [string]$Target = "x86_64-pc-windows-msvc", [string]$Features = "--no-default-features")
Push-Location $Project
$json = cargo metadata --format-version 1 --filter-platform $Target $Features.Split(' ') 2>$null | ConvertFrom-Json
Pop-Location
$ids = $json.resolve.nodes | ForEach-Object { $_.id }
$pkgs = $json.packages | Where-Object { $ids -contains $_.id -and $_.name -ne "eyecansee" } | Sort-Object name, version
$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine("Third-party software included in EyeCanSee")
[void]$sb.AppendLine("==========================================")
[void]$sb.AppendLine("")
[void]$sb.AppendLine("EyeCanSee is built with the following libraries. Their licences and copyright notices follow.")
[void]$sb.AppendLine("")
foreach ($p in $pkgs) { [void]$sb.AppendLine(("- {0} {1} ({2})" -f $p.name, $p.version, $p.license)) }
$missing = @()
foreach ($p in $pkgs) {
    $dir = Split-Path $p.manifest_path
    $files = @(Get-ChildItem -Path $dir -File) | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)' }
    # Bundled C libraries keep their licence in a subfolder (libjpeg-turbo, SQLite).
    $files = @($files) + @(Get-ChildItem -Path $dir -Recurse -File -Depth 2 -ErrorAction SilentlyContinue |
        Where-Object { $_.DirectoryName -ne $dir -and $_.Name -match '^(LICEN[CS]E|COPYING|README\.ijg)' -and $_.FullName -notmatch '\\(tests?|examples|target)\\' })
    $files = @($files | Where-Object { $_ })
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("=" * 78)
    [void]$sb.AppendLine(("{0} {1}  -  {2}" -f $p.name, $p.version, $p.license))
    if ($p.repository) { [void]$sb.AppendLine($p.repository) }
    [void]$sb.AppendLine("=" * 78)
    if ($p.authors) { [void]$sb.AppendLine("Copyright: " + ($p.authors -join ", ")) }
    if (-not $files) { $missing += "$($p.name) $($p.version) ($($p.license))"; [void]$sb.AppendLine("(no licence file in the package: the full text of $($p.license) is at the end of this file)"); continue }
    foreach ($f in $files) {
        [void]$sb.AppendLine("")
        [void]$sb.AppendLine("--- " + $f.FullName.Substring($dir.Length + 1))
        [void]$sb.AppendLine((Get-Content -Raw -Encoding utf8 $f.FullName))
    }
}
[void]$sb.AppendLine("")
[void]$sb.AppendLine("=" * 78)
[void]$sb.AppendLine("Standard licence texts (from the SPDX License List)")
[void]$sb.AppendLine("=" * 78)
foreach ($id in "MIT", "Apache-2.0", "BSD-3-Clause", "BSL-1.0", "OFL-1.1", "Ubuntu-font-1.0") {
    $text = (Invoke-WebRequest -UseBasicParsing "https://raw.githubusercontent.com/spdx/license-list-data/main/text/$id.txt").Content
    [void]$sb.AppendLine("")
    [void]$sb.AppendLine("--- $id")
    [void]$sb.AppendLine($text)
}
[System.IO.File]::WriteAllText($Out, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))
"crates: $($pkgs.Count)"
"without licence file: $($missing.Count)"
$missing
