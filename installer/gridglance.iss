; Inno Setup script for GridGlance (Rust-only).
; Compiled by the CI release workflow:
;   ISCC /DMyAppVersion=1.0.123 installer\gridglance.iss
; Paths are relative to the repo root (SourceDir below).

#define MyAppName "GridGlance"
#define MyAppExeName "gridglance-overlay.exe"
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

[Setup]
AppId={{8B2F1E64-7C3A-4D2E-9E51-2B7A1C6D5F30}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=GridGlance
DefaultDirName={autopf}\GridGlance
DefaultGroupName=GridGlance
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
SourceDir=..
OutputDir=installer_output
OutputBaseFilename=GridGlance-Setup-{#MyAppVersion}
SetupIconFile=assets\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
; Build: cargo build -p gridglance-overlay --release
Source: "target\release\gridglance-overlay.exe"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
Source: "target\release\gridglance-overlay.exe"; DestDir: "{app}"; DestName: "GridGlance.exe"; Flags: ignoreversion
Source: "assets\app.ico"; DestDir: "{app}"; DestName: "app.ico"; Flags: ignoreversion
Source: "assets\app.ico"; DestDir: "{app}\assets"; DestName: "app.ico"; Flags: ignoreversion

[Icons]
Name: "{group}\GridGlance"; Filename: "{app}\GridGlance.exe"
Name: "{group}\Uninstall GridGlance"; Filename: "{uninstallexe}"
Name: "{userdesktop}\GridGlance"; Filename: "{app}\GridGlance.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\GridGlance.exe"; Description: "Launch GridGlance"; Flags: nowait postinstall
