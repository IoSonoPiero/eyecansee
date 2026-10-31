# The Microsoft Store package (MSIX) from the folder the zip is made of (pkg\ in release.yml).
# Unsigned: the Store signs what it publishes. To install it locally it must be signed with a
# certificate whose subject is the manifest's Publisher (see notes, WINDOWS.md).
#   .\tools\msix.ps1 -Pkg pkg -Version 0.72.0 -Out EyeCanSee-0.72.0-windows-x64.msix
param(
    [Parameter(Mandatory)] [string] $Pkg,
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot

$layout = Join-Path ([IO.Path]::GetTempPath()) "eyecansee-msix-$PID"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
Copy-Item -Recurse $Pkg $layout
New-Item -ItemType Directory "$layout\Assets" | Out-Null
Copy-Item "$here\msix\logo-44.png" "$layout\Assets\Square44x44Logo.png"
Copy-Item "$here\msix\logo-50.png" "$layout\Assets\StoreLogo.png"
Copy-Item "$here\msix\logo-150.png" "$layout\Assets\Square150x150Logo.png"

# The installer's "Open with" extensions: one list for both.
$types = Select-String -Path "$here\eyecansee.iss" -Pattern '\\(\.[a-z0-9]+)\\OpenWithProgids' |
    ForEach-Object { $_.Matches[0].Groups[1].Value }
if ($types.Count -lt 10) { throw "file types not found in eyecansee.iss" }
$lines = ($types | ForEach-Object { "              <uap:FileType>$_</uap:FileType>" }) -join "`r`n"
# The Store wants four numbers, the last one 0.
$manifest = (Get-Content -Raw "$here\msix\AppxManifest.xml").Replace('@VERSION@', "$Version.0").Replace('@FILETYPES@', $lines)
[IO.File]::WriteAllText("$layout\AppxManifest.xml", $manifest)

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object FullName | Select-Object -Last 1
if (-not $makeappx) { throw "makeappx.exe not found (Windows SDK)" }
& $makeappx.FullName pack /o /d $layout /p $Out
if ($LASTEXITCODE) { throw "makeappx failed: $LASTEXITCODE" }
Remove-Item -Recurse -Force $layout
