# Installs the private test copy of EyeCanSee for the current user, next to the published one:
# unpacks the zip made by pack-windows.ps1 into %LOCALAPPDATA%\EyeCanSee-prova and puts the
# shortcut "EyeCanSee prova" (red icon) on the Desktop. Run it again with a newer zip to update.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File install-windows.ps1 -Zip <zip>
# Note: the test copy keeps its thumbnails, components and kept edits in %LOCALAPPDATA%\eyecansee-prova,
# the same folder (Windows does not tell case apart): never delete it to update.
param([Parameter(Mandatory = $true)][string]$Zip)
$ErrorActionPreference = "Stop"
$dest = Join-Path $env:LOCALAPPDATA "EyeCanSee-prova"
New-Item -ItemType Directory $dest -Force | Out-Null
Expand-Archive -Path $Zip -DestinationPath $dest -Force
$exe = Join-Path $dest "eyecansee.exe"
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Desktop")) "EyeCanSee prova.lnk"))
$link.TargetPath = $exe
$link.WorkingDirectory = $dest
$link.IconLocation = "$exe,0"
$link.Save()
Get-Content (Join-Path $dest "BUILD.txt")
Write-Output "Installed in $dest"
