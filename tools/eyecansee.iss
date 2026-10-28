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

[UninstallDelete]
; Components the app downloaded on request (the video engine): gone with the program. Settings
; (%APPDATA%\EyeCanSee) and the thumbnail cache (%LOCALAPPDATA%\EyeCanSee\thumbs.db) stay.
Type: filesandordirs; Name: "{localappdata}\EyeCanSee\components"

[Run]
Filename: "{app}\eyecansee.exe"; Description: "{cm:LaunchProgram,EyeCanSee}"; Flags: nowait postinstall skipifsilent
