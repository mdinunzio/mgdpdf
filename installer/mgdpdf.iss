; Inno Setup script for mgdpdf — a per-user Windows installer.
;
; Build prerequisites (run from the repo root):
;   cargo build --release
; Then compile this script:
;   "%LocalAppData%\Programs\Inno Setup 6\ISCC.exe" installer\mgdpdf.iss
; Output: installer\dist\mgdpdf-setup-<version>.exe
;
; Install scope: per-user (no admin). Installs to %LocalAppData%\Programs\mgdpdf,
; adds a Start-menu shortcut, and registers mgdpdf in the PDF "Open with" list
; without hijacking the user's default PDF association.

#define MyAppName "mgdpdf"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Michael DiNunzio"
#define MyAppExeName "mgdpdf.exe"
; Path to the built release binaries, relative to this script.
#define BuildDir "..\target\release"

[Setup]
; A stable GUID identifies the app for upgrades/uninstall. Generated for mgdpdf.
AppId={{8B5F2C7A-3E94-4D21-9A6F-1C7B0E5D8A42}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Per-user install — no admin elevation required.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=dist
OutputBaseFilename=mgdpdf-setup-{#MyAppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
Name: "associatepdf"; Description: "Add {#MyAppName} to the PDF ""Open with"" list"; GroupDescription: "File associations:"

[Files]
; The app exe and the PDFium runtime DLL must sit side by side — our loader
; looks for pdfium.dll in the executable's directory first.
Source: "{#BuildDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\pdfium.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Register a ProgID for mgdpdf and add it to the PDF "Open with" list. This does
; NOT change the user's default PDF handler — it only makes mgdpdf available via
; right-click → Open with. (HKCU keys, consistent with a per-user install.)
Root: HKCU; Subkey: "Software\Classes\mgdpdf.pdf"; ValueType: string; ValueName: ""; ValueData: "PDF Document"; Flags: uninsdeletekey; Tasks: associatepdf
Root: HKCU; Subkey: "Software\Classes\mgdpdf.pdf\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"; Tasks: associatepdf
Root: HKCU; Subkey: "Software\Classes\mgdpdf.pdf\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associatepdf
Root: HKCU; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "mgdpdf.pdf"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associatepdf
; Register the app under "Applications" so it also appears in the generic
; Open-with list with a friendly name.
Root: HKCU; Subkey: "Software\Classes\Applications\{#MyAppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey; Tasks: associatepdf
Root: HKCU; Subkey: "Software\Classes\Applications\{#MyAppExeName}\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""; Tasks: associatepdf

[Run]
; Offer to launch the app after install.
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent
