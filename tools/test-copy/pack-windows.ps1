# Builds the private test copy for Windows (red icon, its own folders, no updates:
# src/test_copy.rs) and packs it like the release zip, licences included, with BUILD.txt (commit
# and date) and install-windows.ps1: EyeCanSee-prova-<commit>.zip in the repository folder.
# Run from the repository in the Windows build machine, with VCPKG_ROOT, IMAGE_MAGICK_DIR and
# LIBCLANG_PATH set (README.md, "Build" for Windows). Never on GitHub Actions (build.rs refuses it there).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")
foreach ($v in [Environment]::GetEnvironmentVariables("User").GetEnumerator()) {
    if ($v.Key -ne "Path") { Set-Item "env:$($v.Key)" $v.Value }
}
$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")

cargo build --locked --profile prova --no-default-features --features heif,magick,test-copy
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$commit = (git rev-parse --short HEAD).Trim()
git diff --quiet HEAD
if ($LASTEXITCODE -ne 0) { $commit = "$commit-dirty" }
$pkg = "target\test-copy-pkg"
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
New-Item -ItemType Directory $pkg | Out-Null
# Licences as in the release zip (release.yml, "Zip"): crates, vcpkg C libraries, ImageMagick.
$licences = "$pkg\THIRD-PARTY-LICENSES.txt"
.\tools\third-party-licenses.ps1 -Project . -Out $licences
$sources = [ordered]@{ libheif = "https://github.com/strukturag/libheif"
                       libde265 = "https://github.com/strukturag/libde265"
                       dav1d = "https://code.videolan.org/videolan/dav1d" }
$bar = "=" * 78
foreach ($p in $sources.Keys) {
    $line = & "$env:VCPKG_ROOT\vcpkg.exe" list $p | Where-Object { $_ -match "^${p}:x64-windows-static\s" }
    $ver = ($line -split '\s+')[1]
    Add-Content $licences "`n$bar`n$p $ver (linked into eyecansee.exe, built with vcpkg)`nSource: $($sources[$p])`n$bar`n"
    Get-Content -Raw "$env:VCPKG_ROOT\installed\x64-windows-static\share\$p\copyright" | Add-Content $licences
}
# IM_VERSION is set on GitHub Actions; on a build machine it is read from magick.exe.
$im = $env:IM_VERSION
if (-not $im) { $im = ((& "$env:IMAGE_MAGICK_DIR\magick.exe" -version | Select-Object -First 1) -split '\s+')[2] }
Add-Content $licences "`n$bar`nImageMagick $im (DLLs and modules\ next to eyecansee.exe, official Windows build)`nSource: https://github.com/ImageMagick/ImageMagick`n$bar`n"
Get-Content -Raw "$env:IMAGE_MAGICK_DIR\NOTICE.txt" | Add-Content $licences
Copy-Item "$env:IMAGE_MAGICK_DIR\*.dll", "$env:IMAGE_MAGICK_DIR\*.xml", "$env:IMAGE_MAGICK_DIR\sRGB.icc" $pkg
Copy-Item "$env:IMAGE_MAGICK_DIR\modules" $pkg -Recurse
Copy-Item target\prova\eyecansee.exe $pkg
Copy-Item LICENSE "$pkg\LICENSE.txt"
Copy-Item tools\test-copy\install-windows.ps1 $pkg
$built = Get-Date -Format "yyyy-MM-dd HH:mm K"
$branch = (git branch --show-current).Trim()
$version = (Get-Content VERSION -Raw).Trim()
"EyeCanSee test copy`r`nVersion: $version`r`nBranch: $branch`r`nCommit: $commit`r`nBuilt: $built`r`n" | Set-Content "$pkg\BUILD.txt" -Encoding ASCII
$zip = "EyeCanSee-prova-$commit.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "$pkg\*" -DestinationPath $zip
Write-Output "Done: $zip"
