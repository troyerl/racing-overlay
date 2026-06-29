; Inno Setup script for GridGlance.
; Compiled by the CI release workflow:
;   ISCC /DMyAppVersion=1.0.123 installer\gridglance.iss
; Paths are relative to the repo root (SourceDir below).

#define MyAppName "GridGlance"
#define MyAppExeName "GridGlance.exe"
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
; Per-user install (no admin prompt); keeps the app folder writable and makes
; in-app updates painless.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Relative to this .iss file's folder (installer/), so ".." is the repo root.
SourceDir=..
OutputDir=installer_output
OutputBaseFilename=GridGlance-Setup-{#MyAppVersion}
SetupIconFile=assets\app.ico
; Icon shown in "Apps & features" / "Add or remove programs".
UninstallDisplayIcon={app}\{#MyAppExeName}
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
Source: "dist\GridGlance\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\GridGlance"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall GridGlance"; Filename: "{uninstallexe}"
Name: "{userdesktop}\GridGlance"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch GridGlance"; Flags: nowait postinstall skipifsilent
