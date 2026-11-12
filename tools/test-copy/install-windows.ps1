# Installs the private test copy of EyeCanSee for the current user, next to the published one:
# unpacks the zip made by pack-windows.ps1 into %LOCALAPPDATA%\Programs\EyeCanSee-prova (like the
# installer of the published copy) and puts the shortcut "EyeCanSee prova" (red icon) on the
# Desktop. Run it again with a newer zip to update: the program folder is replaced, the data
# (%APPDATA%\eyecansee-prova, %LOCALAPPDATA%\eyecansee-prova) is another folder and stays.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File install-windows.ps1 -Zip <zip>
param([Parameter(Mandatory = $true)][string]$Zip)
$ErrorActionPreference = "Stop"
$dest = Join-Path $env:LOCALAPPDATA "Programs\EyeCanSee-prova"
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
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
