; Inno Setup script for Racing Overlay.
; Compiled by the CI release workflow:
;   ISCC /DMyAppVersion=1.0.123 installer\racing_overlay.iss
; Paths are relative to the repo root (SourceDir below).

#define MyAppName "Racing Overlay"
#define MyAppExeName "Racing Overlay.exe"
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

[Setup]
AppId={{8B2F1E64-7C3A-4D2E-9E51-2B7A1C6D5F30}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Racing Overlay
DefaultDirName={autopf}\Racing Overlay
DefaultGroupName=Racing Overlay
DisableProgramGroupPage=yes
; Per-user install (no admin prompt); keeps the app folder writable and makes
; in-app updates painless.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Relative to this .iss file's folder (installer/), so ".." is the repo root.
SourceDir=..
OutputDir=installer_output
OutputBaseFilename=Racing-Overlay-Setup-{#MyAppVersion}
SetupIconFile=assets\app.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
; The whole PyInstaller onedir output.
Source: "dist\Racing Overlay\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\Racing Overlay"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall Racing Overlay"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Racing Overlay"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch Racing Overlay"; Flags: nowait postinstall skipifsilent
