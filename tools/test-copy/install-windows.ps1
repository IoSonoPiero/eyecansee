# Installs the private test copy of EyeCanSee for the current user, next to the published one:
# unpacks the zip made by pack-windows.ps1 into %LOCALAPPDATA%\Programs\EyeCanSee-prova (like the
# installer of the published copy) and puts the shortcut "EyeCanSee prova" (red icon) on the
# Desktop. Run it again with a newer zip to update: the program folder is replaced, the data
# (%APPDATA%\eyecansee-prova, %LOCALAPPDATA%\eyecansee-prova) is another folder and stays.
# The shortcut starts it through avvia.ps1, which always writes the log (Piero, 21/11/2026: a crash
# during the tests must leave its trace): one file per start in %LOCALAPPDATA%\eyecansee-prova\registri,
# with the standard error (a panic of the main thread) next to it; the last 30 starts are kept.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File install-windows.ps1 -Zip <zip>
param([Parameter(Mandatory = $true)][string]$Zip)
$ErrorActionPreference = "Stop"
$dest = Join-Path $env:LOCALAPPDATA "Programs\EyeCanSee-prova"
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
New-Item -ItemType Directory $dest -Force | Out-Null
Expand-Archive -Path $Zip -DestinationPath $dest -Force
$exe = Join-Path $dest "eyecansee.exe"
$run = @'
$logs = Join-Path $env:LOCALAPPDATA "eyecansee-prova\registri"
New-Item -ItemType Directory -Force $logs | Out-Null
Get-ChildItem $logs -Filter "registro-*.log" | Sort-Object LastWriteTime -Descending | Select-Object -Skip 29 | ForEach-Object { Remove-Item $_.FullName -Force; Remove-Item "$($_.FullName).stderr" -Force -ErrorAction SilentlyContinue }
$log = Join-Path $logs ("registro-" + (Get-Date -Format "yyyy-MM-dd_HHmmss") + ".log")
$env:EYECANSEE_LOG = $log
$env:RUST_BACKTRACE = "1"
$exe = Join-Path $PSScriptRoot "eyecansee.exe"
if ($args.Count -gt 0) { Start-Process -FilePath $exe -ArgumentList $args -RedirectStandardError "$log.stderr" } else { Start-Process -FilePath $exe -RedirectStandardError "$log.stderr" }
'@
Set-Content -Path (Join-Path $dest "avvia.ps1") -Value $run -Encoding ASCII
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Desktop")) "EyeCanSee prova.lnk"))
$link.TargetPath = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
$link.Arguments = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$(Join-Path $dest 'avvia.ps1')`""
$link.WorkingDirectory = $dest
$link.IconLocation = "$exe,0"
$link.Save()
Get-Content (Join-Path $dest "BUILD.txt")
Write-Output "Installed in $dest"
