# Builds the private test copy for Windows (red icon, its own folders, no updates:
# src/test_copy.rs) and packs it like the release zip, with BUILD.txt (commit and date) and
# install-windows.ps1: EyeCanSee-prova-<commit>.zip in the repository folder.
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
