# The CAD engine for Windows, as tools/build-occt-linux.sh does for Linux: Open CASCADE Technology
# from its official sources, only the toolkits for reading STEP and IGES (with colours and
# assemblies) and for turning surfaces into triangles, statically with the static C runtime (/MT,
# as EyeCanSee), linked with tools/occt/ecs_occt.cpp into one ecs_occt.dll.
# Needs the Visual Studio 2022 Build Tools and CMake. Run from the repository's folder:
#   powershell -ExecutionPolicy Bypass -File tools\build-occt-windows.ps1
# Output: target\occt-windows\occt-windows-x64-<version>-<build>.tar.gz
$ErrorActionPreference = "Stop"
$Occt = "8.0.1"; $Tag = "V8.0.1"; $Build = 1
$Root = (Get-Location).Path
$B = Join-Path $env:TEMP "eyecansee-occt-$Occt"
$P = Join-Path $B "prefix"
New-Item -ItemType Directory -Force $B, "$Root\target\occt-windows" | Out-Null

# The compiler's environment (cl, link, lib) from the Build Tools.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
cmd /c "`"$vs\VC\Auxiliary\Build\vcvars64.bat`" >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2]) }
}

$tgz = Join-Path $B "occt.tar.gz"
if (-not (Test-Path $tgz)) {
    Invoke-WebRequest "https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/$Tag.tar.gz" -OutFile $tgz
}
tar -xzf $tgz -C $B
$Src = (Get-ChildItem $B -Directory -Filter "OCCT-*").FullName

cmake -S $Src -B "$B\build" -G "Visual Studio 17 2022" -A x64 -DBUILD_LIBRARY_TYPE=Static `
    -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DINSTALL_DIR="$P" `
    -DBUILD_MODULE_FoundationClasses=ON -DBUILD_MODULE_ModelingData=ON -DBUILD_MODULE_ModelingAlgorithms=ON `
    -DBUILD_MODULE_DataExchange=OFF -DBUILD_MODULE_Visualization=OFF -DBUILD_MODULE_ApplicationFramework=OFF `
    -DBUILD_MODULE_Draw=OFF `
    "-DBUILD_ADDITIONAL_TOOLKITS=TKDE TKXSBase TKDESTEP TKDEIGES TKXCAF TKRWMesh TKService TKV3d TKVCAF TKCDF TKLCAF TKCAF" `
    -DUSE_FREETYPE=OFF -DUSE_OPENGL=OFF -DUSE_GLES2=OFF -DUSE_D3D=OFF -DUSE_TK=OFF -DUSE_FREEIMAGE=OFF `
    -DUSE_RAPIDJSON=OFF -DUSE_DRACO=OFF -DUSE_TBB=OFF -DUSE_VTK=OFF -DUSE_FFMPEG=OFF -DUSE_OPENVR=OFF `
    -DBUILD_DOC_Overview=OFF | Out-File "$B\cmake.log"
if ($LASTEXITCODE) { Get-Content "$B\cmake.log" -Tail 30; exit 1 }
cmake --build "$B\build" --config Release --target INSTALL -- /m /v:minimal | Out-File "$B\build.log"
if ($LASTEXITCODE) { Get-Content "$B\build.log" -Tail 40; exit 1 }

$Out = Join-Path $B "out"
New-Item -ItemType Directory -Force $Out | Out-Null
$libs = (Get-ChildItem "$P\win64\vc14\lib*", "$P\lib*" -Recurse -Filter "TK*.lib" -ErrorAction SilentlyContinue).FullName
cl /nologo /LD /MT /O2 /std:c++17 /EHsc /DOCCT_STATIC_BUILD "/I$P\inc" "/I$P\include\opencascade" `
    "$Root\tools\occt\ecs_occt.cpp" "/Fe:$Out\ecs_occt.dll" "/Fo:$B\ecs_occt.obj" `
    /link /OPT:REF /OPT:ICF $libs user32.lib advapi32.lib wsock32.lib
if ($LASTEXITCODE) { exit 1 }
Remove-Item "$Out\ecs_occt.lib", "$Out\ecs_occt.exp" -ErrorAction SilentlyContinue
dumpbin /exports "$Out\ecs_occt.dll" | Select-String "ecs_"
dumpbin /dependents "$Out\ecs_occt.dll" | Select-String "\.dll"

Copy-Item "$Src\LICENSE_LGPL_21.txt" "$Out\LICENSE.occt-LGPL-2.1"
Copy-Item "$Src\OCCT_LGPL_EXCEPTION.txt" "$Out\LICENSE.occt-exception"
@"
Open CASCADE Technology $Occt, built by EyeCanSee's tools/build-occt-windows.ps1 from
https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/$Tag.tar.gz
with tools/occt/ecs_occt.cpp (https://github.com/IoSonoPiero/eyecansee).
"@ | Out-File -Encoding ascii "$Out\SOURCE.txt"
$Name = "occt-windows-x64-$Occt-$Build.tar.gz"
tar -czf "$Root\target\occt-windows\$Name" -C $Out .
Get-Item "$Root\target\occt-windows\$Name" | Format-List Name, Length
(Get-FileHash "$Root\target\occt-windows\$Name" -Algorithm SHA256).Hash.ToLower()
