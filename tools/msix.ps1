# The Microsoft Store package (MSIX) from the folder the zip is made of (pkg\ in release.yml).
# Unsigned: the Store signs what it publishes. To install it locally it must be signed with a
# certificate whose subject is the manifest's Publisher.
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
# The logos in every size and theme (tools/msix-logos.sh); the manifest names the plain ones.
Copy-Item -Recurse "$here\msix\Assets" "$layout\Assets"

# The installer's "Open with" extensions: one list for both.
$types = Select-String -Path "$here\eyecansee.iss" -Pattern '\\(\.[a-z0-9]+)\\OpenWithProgids' |
    ForEach-Object { $_.Matches[0].Groups[1].Value }
if ($types.Count -lt 10) { throw "file types not found in eyecansee.iss" }
$lines = ($types | ForEach-Object { "              <uap:FileType>$_</uap:FileType>" }) -join "`r`n"
# The Store wants four numbers, the last one 0.
$manifest = (Get-Content -Raw "$here\msix\AppxManifest.xml").Replace('@VERSION@', "$Version.0").Replace('@FILETYPES@', $lines)
[IO.File]::WriteAllText("$layout\AppxManifest.xml", $manifest)

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object FullName | Select-Object -Last 1
if (-not $sdk) { throw "makeappx.exe not found (Windows SDK)" }
$makeappx = $sdk.FullName
$makepri = Join-Path $sdk.DirectoryName makepri.exe

# resources.pri: without it Windows does not see the qualified logo names (targetsize, altform)
# and shrinks the plain 44 px logo onto a plate in Start and on the taskbar.
$priconfig = "$layout.priconfig.xml"
& $makepri createconfig /cf $priconfig /dq en-US /pv 10.0.0 /o
if ($LASTEXITCODE) { throw "makepri createconfig failed: $LASTEXITCODE" }
& $makepri new /pr $layout /cf $priconfig /mn "$layout\AppxManifest.xml" /of "$layout\resources.pri" /o
if ($LASTEXITCODE) { throw "makepri new failed: $LASTEXITCODE" }
Remove-Item $priconfig
& $makeappx pack /o /d $layout /p $Out
if ($LASTEXITCODE) { throw "makeappx failed: $LASTEXITCODE" }
Remove-Item -Recurse -Force $layout
