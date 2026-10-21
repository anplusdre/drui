; Inno Setup script for the Windows installer.
; Build the release first:   cargo build --release
; Then compile this file:    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" tools\installer\drui.iss
; The installer is written to dist\drui-setup-<version>.exe
#define AppVersion "0.1.0"

[Setup]
AppId={{6F0D2B1E-4C7A-4B7E-9D3A-5E2C8A1F7B40}
AppName=drui
AppVersion={#AppVersion}
AppPublisher=anplusdre
AppPublisherURL=https://github.com/anplusdre/drui
DefaultDirName={autopf}\drui
DefaultGroupName=drui
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
OutputBaseFilename=drui-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
UninstallDisplayName=drui

[Files]
Source: "..\..\target\release\drui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\drui"; Filename: "{app}\drui.exe"; WorkingDir: "{userdocs}"
Name: "{group}\Uninstall drui"; Filename: "{uninstallexe}"
Name: "{autodesktop}\drui"; Filename: "{app}\drui.exe"; WorkingDir: "{userdocs}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\drui.exe"; WorkingDir: "{userdocs}"; Description: "Start drui"; Flags: nowait postinstall skipifsilent
