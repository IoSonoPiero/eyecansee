; Windows installer, built by .github/workflows/release.yml with Inno Setup:
;   ISCC /DAppVersion=<version> /DPkg=<folder with eyecansee.exe and the licences> tools\eyecansee.iss
; Installs for the current user (no administrator) unless "all users" is chosen at the start.
; Settings and thumbnails (%APPDATA%, %LOCALAPPDATA%) stay after uninstalling.

[Setup]
; Never change AppId: it is how Windows recognises an update of the same program.
AppId={{26C8D984-C1DA-4C83-BE57-0DACFF7F2232}
AppName=EyeCanSee
AppVersion={#AppVersion}
AppPublisher=Pierluigi Montinaro
AppComments=A viewer and manager for photos, videos, 3D models and CAD drawings
AppPublisherURL=https://github.com/IoSonoPiero/eyecansee
AppSupportURL=https://github.com/IoSonoPiero/eyecansee/issues
AppUpdatesURL=https://github.com/IoSonoPiero/eyecansee/releases
DefaultDirName={autopf}\EyeCanSee
DefaultGroupName=EyeCanSee
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
SetupIconFile=..\assets\eyecansee.ico
UninstallDisplayIcon={app}\eyecansee.exe
OutputBaseFilename=EyeCanSee-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Pkg}\eyecansee.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Pkg}\LICENSE.txt"; DestDir: "{app}"
Source: "{#Pkg}\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}"
; ImageMagick: its DLLs, configuration files and coders (modules\) next to the exe.
Source: "{#Pkg}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Pkg}\*.xml"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Pkg}\sRGB.icc"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Pkg}\modules\*"; DestDir: "{app}\modules"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\EyeCanSee"; Filename: "{app}\eyecansee.exe"
Name: "{autodesktop}\EyeCanSee"; Filename: "{app}\eyecansee.exe"; Tasks: desktopicon

[Registry]
; "Open with" for images: EyeCanSee is offered, the default program stays the user's choice.
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Image"; ValueType: string; ValueData: "EyeCanSee"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Image"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "EyeCanSee"
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Image\DefaultIcon"; ValueType: string; ValueData: "{app}\eyecansee.exe,0"
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Image\shell\open\command"; ValueType: string; ValueData: """{app}\eyecansee.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpe\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jfif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.ico\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.heic\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.heif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.hif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.avif\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jxl\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.svg\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.svgz\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.tga\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.exr\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.hdr\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.qoi\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.pnm\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.pbm\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.pgm\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.ppm\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.psd\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jp2\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.dds\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.dng\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.cr2\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.cr3\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.nef\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.arw\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.raf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.orf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.rw2\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
; 3D models (`model.rs`).
Root: HKA; Subkey: "Software\Classes\.stl\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.obj\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.ply\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.glb\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gltf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.3mf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.ifc\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.fbx\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.dae\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.zae\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.3ds\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.off\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.amf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.wrl\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.wrz\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.x3d\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.x3dv\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.x3dz\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.x3dvz\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
; Solids of mechanical CAD programs, read by the CAD engine (`occt.rs`).
Root: HKA; Subkey: "Software\Classes\.step\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.stp\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.iges\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.igs\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.brep\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.brp\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
; CAD drawings (`cad.rs`).
Root: HKA; Subkey: "Software\Classes\.dxf\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.dwg\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.plt\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.hpgl\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.hpg\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.hgl\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Image"; ValueData: ""; Flags: uninsdeletevalue
; "Open with" for videos, the same way: offered, never the default.
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Video"; ValueType: string; ValueData: "EyeCanSee"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Video"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "EyeCanSee"
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Video\DefaultIcon"; ValueType: string; ValueData: "{app}\eyecansee.exe,0"
Root: HKA; Subkey: "Software\Classes\EyeCanSee.Video\shell\open\command"; ValueType: string; ValueData: """{app}\eyecansee.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.mp4\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Video"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mkv\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Video"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mov\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Video"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.webm\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Video"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.avi\OpenWithProgids"; ValueType: string; ValueName: "EyeCanSee.Video"; ValueData: ""; Flags: uninsdeletevalue
; Explorer keeps the name of every program it has seen (MuiCache) and never reads it again: copies
; installed before the exe was named "EyeCanSee" showed "eyecansee" in "Open with" even after
; updating. Removed at every install and update; Windows takes the name from the exe again.
Root: HKCU; Subkey: "Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"; ValueType: none; ValueName: "{app}\eyecansee.exe.FriendlyAppName"; Flags: deletevalue

[UninstallDelete]
; Components the app downloaded on request (the video engine): gone with the program. Settings
; (%APPDATA%\EyeCanSee) and the thumbnail cache (%LOCALAPPDATA%\EyeCanSee\thumbs.db) stay.
Type: filesandordirs; Name: "{localappdata}\EyeCanSee\components"

[Run]
Filename: "{app}\eyecansee.exe"; Description: "{cm:LaunchProgram,EyeCanSee}"; Flags: nowait postinstall skipifsilent
